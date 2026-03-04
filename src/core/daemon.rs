use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::net::TcpListener;
use tokio::signal;
use tracing::{error, info, warn};

use crate::adapters::discord::DiscordAdapter;
use crate::adapters::Adapter;
use crate::api::routes;
use crate::config::loader::load_config;
use crate::config::types::{AdapterConfig, NodeMode};
use crate::core::state::AppState;
use crate::events::bus::EventBus;
use crate::events::persist::spawn_persister;
use crate::events::types::{Event, EventPayload};
use crate::interface::agent::InterfaceAgent;
use crate::interface::session::SessionManager;
use crate::memory::store::MemoryStore;
use crate::network::hole_punch;
use crate::network::quic;
use crate::node::handler;
use crate::node::local::LocalNode;
use crate::operator::agent::OperatorAgent;
use crate::storage::database::Database;
use crate::storage::paths::AuxlryPaths;
use crate::synapse::agent::SynapseAgent;

/// Run the core daemon (foreground mode).
pub async fn run(paths: AuxlryPaths) -> Result<()> {
    // Install rustls crypto provider before anything uses TLS
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("failed to install rustls crypto provider"))?;

    // Init paths
    paths.ensure_dirs()?;

    // Load config
    let config = load_config(&paths.config_file)?;
    info!(port = config.core.api_port, "loaded config");

    // Open database
    let db_path = paths.database.to_string_lossy().to_string();
    let db = Database::open(&db_path).await?;
    info!("database ready");

    // Initialize memory store
    let memory = match MemoryStore::open(&paths.memory_dir).await {
        Ok(m) => {
            info!("memory store ready");
            Some(Arc::new(m))
        }
        Err(e) => {
            warn!(error = %e, "failed to initialize memory store, continuing without memory");
            None
        }
    };

    // Create event bus
    let bus = EventBus::new();

    // Start event persister
    let _persister = spawn_persister(bus.subscribe(), db.clone());

    // Build shared state
    let state = AppState::new(config.clone(), bus.clone(), db.clone(), paths.clone(), memory);

    // Publish CoreStarted
    bus.publish(Event::new(EventPayload::CoreStarted));

    // STUN discovery — log public address
    if let Some(stun_server) = config.core.stun_servers.first() {
        match hole_punch::stun_discover(stun_server).await {
            Ok(public_addr) => {
                info!(%public_addr, "STUN discovery: detected public address");
            }
            Err(e) => {
                warn!(error = %e, "STUN discovery failed, continuing with local address");
            }
        }
    }

    // Create session manager for message batching
    let (session_mgr, batch_rx) = SessionManager::new(1500);
    let session_mgr = Arc::new(session_mgr);

    // Instantiate adapters from config
    let mut adapters: Vec<Arc<dyn Adapter>> = Vec::new();
    for iface in &config.interfaces {
        match &iface.adapter {
            AdapterConfig::Discord { token, channels } => {
                let adapter = DiscordAdapter::new(
                    token.clone(),
                    channels.clone(),
                    bus.clone(),
                    iface.name.clone(),
                );
                adapters.push(Arc::new(adapter));
            }
            other => {
                warn!(name = %iface.name, adapter = ?other, "adapter type not yet implemented, skipping");
            }
        }
    }

    // Spawn each adapter's start() as a background task
    for adapter in &adapters {
        let adapter = Arc::clone(adapter);
        tokio::spawn(async move {
            if let Err(e) = adapter.start().await {
                error!(adapter = adapter.name(), error = %e, "adapter stopped with error");
            }
        });
    }

    // Bridge event bus → session manager (forward MessageReceived events)
    {
        let mut bus_rx = bus.subscribe();
        let session_mgr = Arc::clone(&session_mgr);
        let db = db.clone();
        tokio::spawn(async move {
            loop {
                match bus_rx.recv().await {
                    Ok(event) => {
                        if let EventPayload::MessageReceived {
                            interface,
                            channel,
                            author,
                            content,
                        } = &event.payload
                        {
                            session_mgr
                                .add_message(interface, channel, author, content)
                                .await;
                            if let Err(e) = db
                                .insert_message(interface, channel, author, content, "inbound")
                                .await
                            {
                                warn!(error = %e, "failed to persist inbound message");
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!(skipped = n, "event bus subscriber lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    // Create local node from config
    let local_node: Arc<dyn crate::node::executor::NodeExecutor> = {
        let workspace = paths.workspace_dir.clone();
        let node_cfg = config.nodes.first();
        let (name, mode) = match node_cfg {
            Some(n) => (n.name.clone(), n.mode.clone()),
            None => ("local".into(), NodeMode::Workspace),
        };
        Arc::new(LocalNode::new(name, mode, Some(workspace)))
    };

    // Create the agent hierarchy: Operator → Synapse → Interface
    // Each layer can delegate to the one below via rig tool calling.
    let operator = match OperatorAgent::new(state.clone(), local_node.clone()) {
        Ok(o) => Arc::new(o),
        Err(e) => {
            error!(error = %e, "failed to create operator agent");
            return Err(e);
        }
    };
    let synapse = match SynapseAgent::new(state.clone(), operator.clone()) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            error!(error = %e, "failed to create synapse agent");
            return Err(e);
        }
    };

    // Spawn the interface agent with delegation tools
    if !adapters.is_empty() {
        match InterfaceAgent::new(state.clone(), synapse, operator) {
            Ok(agent) => {
                let agent_adapters = adapters.clone();
                tokio::spawn(async move {
                    agent.run(batch_rx, agent_adapters).await;
                });
            }
            Err(e) => {
                error!(error = %e, "failed to create interface agent");
            }
        }
    }

    // Start QUIC server for remote nodes
    {
        let quic_addr: SocketAddr =
            format!("{}:{}", config.core.host, config.core.quic_port)
                .parse()
                .context("invalid QUIC bind address")?;

        match quic::server_endpoint(quic_addr) {
            Ok(endpoint) => {
                info!(%quic_addr, "QUIC server listening");
                let db = db.clone();
                let bus = bus.clone();
                let workspace = paths.workspace_dir.clone();
                tokio::spawn(async move {
                    loop {
                        match endpoint.accept().await {
                            Some(incoming) => {
                                let conn = match incoming.await {
                                    Ok(c) => c,
                                    Err(e) => {
                                        warn!(error = %e, "failed to accept QUIC connection");
                                        continue;
                                    }
                                };
                                let db = db.clone();
                                let bus = bus.clone();
                                let workspace = workspace.clone();
                                tokio::spawn(async move {
                                    handler::handle_connection(conn, db, bus, workspace).await;
                                });
                            }
                            None => {
                                info!("QUIC endpoint closed");
                                break;
                            }
                        }
                    }
                });
            }
            Err(e) => {
                warn!(error = %e, "failed to start QUIC server, remote nodes unavailable");
            }
        }
    }

    // Write PID file
    let pid = std::process::id();
    std::fs::write(&paths.core_pid, pid.to_string())
        .context("failed to write PID file")?;
    info!(pid, "wrote PID file");

    // Start API server
    let addr: SocketAddr = format!("{}:{}", config.core.host, config.core.api_port)
        .parse()
        .context("invalid bind address")?;
    let app = routes::router(state);
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "API server listening");

    // Serve until shutdown signal
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("API server error")?;

    // Cleanup
    bus.publish(Event::new(EventPayload::CoreStopping));
    // Brief delay to let persister flush
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    if paths.core_pid.exists() {
        let _ = std::fs::remove_file(&paths.core_pid);
    }
    info!("core daemon stopped");

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => info!("received Ctrl+C"),
        () = terminate => info!("received SIGTERM"),
    }
}
