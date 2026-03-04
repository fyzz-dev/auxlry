use std::time::Duration;

use anyhow::{Context, Result, bail};
use tracing::info;

use crate::config::types::NodeMode;
use crate::network::quic;
use crate::network::transport::{recv_message, send_message};
use crate::node::executor::NodeExecutor;
use crate::node::linking;
use crate::node::local::LocalNode;
use crate::node::protocol::ProtocolMessage;
use crate::storage::paths::AuxlryPaths;

const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Start a node — connects to core via QUIC and enters the protocol loop.
/// Automatically reconnects on network errors with exponential backoff.
pub async fn start(name: &str) -> Result<()> {
    info!(node = name, "starting node");

    let paths = AuxlryPaths::new()?;
    let token = linking::read_token_file(&paths.token_file).await?;
    let core_addr = linking::read_core_addr(&paths.core_addr_file).await?;

    let workspace = paths.workspace_dir.join(name);
    std::fs::create_dir_all(&workspace)?;
    let local_node = LocalNode::new(name.to_string(), NodeMode::Workspace, Some(workspace));

    let mut backoff = BACKOFF_INITIAL;

    loop {
        // Connect to core
        let endpoint = quic::client_endpoint()?;
        let conn = match endpoint
            .connect(core_addr.parse()?, "auxlry")
            .context("failed to connect to core")
        {
            Ok(connecting) => match connecting.await {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("QUIC handshake failed: {e}");
                    println!("reconnecting in {}s...", backoff.as_secs());
                    info!(delay = backoff.as_secs(), "reconnecting after handshake failure");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(BACKOFF_MAX);
                    continue;
                }
            },
            Err(e) => {
                eprintln!("failed to connect to core: {e}");
                println!("reconnecting in {}s...", backoff.as_secs());
                info!(delay = backoff.as_secs(), "reconnecting after connect failure");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
                continue;
            }
        };

        // Authenticate with token
        let auth_result = authenticate(&conn, &token).await;
        match auth_result {
            Ok(()) => {
                info!("authenticated with core");
            }
            Err(AuthError::InvalidToken(msg)) => {
                bail!("{msg}");
            }
            Err(AuthError::Network(e)) => {
                eprintln!("auth failed (network): {e}");
                println!("reconnecting in {}s...", backoff.as_secs());
                info!(delay = backoff.as_secs(), "reconnecting after auth network error");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
                continue;
            }
        }

        // Connected and authenticated — reset backoff
        backoff = BACKOFF_INITIAL;
        println!("node '{name}' connected and ready");

        // Protocol loop
        let exit = run_protocol_loop(&conn, &local_node).await;
        match exit {
            LoopExit::GracefulClose => {
                println!("core disconnected gracefully");
                return Ok(());
            }
            LoopExit::ConnectionError(e) => {
                eprintln!("connection lost: {e}");
                println!("reconnecting in {}s...", backoff.as_secs());
                info!(delay = backoff.as_secs(), "reconnecting after connection loss");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        }
    }
}

enum AuthError {
    InvalidToken(String),
    Network(anyhow::Error),
}

async fn authenticate(conn: &quinn::Connection, token: &str) -> std::result::Result<(), AuthError> {
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| AuthError::Network(e.into()))?;

    send_message(
        &mut send,
        &ProtocolMessage::TokenAuth {
            token: token.to_string(),
        },
    )
    .await
    .map_err(|e| AuthError::Network(e))?;

    send.finish()
        .map_err(|e| AuthError::Network(e.into()))?;

    let response = recv_message(&mut recv)
        .await
        .map_err(|e| AuthError::Network(e))?;

    match response {
        ProtocolMessage::TokenAuthResponse { success: true } => Ok(()),
        ProtocolMessage::TokenAuthResponse { success: false } => Err(AuthError::InvalidToken(
            "authentication failed — token may be invalid, try re-linking".to_string(),
        )),
        _ => Err(AuthError::Network(anyhow::anyhow!("unexpected auth response"))),
    }
}

enum LoopExit {
    GracefulClose,
    ConnectionError(String),
}

async fn run_protocol_loop(conn: &quinn::Connection, local_node: &LocalNode) -> LoopExit {
    loop {
        let (mut send, mut recv) = match conn.accept_bi().await {
            Ok(s) => s,
            Err(quinn::ConnectionError::ApplicationClosed(_)) => {
                return LoopExit::GracefulClose;
            }
            Err(e) => {
                return LoopExit::ConnectionError(e.to_string());
            }
        };

        let msg = match recv_message(&mut recv).await {
            Ok(m) => m,
            Err(e) => {
                eprintln!("failed to read message: {e}");
                continue;
            }
        };

        let response = match msg {
            ProtocolMessage::Ping => ProtocolMessage::Pong,
            ProtocolMessage::RunCommand { command, cwd } => {
                match local_node.run_command(&command, cwd.as_deref()).await {
                    Ok(r) => ProtocolMessage::RunCommandResult(r),
                    Err(e) => ProtocolMessage::Error {
                        message: e.to_string(),
                    },
                }
            }
            ProtocolMessage::ReadFile { path } => match local_node.read_file(&path).await {
                Ok(c) => ProtocolMessage::ReadFileResult(c),
                Err(e) => ProtocolMessage::Error {
                    message: e.to_string(),
                },
            },
            ProtocolMessage::WriteFile { path, content } => {
                match local_node.write_file(&path, &content).await {
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
            ProtocolMessage::ListDir { path } => match local_node.list_dir(&path).await {
                Ok(entries) => ProtocolMessage::ListDirResult(entries),
                Err(e) => ProtocolMessage::Error {
                    message: e.to_string(),
                },
            },
            ProtocolMessage::SearchFiles { pattern, root } => {
                match local_node.search_files(&pattern, root.as_deref()).await {
                    Ok(paths) => ProtocolMessage::SearchFilesResult(paths),
                    Err(e) => ProtocolMessage::Error {
                        message: e.to_string(),
                    },
                }
            }
            other => ProtocolMessage::Error {
                message: format!("unexpected message: {other:?}"),
            },
        };

        if let Err(e) = send_message(&mut send, &response).await {
            eprintln!("failed to send response: {e}");
        }
        let _ = send.finish();
    }
}

/// Stop a node.
pub async fn stop(name: &str) -> Result<()> {
    tracing::info!(node = name, "stopping node");
    println!("node '{name}' stopped");
    Ok(())
}

/// Link a remote node to this core using a one-time code.
pub async fn link(name: &str, core_addr: &str, code: &str) -> Result<()> {
    info!(core = core_addr, "linking node");

    let paths = AuxlryPaths::new()?;
    let endpoint = quic::client_endpoint()?;

    let addr: std::net::SocketAddr = core_addr
        .parse()
        .context("invalid core address — expected host:port")?;

    let conn = endpoint
        .connect(addr, "auxlry")
        .context("failed to connect to core")?
        .await
        .context("QUIC handshake failed")?;

    // Send auth request with the one-time code
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .context("failed to open auth stream")?;

    send_message(
        &mut send,
        &ProtocolMessage::AuthRequest {
            code: code.to_string(),
            name: name.to_string(),
        },
    )
    .await?;
    send.finish().context("failed to finish auth send")?;

    let response = recv_message(&mut recv).await?;
    match response {
        ProtocolMessage::AuthResponse {
            success: true,
            token: Some(token),
        } => {
            // Save token and core address to disk
            linking::store_token_file(&paths.token_file, &token).await?;
            linking::store_core_addr(&paths.core_addr_file, core_addr).await?;
            println!("linked successfully — token saved to {}", paths.token_file.display());
            Ok(())
        }
        ProtocolMessage::AuthResponse {
            success: false, ..
        } => {
            bail!("linking failed — invalid code")
        }
        _ => bail!("unexpected response from core"),
    }
}
