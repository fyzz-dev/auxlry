# auxlry

A multi-agent AI system built in Rust. Users interact through chat platforms (Discord, Telegram, webhooks), and auxlry delegates work to specialized sub-agents that can think, act, and remember — across multiple machines.

> **Status:** Early development (v0.1.0). Expect breaking changes.

```
You (Discord) ──▶ Interface Agent ──▶ Synapse (thinks) ──▶ Operator (acts)
                        │                                        │
                    Event Bus ◄──────────────────────────────────┘
                        │
                  SQLite + LanceDB
```

## Features

- **Three-tier agent architecture** — Interface routes, Synapse reasons, Operator executes
- **Chat platform adapters** — Discord, Telegram, webhooks
- **Distributed nodes** — run operators on remote machines over QUIC with NAT traversal
- **Hybrid memory** — vector search (fastembed + LanceDB) + knowledge graph + earned importance scoring
- **Smart batching** — debounces rapid-fire messages into coherent requests
- **Event-driven** — broadcast bus with SQLite persistence and real-time SSE streaming
- **REST API** — health, events, memory, config endpoints with Swagger UI
- **Customizable prompts** — MiniJinja templates per agent and adapter

## Quick Start

### Prerequisites

- **Rust 1.82+** (edition 2024)
- An [OpenRouter](https://openrouter.ai/) API key (or any provider rig-core supports)
- A Discord bot token (if using the Discord adapter)

### Build

```sh
git clone git@github.com:fyzz-dev/auxlry.git && cd auxlry
cargo build --release
```

### Configure

```sh
export OPENROUTER_API_KEY="sk-or-..."
export DISCORD_TOKEN="MTIz..."       # optional
```

auxlry auto-generates a documented config at `~/.auxlry/config.yml` on first run. Environment variables (`${VAR}`) are interpolated at load time. Edit the config to add your API keys and adapters.

### Run

```sh
# Foreground (recommended for first run)
auxlry core start --foreground

# Background daemon
auxlry core start

# Check status
auxlry core status

# Stop
auxlry core stop
```

### Verify

```sh
curl http://localhost:8400/health
# {"status":"ok"}

# Interactive API docs
open http://localhost:8400/swagger-ui/
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        CORE DAEMON                          │
│                                                             │
│  Event Bus ──▶ SQLite DB          REST API (axum)           │
│      │                            /health /events /swagger  │
│      │                                                      │
│  Interface Agent                                            │
│    messages in → batch → LLM → reply / delegate             │
│      │              │              │                         │
│  Adapters       Synapse          Operator                   │
│  (Discord)     (Thinker)        (Actor)                     │
│                                    │                        │
│  Memory                          Nodes                      │
│  fastembed + LanceDB     local ◄─ QUIC ─▶ remote            │
└─────────────────────────────────────────────────────────────┘
```

| Agent | Role | Memory Access |
|-------|------|---------------|
| **Interface** | Routes messages, delegates tasks | Auto-searches for context |
| **Synapse** | Reasoning, planning, analysis | Full read/write + graph edges |
| **Operator** | Executes file/shell operations on nodes | Read-only search |

## CLI Reference

```
auxlry core start [--foreground]    Start the core daemon
auxlry core stop                    Stop the core daemon
auxlry core restart [--foreground]  Restart the core daemon
auxlry core status                  Show running status

auxlry node start <name>            Start a node
auxlry node stop <name>             Stop a node
auxlry node link <addr> <code>      Link a remote node via one-time code
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/events` | SSE event stream |
| GET | `/events/recent` | Last 50 persisted events |
| GET | `/dashboard/status` | System overview |
| GET | `/config` | Current config (secrets redacted) |
| GET | `/memories/search?q=...` | Hybrid memory search |
| POST | `/memories` | Store a memory |
| POST | `/memories/edges` | Create a graph edge |
| GET | `/memories/{id}/edges` | Get edges for a memory |
| GET | `/swagger-ui/` | Interactive API docs |

## Configuration

Every field has a default. You only need to set what you want to change. See the [full config reference](docs/CONFIG.md) for all options.

Minimal example:

```yaml
# ~/.auxlry/config.yml
models:
  api_key: ${OPENROUTER_API_KEY}

interfaces:
  - name: discord-main
    adapter:
      type: discord
      token: ${DISCORD_TOKEN}
```

## Tech Stack

| Component | Crate | Purpose |
|-----------|-------|---------|
| Runtime | `tokio` | Async runtime |
| LLM | `rig-core` | OpenRouter / multi-provider |
| API | `axum` + `utoipa` | REST, SSE, Swagger UI |
| Database | `sqlx` (SQLite) | Events, messages, tokens |
| Discord | `serenity` | Gateway + HTTP |
| Transport | `quinn` + `bincode` | QUIC with binary serialization |
| Embeddings | `fastembed` | Local ONNX inference |
| Vectors | `lancedb` | Columnar vector store |
| Templates | `minijinja` | Agent prompt templates |

## Development

```sh
cargo test           # Run tests (34 unit tests)
cargo check          # Type-check without building
cargo clippy         # Lint

# Debug logging
RUST_LOG=auxlry=debug cargo run -- core start --foreground
```

## Project Structure

```
src/
├── cli/          CLI entrypoint (clap)
├── config/       YAML loading + env interpolation
├── events/       Event bus, types, SQLite persistence
├── storage/      SQLite pool, migrations, paths
├── core/         AppState, daemon lifecycle
├── api/          Axum routes, SSE, Swagger
├── interface/    Interface agent, session batching, prompt routing
├── adapters/     Discord (Telegram, webhook planned)
├── synapse/      Thinker agent + task lifecycle
├── operator/     Actor agent + tools (read/write/run/list/search)
├── node/         NodeExecutor trait, local/remote, QUIC protocol
├── network/      QUIC transport, TLS, NAT traversal
└── memory/       Vector store, graph, importance scoring, agent tools
```

## Contributing

Contributions are welcome. Please open an issue first to discuss what you'd like to change.

## License

TBD
