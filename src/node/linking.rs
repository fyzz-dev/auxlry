use std::path::Path;

use anyhow::{Context, Result};
use rand::Rng;

use crate::storage::database::Database;

/// Generate a one-time link code.
pub fn generate_link_code() -> String {
    let mut rng = rand::thread_rng();
    let code: u32 = rng.r#gen::<u32>() % 1_000_000;
    format!("{code:06}")
}

/// Store a linking token after successful authentication (core-side, SQLite).
pub async fn store_link_token(db: &Database, node_name: &str, token: &str) -> Result<()> {
    db.store_node_token(node_name, token)
        .await
        .context("failed to store link token")
}

/// Verify a node's authentication token (core-side, SQLite).
pub async fn verify_token(db: &Database, node_name: &str, token: &str) -> Result<bool> {
    match db.get_node_token(node_name).await? {
        Some(stored) => Ok(stored == token),
        None => Ok(false),
    }
}

/// Store a token to a file (standalone node-side, no DB).
pub async fn store_token_file(path: &Path, token: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("failed to create token directory")?;
    }
    tokio::fs::write(path, token)
        .await
        .context("failed to write token file")
}

/// Read a token from a file (standalone node-side, no DB).
pub async fn read_token_file(path: &Path) -> Result<String> {
    let contents = tokio::fs::read_to_string(path)
        .await
        .context("no token found — run `auxlry node link` first")?;
    let token = contents.trim().to_string();
    if token.is_empty() {
        anyhow::bail!("token file is empty — run `auxlry node link` first");
    }
    Ok(token)
}

/// Store the core address to a file (node-side).
pub async fn store_core_addr(path: &Path, addr: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("failed to create core addr directory")?;
    }
    tokio::fs::write(path, addr)
        .await
        .context("failed to write core addr file")
}

/// Read the core address from a file (node-side).
pub async fn read_core_addr(path: &Path) -> Result<String> {
    let contents = tokio::fs::read_to_string(path)
        .await
        .context("no core address found — run `auxlry node link` first")?;
    let addr = contents.trim().to_string();
    if addr.is_empty() {
        anyhow::bail!("core addr file is empty — run `auxlry node link` first");
    }
    Ok(addr)
}
