use anyhow::{Context, Result};
use async_trait::async_trait;
use quinn::Connection;

use super::executor::{DirEntry, ExecResult, FileContent, NodeExecutor};
use super::protocol::ProtocolMessage;
use crate::network::transport::{recv_message, send_message};

/// Remote node executor — proxies commands over QUIC.
pub struct RemoteNode {
    node_name: String,
    connection: Connection,
}

impl RemoteNode {
    pub fn new(name: String, connection: Connection) -> Self {
        Self {
            node_name: name,
            connection,
        }
    }

    async fn request(&self, msg: ProtocolMessage) -> Result<ProtocolMessage> {
        let (mut send, mut recv) = self
            .connection
            .open_bi()
            .await
            .context("failed to open QUIC stream")?;

        send_message(&mut send, &msg).await?;
        send.finish().context("failed to finish send")?;

        recv_message(&mut recv).await
    }
}

#[async_trait]
impl NodeExecutor for RemoteNode {
    async fn run_command(&self, command: &str, cwd: Option<&str>) -> Result<ExecResult> {
        let resp = self
            .request(ProtocolMessage::RunCommand {
                command: command.to_string(),
                cwd: cwd.map(String::from),
            })
            .await?;

        match resp {
            ProtocolMessage::RunCommandResult(result) => Ok(result),
            ProtocolMessage::Error { message } => anyhow::bail!("remote error: {message}"),
            _ => anyhow::bail!("unexpected response"),
        }
    }

    async fn read_file(&self, path: &str) -> Result<FileContent> {
        let resp = self
            .request(ProtocolMessage::ReadFile {
                path: path.to_string(),
            })
            .await?;

        match resp {
            ProtocolMessage::ReadFileResult(content) => Ok(content),
            ProtocolMessage::Error { message } => anyhow::bail!("remote error: {message}"),
            _ => anyhow::bail!("unexpected response"),
        }
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        let resp = self
            .request(ProtocolMessage::WriteFile {
                path: path.to_string(),
                content: content.to_string(),
            })
            .await?;

        match resp {
            ProtocolMessage::WriteFileResult { success, error } => {
                if success {
                    Ok(())
                } else {
                    anyhow::bail!("remote write error: {}", error.unwrap_or_default())
                }
            }
            ProtocolMessage::Error { message } => anyhow::bail!("remote error: {message}"),
            _ => anyhow::bail!("unexpected response"),
        }
    }

    async fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>> {
        let resp = self
            .request(ProtocolMessage::ListDir {
                path: path.to_string(),
            })
            .await?;

        match resp {
            ProtocolMessage::ListDirResult(entries) => Ok(entries),
            ProtocolMessage::Error { message } => anyhow::bail!("remote error: {message}"),
            _ => anyhow::bail!("unexpected response"),
        }
    }

    async fn search_files(&self, pattern: &str, root: Option<&str>) -> Result<Vec<String>> {
        let resp = self
            .request(ProtocolMessage::SearchFiles {
                pattern: pattern.to_string(),
                root: root.map(String::from),
            })
            .await?;

        match resp {
            ProtocolMessage::SearchFilesResult(paths) => Ok(paths),
            ProtocolMessage::Error { message } => anyhow::bail!("remote error: {message}"),
            _ => anyhow::bail!("unexpected response"),
        }
    }

    fn name(&self) -> &str {
        &self.node_name
    }
}
