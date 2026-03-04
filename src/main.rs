use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "auxlry", version, about = "Agentic multi-node AI system")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage the core daemon
    Core {
        #[command(subcommand)]
        action: CoreAction,
    },
    /// Manage nodes
    Node {
        #[command(subcommand)]
        action: NodeAction,
    },
}

#[derive(Subcommand)]
enum CoreAction {
    /// Start the core daemon
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the core daemon
    Stop,
    /// Restart the core daemon
    Restart {
        #[arg(long)]
        foreground: bool,
    },
    /// Show core daemon status
    Status,
    /// Generate a one-time link code for a remote node
    Link,
}

#[derive(Subcommand)]
enum NodeAction {
    /// Start a node
    Start {
        /// Node name
        name: String,
    },
    /// Stop a node
    Stop {
        /// Node name
        name: String,
    },
    /// Link a remote node to this core
    Link {
        /// Core address (host:port)
        core_addr: String,
        /// One-time auth code
        code: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Init tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "auxlry=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Core { action } => match action {
            CoreAction::Start { foreground } => {
                auxlry::cli::core_cmd::start(foreground).await
            }
            CoreAction::Stop => auxlry::cli::core_cmd::stop().await,
            CoreAction::Restart { foreground } => {
                auxlry::cli::core_cmd::restart(foreground).await
            }
            CoreAction::Status => auxlry::cli::core_cmd::status().await,
            CoreAction::Link => auxlry::cli::core_cmd::link().await,
        },
        Commands::Node { action } => match action {
            NodeAction::Start { name } => auxlry::cli::node_cmd::start(&name).await,
            NodeAction::Stop { name } => auxlry::cli::node_cmd::stop(&name).await,
            NodeAction::Link { core_addr, code } => {
                auxlry::cli::node_cmd::link(&core_addr, &code).await
            }
        },
    }
}
