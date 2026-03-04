use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use serde_json::json;

use crate::node::executor::NodeExecutor;

/// Shared error type for all node tools.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct NodeToolError(pub String);

// ── ReadFile ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ReadFileArgs {
    pub path: String,
}

pub struct ReadFileTool {
    pub node: Arc<dyn NodeExecutor>,
}

impl Tool for ReadFileTool {
    const NAME: &'static str = "read_file";
    type Error = NodeToolError;
    type Args = ReadFileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".to_string(),
            description: "Read the contents of a file at the given path".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to read" }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.node
            .read_file(&args.path)
            .await
            .map(|fc| fc.content)
            .map_err(|e| NodeToolError(e.to_string()))
    }
}

// ── WriteFile ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct WriteFileArgs {
    pub path: String,
    pub content: String,
}

pub struct WriteFileTool {
    pub node: Arc<dyn NodeExecutor>,
}

impl Tool for WriteFileTool {
    const NAME: &'static str = "write_file";
    type Error = NodeToolError;
    type Args = WriteFileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "write_file".to_string(),
            description: "Write content to a file at the given path".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to write to" },
                    "content": { "type": "string", "description": "Content to write" }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.node
            .write_file(&args.path, &args.content)
            .await
            .map(|()| format!("wrote {}", args.path))
            .map_err(|e| NodeToolError(e.to_string()))
    }
}

// ── RunCommand ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RunCommandArgs {
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
}

pub struct RunCommandTool {
    pub node: Arc<dyn NodeExecutor>,
}

impl Tool for RunCommandTool {
    const NAME: &'static str = "run_command";
    type Error = NodeToolError;
    type Args = RunCommandArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "run_command".to_string(),
            description: "Execute a shell command and return its output".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute" },
                    "cwd": { "type": "string", "description": "Working directory (optional)", "nullable": true }
                },
                "required": ["command"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        match self.node.run_command(&args.command, args.cwd.as_deref()).await {
            Ok(result) => {
                let output = if result.stderr.is_empty() {
                    result.stdout
                } else {
                    format!("{}\n--- stderr ---\n{}", result.stdout, result.stderr)
                };
                Ok(output)
            }
            Err(e) => Err(NodeToolError(e.to_string())),
        }
    }
}

// ── ListDir ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ListDirArgs {
    pub path: String,
}

pub struct ListDirTool {
    pub node: Arc<dyn NodeExecutor>,
}

impl Tool for ListDirTool {
    const NAME: &'static str = "list_dir";
    type Error = NodeToolError;
    type Args = ListDirArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "list_dir".to_string(),
            description: "List the contents of a directory".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to list" }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.node
            .list_dir(&args.path)
            .await
            .map(|entries| {
                entries
                    .iter()
                    .map(|e| {
                        let kind = if e.is_dir { "dir" } else { "file" };
                        format!("{} ({}, {} bytes)", e.name, kind, e.size)
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .map_err(|e| NodeToolError(e.to_string()))
    }
}

// ── SearchFiles ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SearchFilesArgs {
    pub pattern: String,
    #[serde(default)]
    pub root: Option<String>,
}

pub struct SearchFilesTool {
    pub node: Arc<dyn NodeExecutor>,
}

impl Tool for SearchFilesTool {
    const NAME: &'static str = "search_files";
    type Error = NodeToolError;
    type Args = SearchFilesArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "search_files".to_string(),
            description: "Search for files matching a glob pattern".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Glob pattern to search for" },
                    "root": { "type": "string", "description": "Root directory to search from (optional)", "nullable": true }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.node
            .search_files(&args.pattern, args.root.as_deref())
            .await
            .map(|paths| paths.join("\n"))
            .map_err(|e| NodeToolError(e.to_string()))
    }
}
