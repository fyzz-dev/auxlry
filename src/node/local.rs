use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;

use super::executor::{DirEntry, ExecResult, FileContent, NodeExecutor};
use crate::config::types::NodeMode;

/// Local node executor with optional sandbox enforcement.
pub struct LocalNode {
    node_name: String,
    mode: NodeMode,
    workspace_root: Option<PathBuf>,
}

impl LocalNode {
    pub fn new(name: String, mode: NodeMode, workspace_root: Option<PathBuf>) -> Self {
        Self {
            node_name: name,
            mode,
            workspace_root,
        }
    }

    /// Check if a path is within the sandbox (workspace mode only).
    fn check_sandbox(&self, path: &str) -> Result<PathBuf> {
        let resolved = Path::new(path)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(path));

        if let NodeMode::Workspace = self.mode {
            if let Some(ref root) = self.workspace_root {
                if !resolved.starts_with(root) {
                    bail!(
                        "path '{}' is outside workspace sandbox '{}'",
                        path,
                        root.display()
                    );
                }
            }
        }
        Ok(resolved)
    }
}

#[async_trait]
impl NodeExecutor for LocalNode {
    async fn run_command(&self, command: &str, cwd: Option<&str>) -> Result<ExecResult> {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(command);

        if let Some(dir) = cwd {
            self.check_sandbox(dir)?;
            cmd.current_dir(dir);
        } else if let Some(ref root) = self.workspace_root {
            if matches!(self.mode, NodeMode::Workspace) {
                cmd.current_dir(root);
            }
        }

        let output = cmd.output().await.context("failed to execute command")?;

        Ok(ExecResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn read_file(&self, path: &str) -> Result<FileContent> {
        let resolved = self.check_sandbox(path)?;
        let content =
            tokio::fs::read_to_string(&resolved)
                .await
                .with_context(|| format!("failed to read file: {path}"))?;
        Ok(FileContent {
            path: resolved.to_string_lossy().to_string(),
            content,
        })
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        let resolved = self.check_sandbox(path)?;
        if let Some(parent) = resolved.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&resolved, content)
            .await
            .with_context(|| format!("failed to write file: {path}"))
    }

    async fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>> {
        let resolved = self.check_sandbox(path)?;
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(&resolved)
            .await
            .with_context(|| format!("failed to read directory: {path}"))?;

        while let Some(entry) = dir.next_entry().await? {
            let metadata = entry.metadata().await?;
            entries.push(DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                is_dir: metadata.is_dir(),
                size: metadata.len(),
            });
        }
        Ok(entries)
    }

    async fn search_files(&self, pattern: &str, root: Option<&str>) -> Result<Vec<String>> {
        let search_root = match root {
            Some(r) => self.check_sandbox(r)?,
            None => self
                .workspace_root
                .clone()
                .unwrap_or_else(|| PathBuf::from(".")),
        };

        let pattern = pattern.to_string();
        let results = tokio::task::spawn_blocking(move || {
            let mut matches = Vec::new();
            for entry in glob::glob(
                &search_root
                    .join(&pattern)
                    .to_string_lossy(),
            )
            .unwrap_or_else(|_| glob::glob("").unwrap())
            {
                if let Ok(path) = entry {
                    matches.push(path.to_string_lossy().to_string());
                }
            }
            matches
        })
        .await?;

        Ok(results)
    }

    fn name(&self) -> &str {
        &self.node_name
    }
}
