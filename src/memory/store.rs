use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_array::types::Float32Type;
use arrow_array::{FixedSizeListArray, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use lancedb::Connection;
use tokio::sync::Mutex;

use super::types::MemoryType;

pub(crate) const TABLE_NAME: &str = "memories";
const EMBEDDING_DIM: i32 = 384; // bge-small-en-v1.5

/// Vector memory store using fastembed + LanceDB.
pub struct MemoryStore {
    pub(crate) db: Connection,
    pub(crate) embedder: Arc<Mutex<fastembed::TextEmbedding>>,
}

fn memory_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
        Field::new("source", DataType::Utf8, true),
        Field::new("memory_type", DataType::Utf8, false),
        Field::new("created_at", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                EMBEDDING_DIM,
            ),
            true,
        ),
    ]))
}

impl MemoryStore {
    pub async fn open(store_path: &Path) -> Result<Self> {
        std::fs::create_dir_all(store_path)?;

        let db = lancedb::connect(store_path.to_string_lossy().as_ref())
            .execute()
            .await
            .context("failed to open LanceDB")?;

        let embedder = fastembed::TextEmbedding::try_new(
            fastembed::InitOptions::new(fastembed::EmbeddingModel::BGESmallENV15)
                .with_show_download_progress(true),
        )
        .context("failed to init embedding model")?;

        let store = Self {
            db,
            embedder: Arc::new(Mutex::new(embedder)),
        };

        store.ensure_table().await?;
        Ok(store)
    }

    async fn ensure_table(&self) -> Result<()> {
        let tables = self.db.table_names().execute().await?;
        if !tables.contains(&TABLE_NAME.to_string()) {
            self.db
                .create_empty_table(TABLE_NAME, memory_schema())
                .execute()
                .await
                .context("failed to create memories table")?;
        }
        Ok(())
    }

    /// Store a memory with its embedding and type.
    pub async fn store(
        &self,
        id: &str,
        content: &str,
        source: Option<&str>,
        memory_type: MemoryType,
    ) -> Result<()> {
        let embeddings = {
            let mut embedder = self.embedder.lock().await;
            embedder
                .embed(vec![content.to_string()], None)
                .context("embedding failed")?
        };
        let vector = &embeddings[0];

        let schema = memory_schema();
        let now = chrono::Utc::now().to_rfc3339();

        let id_arr = StringArray::from(vec![id]);
        let content_arr = StringArray::from(vec![content]);
        let source_arr = StringArray::from(vec![source.unwrap_or("")]);
        let type_arr = StringArray::from(vec![memory_type.as_str()]);
        let created_arr = StringArray::from(vec![now.as_str()]);
        let vector_arr = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            vec![Some(vector.iter().map(|v| Some(*v)).collect::<Vec<_>>())],
            EMBEDDING_DIM,
        );

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(id_arr),
                Arc::new(content_arr),
                Arc::new(source_arr),
                Arc::new(type_arr),
                Arc::new(created_arr),
                Arc::new(vector_arr),
            ],
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);

        let table = self.db.open_table(TABLE_NAME).execute().await?;
        table
            .add(Box::new(batches))
            .execute()
            .await
            .context("failed to add memory")?;

        Ok(())
    }

    /// Update a memory: delete old row, re-embed content, insert new row with same ID.
    pub async fn update(
        &self,
        id: &str,
        content: &str,
        source: Option<&str>,
        memory_type: MemoryType,
    ) -> Result<()> {
        let table = self.db.open_table(TABLE_NAME).execute().await?;
        table
            .delete(&format!("id = '{}'", id.replace('\'', "''")))
            .await
            .context("failed to delete old memory for update")?;

        self.store(id, content, source, memory_type).await
    }

    /// Delete a memory by ID from LanceDB.
    pub async fn delete(&self, id: &str) -> Result<()> {
        let table = self.db.open_table(TABLE_NAME).execute().await?;
        table
            .delete(&format!("id = '{}'", id.replace('\'', "''")))
            .await
            .context("failed to delete memory")?;
        Ok(())
    }

    /// Fetch memories by their IDs from LanceDB.
    pub async fn fetch_by_ids(&self, ids: &[String]) -> Result<Vec<super::search::SearchResult>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let table = self.db.open_table(TABLE_NAME).execute().await?;

        let quoted: Vec<String> = ids.iter().map(|id| format!("'{}'", id.replace('\'', "''"))).collect();
        let filter = format!("id IN ({})", quoted.join(", "));

        use lancedb::query::{ExecutableQuery, QueryBase};
        let results = table
            .query()
            .only_if(filter)
            .execute()
            .await
            .context("fetch_by_ids failed")?;

        use futures::TryStreamExt;
        let batches: Vec<_> = results.try_collect().await?;

        let mut out = Vec::new();
        for batch in &batches {
            let id_col = batch
                .column_by_name("id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let content_col = batch
                .column_by_name("content")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let source_col = batch
                .column_by_name("source")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let type_col = batch
                .column_by_name("memory_type")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            if let (Some(ids), Some(contents)) = (id_col, content_col) {
                for i in 0..batch.num_rows() {
                    let mt = type_col
                        .and_then(|t| MemoryType::from_str(t.value(i)))
                        .unwrap_or(MemoryType::Observation);
                    out.push(super::search::SearchResult {
                        id: ids.value(i).to_string(),
                        content: contents.value(i).to_string(),
                        source: source_col
                            .map(|s| s.value(i).to_string())
                            .unwrap_or_default(),
                        score: 0.0,
                        memory_type: mt,
                    });
                }
            }
        }
        Ok(out)
    }
}
