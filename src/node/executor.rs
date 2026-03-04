use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Result of a command execution on a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Result of a file read operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
}

/// Directory listing entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Trait for executing operations on a node (local or remote).
#[async_trait]
pub trait NodeExecutor: Send + Sync {
    /// Execute a shell command.
    async fn run_command(&self, command: &str, cwd: Option<&str>) -> Result<ExecResult>;

    /// Read a file's contents.
    async fn read_file(&self, path: &str) -> Result<FileContent>;

    /// Write content to a file.
    async fn write_file(&self, path: &str, content: &str) -> Result<()>;

    /// List directory contents.
    async fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>>;

    /// Search for files matching a glob pattern.
    async fn search_files(&self, pattern: &str, root: Option<&str>) -> Result<Vec<String>>;

    /// The node's name.
    fn name(&self) -> &str;
}
