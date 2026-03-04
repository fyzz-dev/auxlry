use anyhow::{Context, Result};
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};

use crate::events::types::Event;

/// A message retrieved from the database.
#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub author: String,
    pub content: String,
    pub direction: String,
    pub created_at: String,
}

/// SQLite database wrapper.
#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Open (or create) the SQLite database and run migrations.
    pub async fn open(path: &str) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let url = format!("sqlite:{path}?mode=rwc");
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .context("failed to connect to SQLite")?;

        let db = Self { pool };
        db.run_migrations().await?;
        Ok(db)
    }

    async fn run_migrations(&self) -> Result<()> {
        let m001 = include_str!("../../migrations/001_initial.sql");
        sqlx::raw_sql(m001)
            .execute(&self.pool)
            .await
            .context("failed to run migration 001")?;

        let m002 = include_str!("../../migrations/002_memory_graph.sql");
        sqlx::raw_sql(m002)
            .execute(&self.pool)
            .await
            .context("failed to run migration 002")?;

        let m003 = include_str!("../../migrations/003_pending_links.sql");
        sqlx::raw_sql(m003)
            .execute(&self.pool)
            .await
            .context("failed to run migration 003")?;

        Ok(())
    }

    /// Insert an event record.
    pub async fn insert_event(&self, event: &Event) -> Result<()> {
        let payload_json =
            serde_json::to_string(&event.payload).context("failed to serialize event payload")?;

        sqlx::query("INSERT INTO events (id, kind, payload, created_at) VALUES (?, ?, ?, ?)")
            .bind(&event.id)
            .bind(event.kind())
            .bind(&payload_json)
            .bind(event.timestamp.to_rfc3339())
            .execute(&self.pool)
            .await
            .context("failed to insert event")?;

        Ok(())
    }

    /// Insert a message record.
    pub async fn insert_message(
        &self,
        interface: &str,
        channel: &str,
        author: &str,
        content: &str,
        direction: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO messages (interface, channel, author, content, direction) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(interface)
        .bind(channel)
        .bind(author)
        .bind(content)
        .bind(direction)
        .execute(&self.pool)
        .await
        .context("failed to insert message")?;

        Ok(())
    }

    /// Query recent events, newest first.
    pub async fn recent_events(&self, limit: i64) -> Result<Vec<Event>> {
        let rows =
            sqlx::query("SELECT id, kind, payload, created_at FROM events ORDER BY created_at DESC LIMIT ?")
                .bind(limit)
                .fetch_all(&self.pool)
                .await?;

        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.get("id");
            let payload_str: String = row.get("payload");
            let created_at: String = row.get("created_at");

            let payload = serde_json::from_str(&payload_str)?;
            let timestamp = chrono::DateTime::parse_from_rfc3339(&created_at)?.to_utc();

            events.push(Event {
                id,
                timestamp,
                payload,
            });
        }
        Ok(events)
    }

    /// Store a node authentication token.
    pub async fn store_node_token(&self, node_name: &str, token: &str) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO node_tokens (node_name, token) VALUES (?, ?)",
        )
        .bind(node_name)
        .bind(token)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get a node's authentication token.
    pub async fn get_node_token(&self, node_name: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT token FROM node_tokens WHERE node_name = ?")
            .bind(node_name)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get("token")))
    }

    /// Find the node name that owns a given token.
    pub async fn find_node_by_token(&self, token: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT node_name FROM node_tokens WHERE token = ?")
            .bind(token)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get("node_name")))
    }

    /// Query recent messages for a given interface+channel, oldest first.
    pub async fn get_recent_messages(
        &self,
        interface: &str,
        channel: &str,
        limit: i64,
    ) -> Result<Vec<StoredMessage>> {
        let rows = sqlx::query(
            "SELECT author, content, direction, created_at FROM messages \
             WHERE interface = ? AND channel = ? \
             ORDER BY created_at DESC LIMIT ?",
        )
        .bind(interface)
        .bind(channel)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        // Reverse so oldest is first (chronological order)
        let mut messages: Vec<StoredMessage> = rows
            .iter()
            .map(|row| StoredMessage {
                author: row.get("author"),
                content: row.get("content"),
                direction: row.get("direction"),
                created_at: row.get("created_at"),
            })
            .collect();
        messages.reverse();
        Ok(messages)
    }

    /// Count events grouped by hour and kind for the last N hours.
    pub async fn events_by_hour(&self, kinds: &[&str], hours: i64) -> Result<Vec<(String, String, i64)>> {
        if kinds.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders: Vec<&str> = kinds.iter().map(|_| "?").collect();
        let sql = format!(
            "SELECT strftime('%Y-%m-%d %H:00', created_at) as hour, kind, COUNT(*) as cnt \
             FROM events \
             WHERE kind IN ({}) AND created_at >= datetime('now', '-{} hours') \
             GROUP BY hour, kind ORDER BY hour",
            placeholders.join(","),
            hours
        );
        let mut query = sqlx::query(&sql);
        for kind in kinds {
            query = query.bind(*kind);
        }
        let rows = query.fetch_all(&self.pool).await?;
        Ok(rows
            .iter()
            .map(|r| (r.get("hour"), r.get("kind"), r.get("cnt")))
            .collect())
    }

    /// Count events grouped by day and kind for the last N days.
    pub async fn events_by_day(&self, kinds: &[&str], days: i64) -> Result<Vec<(String, String, i64)>> {
        if kinds.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders: Vec<&str> = kinds.iter().map(|_| "?").collect();
        let sql = format!(
            "SELECT date(created_at) as day, kind, COUNT(*) as cnt \
             FROM events \
             WHERE kind IN ({}) AND created_at >= datetime('now', '-{} days') \
             GROUP BY day, kind ORDER BY day",
            placeholders.join(","),
            days
        );
        let mut query = sqlx::query(&sql);
        for kind in kinds {
            query = query.bind(*kind);
        }
        let rows = query.fetch_all(&self.pool).await?;
        Ok(rows
            .iter()
            .map(|r| (r.get("day"), r.get("kind"), r.get("cnt")))
            .collect())
    }

    /// Count messages per day for the current month.
    pub async fn messages_per_day_this_month(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            "SELECT date(created_at) as day, COUNT(*) as cnt \
             FROM messages \
             WHERE created_at >= date('now', 'start of month') \
             GROUP BY day ORDER BY day",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| (r.get("day"), r.get("cnt"))).collect())
    }

    /// Count memories grouped by type.
    pub async fn memory_counts_by_type(&self) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            "SELECT memory_type, COUNT(*) as cnt FROM memory_metadata GROUP BY memory_type",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| (r.get("memory_type"), r.get("cnt")))
            .collect())
    }

    /// Store a pending one-time link code.
    pub async fn store_pending_code(&self, code: &str) -> Result<()> {
        sqlx::query("INSERT INTO pending_link_codes (code) VALUES (?)")
            .bind(code)
            .execute(&self.pool)
            .await
            .context("failed to store pending link code")?;
        Ok(())
    }

    /// Consume a pending link code. Returns true if the code existed (and was deleted).
    pub async fn consume_pending_code(&self, code: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM pending_link_codes WHERE code = ?")
            .bind(code)
            .execute(&self.pool)
            .await
            .context("failed to consume pending link code")?;
        Ok(result.rows_affected() > 0)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::types::EventPayload;

    async fn temp_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        // Keep dir alive by leaking — test only
        let path_str = path.to_string_lossy().to_string();
        std::mem::forget(dir);
        Database::open(&path_str).await.unwrap()
    }

    #[tokio::test]
    async fn insert_and_query_events() {
        let db = temp_db().await;
        let event = Event::new(EventPayload::CoreStarted);
        db.insert_event(&event).await.unwrap();

        let events = db.recent_events(10).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind(), "core_started");
    }

    #[tokio::test]
    async fn insert_message() {
        let db = temp_db().await;
        db.insert_message("discord", "general", "user1", "hello", "inbound")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn pending_link_codes() {
        let db = temp_db().await;
        db.store_pending_code("123456").await.unwrap();

        // Consuming a valid code returns true
        assert!(db.consume_pending_code("123456").await.unwrap());
        // Consuming again returns false (already deleted)
        assert!(!db.consume_pending_code("123456").await.unwrap());
        // Unknown code returns false
        assert!(!db.consume_pending_code("999999").await.unwrap());
    }

    #[tokio::test]
    async fn node_tokens() {
        let db = temp_db().await;
        db.store_node_token("mynode", "secret123").await.unwrap();
        let token = db.get_node_token("mynode").await.unwrap();
        assert_eq!(token, Some("secret123".to_string()));

        let missing = db.get_node_token("other").await.unwrap();
        assert!(missing.is_none());
    }
}
