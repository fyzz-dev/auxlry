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

/// Start a node — connects to core via QUIC and enters the protocol loop.
pub async fn start(name: &str) -> Result<()> {
    info!(node = name, "starting node");

    let paths = AuxlryPaths::new()?;

    // Load saved token from file
    let token = linking::read_token_file(&paths.token_file).await?;

    // Read core address from config (default localhost:8401)
    let core_addr = format!("127.0.0.1:8401");
    let endpoint = quic::client_endpoint()?;

    let conn = endpoint
        .connect(core_addr.parse()?, "auxlry")
        .context("failed to connect to core")?
        .await
        .context("QUIC handshake failed")?;

    // Authenticate with token
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .context("failed to open auth stream")?;

    send_message(&mut send, &ProtocolMessage::TokenAuth { token }).await?;
    send.finish().context("failed to finish auth send")?;

    let response = recv_message(&mut recv).await?;
    match response {
        ProtocolMessage::TokenAuthResponse { success: true } => {
            info!("authenticated with core");
        }
        ProtocolMessage::TokenAuthResponse { success: false } => {
            bail!("authentication failed — token may be invalid, try re-linking");
        }
        _ => bail!("unexpected auth response"),
    }

    println!("node '{name}' connected and ready");

    // Create local node for executing requests
    let workspace = paths.workspace_dir.join(name);
    std::fs::create_dir_all(&workspace)?;
    let local_node = LocalNode::new(name.to_string(), NodeMode::Workspace, Some(workspace));

    // Protocol loop: accept requests from core and execute locally
    loop {
        let (mut send, mut recv) = match conn.accept_bi().await {
            Ok(s) => s,
            Err(quinn::ConnectionError::ApplicationClosed(_)) => {
                println!("core disconnected gracefully");
                break;
            }
            Err(e) => {
                eprintln!("connection error: {e}");
                break;
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

    Ok(())
}

/// Stop a node.
pub async fn stop(name: &str) -> Result<()> {
    tracing::info!(node = name, "stopping node");
    println!("node '{name}' stopped");
    Ok(())
}

/// Link a remote node to this core using a one-time code.
pub async fn link(core_addr: &str, code: &str) -> Result<()> {
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
            // Save token to disk
            linking::store_token_file(&paths.token_file, &token).await?;
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
