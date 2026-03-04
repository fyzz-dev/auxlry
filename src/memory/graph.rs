use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::storage::database::Database;

/// Types of relationships between memories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    RelatedTo,
    Supersedes,
    Contradicts,
    CausedBy,
    PartOf,
}

impl EdgeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RelatedTo => "related_to",
            Self::Supersedes => "supersedes",
            Self::Contradicts => "contradicts",
            Self::CausedBy => "caused_by",
            Self::PartOf => "part_of",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "related_to" => Some(Self::RelatedTo),
            "supersedes" => Some(Self::Supersedes),
            "contradicts" => Some(Self::Contradicts),
            "caused_by" => Some(Self::CausedBy),
            "part_of" => Some(Self::PartOf),
            _ => None,
        }
    }
}

impl std::fmt::Display for EdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A graph edge between two memories.
#[derive(Debug, Clone, Serialize)]
pub struct MemoryEdge {
    pub id: i64,
    pub source_id: String,
    pub target_id: String,
    pub relation_type: EdgeType,
    pub weight: f64,
    pub created_at: String,
}

/// Memory metadata from SQLite.
#[derive(Debug, Clone)]
pub struct MemoryMetadata {
    pub id: String,
    pub memory_type: String,
    pub access_count: i64,
    pub last_accessed_at: String,
    pub created_at: String,
}

impl Database {
    /// Create or update an edge between two memories.
    pub async fn create_edge(
        &self,
        source_id: &str,
        target_id: &str,
        relation_type: EdgeType,
        weight: f64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO memory_edges (source_id, target_id, relation_type, weight) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT(source_id, target_id, relation_type) DO UPDATE SET weight = excluded.weight",
        )
        .bind(source_id)
        .bind(target_id)
        .bind(relation_type.as_str())
        .bind(weight)
        .execute(self.pool())
        .await
        .context("failed to create edge")?;
        Ok(())
    }

    /// Get all edges touching a memory (as source or target).
    pub async fn edges_for(&self, memory_id: &str) -> Result<Vec<MemoryEdge>> {
        let rows = sqlx::query(
            "SELECT id, source_id, target_id, relation_type, weight, created_at \
             FROM memory_edges WHERE source_id = ? OR target_id = ?",
        )
        .bind(memory_id)
        .bind(memory_id)
        .fetch_all(self.pool())
        .await?;

        let mut edges = Vec::with_capacity(rows.len());
        for row in rows {
            let rt: String = row.get("relation_type");
            edges.push(MemoryEdge {
                id: row.get("id"),
                source_id: row.get("source_id"),
                target_id: row.get("target_id"),
                relation_type: EdgeType::from_str(&rt).unwrap_or(EdgeType::RelatedTo),
                weight: row.get("weight"),
                created_at: row.get("created_at"),
            });
        }
        Ok(edges)
    }

    /// Count edges touching a memory.
    pub async fn edge_count(&self, memory_id: &str) -> Result<i64> {
        let row = sqlx::query(
            "SELECT COUNT(*) as cnt FROM memory_edges WHERE source_id = ? OR target_id = ?",
        )
        .bind(memory_id)
        .bind(memory_id)
        .fetch_one(self.pool())
        .await?;
        Ok(row.get("cnt"))
    }

    /// Record an access to a memory (bump count + update timestamp).
    pub async fn record_memory_access(&self, memory_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE memory_metadata SET access_count = access_count + 1, \
             last_accessed_at = datetime('now') WHERE id = ?",
        )
        .bind(memory_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Initialize metadata for a new memory.
    pub async fn init_memory_metadata(
        &self,
        memory_id: &str,
        memory_type: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO memory_metadata (id, memory_type) VALUES (?, ?)",
        )
        .bind(memory_id)
        .bind(memory_type)
        .execute(self.pool())
        .await
        .context("failed to init memory metadata")?;
        Ok(())
    }

    /// Get metadata for a memory.
    pub async fn memory_metadata(&self, memory_id: &str) -> Result<MemoryMetadata> {
        let row = sqlx::query(
            "SELECT id, memory_type, access_count, last_accessed_at, created_at \
             FROM memory_metadata WHERE id = ?",
        )
        .bind(memory_id)
        .fetch_one(self.pool())
        .await
        .context("memory metadata not found")?;

        Ok(MemoryMetadata {
            id: row.get("id"),
            memory_type: row.get("memory_type"),
            access_count: row.get("access_count"),
            last_accessed_at: row.get("last_accessed_at"),
            created_at: row.get("created_at"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn temp_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let path_str = path.to_string_lossy().to_string();
        std::mem::forget(dir);
        Database::open(&path_str).await.unwrap()
    }

    #[tokio::test]
    async fn edge_crud() {
        let db = temp_db().await;
        db.init_memory_metadata("m1", "fact").await.unwrap();
        db.init_memory_metadata("m2", "decision").await.unwrap();

        db.create_edge("m1", "m2", EdgeType::RelatedTo, 1.0)
            .await
            .unwrap();

        let edges = db.edges_for("m1").await.unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source_id, "m1");
        assert_eq!(edges[0].target_id, "m2");
        assert_eq!(edges[0].relation_type, EdgeType::RelatedTo);

        // Also visible from m2's perspective
        let edges2 = db.edges_for("m2").await.unwrap();
        assert_eq!(edges2.len(), 1);
    }

    #[tokio::test]
    async fn duplicate_edge_upserts() {
        let db = temp_db().await;
        db.init_memory_metadata("m1", "fact").await.unwrap();
        db.init_memory_metadata("m2", "fact").await.unwrap();

        db.create_edge("m1", "m2", EdgeType::RelatedTo, 1.0)
            .await
            .unwrap();
        db.create_edge("m1", "m2", EdgeType::RelatedTo, 2.0)
            .await
            .unwrap();

        let edges = db.edges_for("m1").await.unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].weight, 2.0);
    }

    #[tokio::test]
    async fn edge_count_works() {
        let db = temp_db().await;
        db.init_memory_metadata("m1", "fact").await.unwrap();
        db.init_memory_metadata("m2", "fact").await.unwrap();
        db.init_memory_metadata("m3", "fact").await.unwrap();

        assert_eq!(db.edge_count("m1").await.unwrap(), 0);

        db.create_edge("m1", "m2", EdgeType::RelatedTo, 1.0)
            .await
            .unwrap();
        db.create_edge("m1", "m3", EdgeType::CausedBy, 1.0)
            .await
            .unwrap();

        assert_eq!(db.edge_count("m1").await.unwrap(), 2);
    }

    #[tokio::test]
    async fn access_tracking() {
        let db = temp_db().await;
        db.init_memory_metadata("m1", "observation").await.unwrap();

        let meta = db.memory_metadata("m1").await.unwrap();
        assert_eq!(meta.access_count, 0);

        db.record_memory_access("m1").await.unwrap();
        db.record_memory_access("m1").await.unwrap();

        let meta = db.memory_metadata("m1").await.unwrap();
        assert_eq!(meta.access_count, 2);
    }
}
