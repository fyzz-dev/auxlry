use std::path::PathBuf;
use std::sync::Arc;
use anyhow::{Context, Result};
use quinn::Connection;
use tracing::{error, info, warn};

use crate::config::types::NodeMode;
use crate::events::bus::EventBus;
use crate::events::types::{Event, EventPayload};
use crate::network::transport::{recv_message, send_message};
use crate::node::executor::NodeExecutor;
use crate::node::linking;
use crate::node::local::LocalNode;
use crate::node::protocol::ProtocolMessage;
use crate::node::registry::NodeRegistry;
use crate::node::remote::RemoteNode;
use crate::storage::database::Database;

/// Handle a single inbound QUIC connection from a remote node.
pub async fn handle_connection(
    conn: Connection,
    db: Database,
    bus: EventBus,
    workspace: PathBuf,
    registry: NodeRegistry,
) {
    let remote = conn.remote_address();
    info!(%remote, "new QUIC connection");

    // First stream must be authentication
    let node_name = match authenticate(&conn, &db).await {
        Ok(name) => name,
        Err(e) => {
            warn!(%remote, error = %e, "authentication failed");
            conn.close(1u32.into(), b"auth failed");
            return;
        }
    };

    info!(node = %node_name, %remote, "node authenticated");

    // Register a RemoteNode so the operator can dispatch work to it
    let remote_node = Arc::new(RemoteNode::new(node_name.clone(), conn.clone()));
    registry
        .register(node_name.clone(), remote_node as Arc<dyn NodeExecutor>)
        .await;
    info!(node = %node_name, "registered remote node in registry");

    bus.publish(Event::new(EventPayload::NodeConnected {
        node: node_name.clone(),
    }));

    // Create a sandboxed LocalNode for this remote connection
    let node_workspace = workspace.join(&node_name);
    if let Err(e) = std::fs::create_dir_all(&node_workspace) {
        error!(error = %e, "failed to create node workspace");
        conn.close(2u32.into(), b"internal error");
        return;
    }
    let local_node = LocalNode::new(node_name.clone(), NodeMode::Workspace, Some(node_workspace));

    // Enter protocol loop
    loop {
        let stream = match conn.accept_bi().await {
            Ok(s) => s,
            Err(quinn::ConnectionError::ApplicationClosed(_)) => {
                info!(node = %node_name, "node disconnected gracefully");
                break;
            }
            Err(e) => {
                warn!(node = %node_name, error = %e, "connection error");
                break;
            }
        };

        let (mut send, mut recv) = stream;

        let msg = match recv_message(&mut recv).await {
            Ok(m) => m,
            Err(e) => {
                warn!(node = %node_name, error = %e, "failed to read message");
                continue;
            }
        };

        let response = handle_message(msg, &local_node).await;

        if let Err(e) = send_message(&mut send, &response).await {
            warn!(node = %node_name, error = %e, "failed to send response");
        }
        let _ = send.finish();
    }

    // Unregister the node from the registry
    registry.unregister(&node_name).await;
    info!(node = %node_name, "unregistered remote node from registry");

    bus.publish(Event::new(EventPayload::NodeDisconnected {
        node: node_name,
    }));
}

/// Authenticate the first stream: expects AuthRequest or TokenAuth.
async fn authenticate(conn: &Connection, db: &Database) -> Result<String> {
    let (mut send, mut recv) = conn
        .accept_bi()
        .await
        .context("failed to accept auth stream")?;

    let msg = recv_message(&mut recv).await?;

    match msg {
        ProtocolMessage::AuthRequest { code, name } => {
            // Validate the one-time code against pending_link_codes table
            let valid = db.consume_pending_code(&code).await.unwrap_or(false);

            if !valid {
                send_message(
                    &mut send,
                    &ProtocolMessage::AuthResponse {
                        success: false,
                        token: None,
                    },
                )
                .await?;
                let _ = send.finish();
                anyhow::bail!("invalid or expired link code");
            }

            let node_name = name;
            let token = uuid::Uuid::new_v4().to_string();
            linking::store_link_token(db, &node_name, &token).await?;

            send_message(
                &mut send,
                &ProtocolMessage::AuthResponse {
                    success: true,
                    token: Some(token),
                },
            )
            .await?;
            let _ = send.finish();
            Ok(node_name)
        }
        ProtocolMessage::TokenAuth { token } => {
            // Look up which node owns this token
            let node_name = db.find_node_by_token(&token).await.unwrap_or(None);

            if let Some(node_name) = node_name {
                send_message(
                    &mut send,
                    &ProtocolMessage::TokenAuthResponse { success: true },
                )
                .await?;
                let _ = send.finish();
                Ok(node_name)
            } else {
                send_message(
                    &mut send,
                    &ProtocolMessage::TokenAuthResponse { success: false },
                )
                .await?;
                let _ = send.finish();
                anyhow::bail!("invalid token")
            }
        }
        _ => {
            send_message(
                &mut send,
                &ProtocolMessage::Error {
                    message: "expected AuthRequest or TokenAuth".into(),
                },
            )
            .await?;
            let _ = send.finish();
            anyhow::bail!("unexpected first message")
        }
    }
}

/// Process a single protocol message and return the response.
async fn handle_message(msg: ProtocolMessage, node: &LocalNode) -> ProtocolMessage {
    match msg {
        ProtocolMessage::Ping => ProtocolMessage::Pong,

        ProtocolMessage::RunCommand { command, cwd } => {
            match node.run_command(&command, cwd.as_deref()).await {
                Ok(result) => ProtocolMessage::RunCommandResult(result),
                Err(e) => ProtocolMessage::Error {
                    message: e.to_string(),
                },
            }
        }

        ProtocolMessage::ReadFile { path } => match node.read_file(&path).await {
            Ok(content) => ProtocolMessage::ReadFileResult(content),
            Err(e) => ProtocolMessage::Error {
                message: e.to_string(),
            },
        },

        ProtocolMessage::WriteFile { path, content } => {
            match node.write_file(&path, &content).await {
                Ok(()) => ProtocolMessage::WriteFileResult {
                    success: true,
                    error: None,
                },
                Err(e) => ProtocolMessage::WriteFileResult {
                    success: false,
                    error: Some(e.to_string()),
                },
            }
        }

        ProtocolMessage::ListDir { path } => match node.list_dir(&path).await {
            Ok(entries) => ProtocolMessage::ListDirResult(entries),
            Err(e) => ProtocolMessage::Error {
                message: e.to_string(),
            },
        },

        ProtocolMessage::SearchFiles { pattern, root } => {
            match node.search_files(&pattern, root.as_deref()).await {
                Ok(paths) => ProtocolMessage::SearchFilesResult(paths),
                Err(e) => ProtocolMessage::Error {
                    message: e.to_string(),
                },
            }
        }

        other => ProtocolMessage::Error {
            message: format!("unexpected message: {other:?}"),
        },
    }
}
