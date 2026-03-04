use std::collections::HashMap;

use anyhow::{Context, Result};
use lancedb::query::{ExecutableQuery, QueryBase};
use serde::Serialize;

use super::store::MemoryStore;
use super::types::MemoryType;
use crate::storage::database::Database;

/// A search result from semantic memory.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub id: String,
    pub content: String,
    pub source: String,
    pub score: f32,
    pub memory_type: MemoryType,
}

/// Parameters for hybrid search.
#[derive(Debug, Clone)]
pub struct SearchParams {
    pub limit: usize,
    pub type_filter: Option<MemoryType>,
    pub min_importance: f64,
    pub graph_depth: u8,
}

impl Default for SearchParams {
    fn default() -> Self {
        Self {
            limit: 10,
            type_filter: None,
            min_importance: 0.0,
            graph_depth: 0,
        }
    }
}

impl MemoryStore {
    /// Search memories by semantic similarity (vector only, backward-compatible).
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        self.vector_search(query, limit).await
    }

    /// Pure vector search — internal building block.
    pub(crate) async fn vector_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let query_vec = {
            let mut embedder = self.embedder.lock().await;
            let embeddings = embedder
                .embed(vec![query.to_string()], None)
                .context("embedding failed")?;
            embeddings.into_iter().next().unwrap()
        };

        let table = self
            .db
            .open_table(super::store::TABLE_NAME)
            .execute()
            .await?;

        let results = table
            .query()
            .limit(limit)
            .nearest_to(query_vec.as_slice())
            .context("vector search setup failed")?
            .execute()
            .await
            .context("search execution failed")?;

        use futures::TryStreamExt;
        let batches: Vec<_> = results.try_collect().await?;

        let mut search_results = Vec::new();
        for batch in &batches {
            let id_col = batch
                .column_by_name("id")
                .and_then(|c| c.as_any().downcast_ref::<arrow_array::StringArray>());
            let content_col = batch
                .column_by_name("content")
                .and_then(|c| c.as_any().downcast_ref::<arrow_array::StringArray>());
            let source_col = batch
                .column_by_name("source")
                .and_then(|c| c.as_any().downcast_ref::<arrow_array::StringArray>());
            let dist_col = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<arrow_array::Float32Array>());
            let type_col = batch
                .column_by_name("memory_type")
                .and_then(|c| c.as_any().downcast_ref::<arrow_array::StringArray>());

            if let (Some(ids), Some(contents)) = (id_col, content_col) {
                for i in 0..batch.num_rows() {
                    let mt = type_col
                        .and_then(|t| MemoryType::from_str(t.value(i)))
                        .unwrap_or(MemoryType::Observation);
                    search_results.push(SearchResult {
                        id: ids.value(i).to_string(),
                        content: contents.value(i).to_string(),
                        source: source_col
                            .map(|s| s.value(i).to_string())
                            .unwrap_or_default(),
                        score: dist_col.map(|d| 1.0 - d.value(i)).unwrap_or(0.0),
                        memory_type: mt,
                    });
                }
            }
        }

        Ok(search_results)
    }

    /// Hybrid search: vector + graph traversal + RRF fusion + importance boost.
    pub async fn hybrid_search(
        &self,
        query: &str,
        params: &SearchParams,
        db: &Database,
    ) -> Result<Vec<SearchResult>> {
        let overfetch = params.limit * 3;

        // 1. Vector search with overfetch
        let vector_results = self.vector_search(query, overfetch).await?;

        // 2. Graph expansion from top vector results
        let mut graph_ids: Vec<String> = Vec::new();
        if params.graph_depth > 0 && !vector_results.is_empty() {
            let seed_ids: Vec<&str> = vector_results
                .iter()
                .take(params.limit)
                .map(|r| r.id.as_str())
                .collect();

            let mut seen: std::collections::HashSet<String> = seed_ids.iter().map(|s| s.to_string()).collect();
            let mut frontier: Vec<String> = seed_ids.iter().map(|s| s.to_string()).collect();

            for _depth in 0..params.graph_depth {
                let mut next_frontier = Vec::new();
                for mem_id in &frontier {
                    if let Ok(edges) = db.edges_for(mem_id).await {
                        for edge in edges {
                            let neighbor = if edge.source_id == *mem_id {
                                &edge.target_id
                            } else {
                                &edge.source_id
                            };
                            if seen.insert(neighbor.clone()) {
                                next_frontier.push(neighbor.clone());
                                graph_ids.push(neighbor.clone());
                            }
                        }
                    }
                }
                frontier = next_frontier;
            }
        }

        // 3. Fetch graph-discovered memories
        let graph_results = self.fetch_by_ids(&graph_ids).await?;

        // 4. RRF fusion (k=60)
        let k = 60.0_f32;
        let mut score_map: HashMap<String, (f32, SearchResult)> = HashMap::new();

        // Vector scores: rank-based
        for (rank, result) in vector_results.into_iter().enumerate() {
            let rrf = 1.0 / (k + rank as f32 + 1.0);
            score_map
                .entry(result.id.clone())
                .and_modify(|(s, _)| *s += rrf)
                .or_insert((rrf, result));
        }

        // Graph scores: half-weight
        for (rank, result) in graph_results.into_iter().enumerate() {
            let rrf = 0.5 / (k + rank as f32 + 1.0);
            score_map
                .entry(result.id.clone())
                .and_modify(|(s, _)| *s += rrf)
                .or_insert((rrf, result));
        }

        // 5. Importance boost
        let now = chrono::Utc::now();
        let mut results: Vec<(f32, SearchResult)> = Vec::with_capacity(score_map.len());
        for (_, (mut score, result)) in score_map {
            if let Ok(meta) = db.memory_metadata(&result.id).await {
                let edge_count = db.edge_count(&result.id).await.unwrap_or(0);
                let importance = super::importance::compute_importance(
                    meta.access_count as u64,
                    edge_count as u64,
                    &meta.last_accessed_at,
                    &now,
                );

                if importance < params.min_importance {
                    continue;
                }

                score *= 1.0 + (importance as f32 * 0.1);
            }

            // Type filter
            if let Some(ref filter) = params.type_filter {
                if result.memory_type != *filter {
                    continue;
                }
            }

            results.push((score, result));
        }

        // Sort by score descending
        results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(params.limit);

        // Record access for returned results
        let final_results: Vec<SearchResult> = results
            .into_iter()
            .map(|(score, mut r)| {
                r.score = score;
                r
            })
            .collect();

        for r in &final_results {
            let _ = db.record_memory_access(&r.id).await;
        }

        Ok(final_results)
    }
}
