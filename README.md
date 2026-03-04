# auxlry

## Our very WIP homegrown agent.

- Architecturally inspired by (Spacebot)[https://github.com/spacedriveapp/spacebot]

A multi-agent AI system in Rust. Users interact through chat platforms (Discord, Telegram, webhooks), and auxlry delegates work to specialized sub-agents that can think, act, and remember — across multiple machines.

```
You (Discord) ──> Interface Agent ──> Synapse (thinks) ──> Operator (acts on nodes)
                       │                                        │
                   Event Bus ◄──────────────────────────────────┘
                       │
                   SQLite + LanceDB
```

---

## Getting Started

### Prerequisites

- **Rust 1.82+** (edition 2024)
- An **OpenRouter API key** (or any provider rig-core supports)
- A **Discord bot token** (if using the Discord adapter)

### Build

```sh
git clone <repo-url> auxlry && cd auxlry
cargo build --release
```

The binary lands at `target/release/auxlry`.

### Configure

auxlry looks for its config at `~/.auxlry/config.yml`. Create it:

```sh
mkdir -p ~/.auxlry
```

```yaml
# ~/.auxlry/config.yml

locale: en

core:
  host: 0.0.0.0
  api_port: 8400
  quic_port: 8401
  stun_servers:
    - stun.l.google.com:19302
  # turn_server: turn.example.com:3478
  # turn_username: user
  # turn_credential: pass

models:
  provider: openrouter
  api_key: ${OPENROUTER_API_KEY}
  interface: anthropic/claude-sonnet-4-20250514
  synapse: anthropic/claude-sonnet-4-20250514
  operator: anthropic/claude-sonnet-4-20250514

interfaces:
  - name: discord-main
    adapter:
      type: discord
      token: ${DISCORD_TOKEN}
      channels: [] # empty = all channels

nodes:
  - name: local
    mode: workspace # sandbox file ops to ~/.auxlry/workspace/local/

memory:
  embedding_model: BAAI/bge-small-en-v1.5
  store_path: ~/.auxlry/store/memory

storage:
  database: ~/.auxlry/store/auxlry.db

concurrency:
  max_synapses: 5
  max_operators: 10
  max_synapse_steps: 5
  max_operator_steps: 10
```

Environment variables (`${VAR}`) are interpolated at load time. Set them however you like:

```sh
export OPENROUTER_API_KEY="sk-or-..."
export DISCORD_TOKEN="MTIz..."
```

If no config file exists, auxlry creates a default `~/.auxlry/config.yml` with all options documented. Edit it to add your API keys and adapters.

### Run

**Foreground** (see logs in terminal):

```sh
auxlry core start --foreground
```

**Background** (daemonize):

```sh
auxlry core start
```

**Check status:**

```sh
auxlry core status
# auxlry core is running (PID 12345)
#   API: http://localhost:8400
```

**Stop:**

```sh
auxlry core stop
```

**Restart:**

```sh
auxlry core restart
```

### Verify it works

Once the core is running:

```sh
# Health check
curl http://localhost:8400/health
# {"status":"ok"}

# System status
curl http://localhost:8400/dashboard/status
# {"status":"running","bus_receivers":1,"has_events":true,...}

# View current config (secrets redacted)
curl http://localhost:8400/config

# Recent events
curl http://localhost:8400/events/recent

# Swagger UI
open http://localhost:8400/swagger-ui/
```

**SSE event stream** (real-time — stays open):

```sh
curl -N http://localhost:8400/events
```

### Log verbosity

Controlled via `RUST_LOG`:

```sh
RUST_LOG=auxlry=debug auxlry core start --foreground
RUST_LOG=auxlry=trace,sqlx=warn auxlry core start --foreground
```

---

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

---

## API Endpoints

| Method | Path                     | Description                                                                              |
| ------ | ------------------------ | ---------------------------------------------------------------------------------------- |
| GET    | `/health`                | Health check (`{"status":"ok"}`)                                                         |
| GET    | `/events`                | SSE stream of real-time events                                                           |
| GET    | `/events/recent`         | Last 50 events from the database                                                         |
| GET    | `/dashboard/status`      | System overview (receivers, event count, nodes)                                          |
| GET    | `/config`                | Current config (no secrets)                                                              |
| GET    | `/memories/search?q=...` | Hybrid memory search (optional: `memory_type`, `min_importance`, `graph_depth`, `limit`) |
| POST   | `/memories`              | Store a memory (`content`, optional `source`, `memory_type`)                             |
| POST   | `/memories/edges`        | Create a graph edge between memories                                                     |
| GET    | `/memories/{id}/edges`   | Get all edges for a memory                                                               |
| GET    | `/swagger-ui/`           | Interactive API docs                                                                     |

---

## Config Reference

Every field has a default. You only need to specify what you want to change.

| Key                              | Default                              | Description                                |
| -------------------------------- | ------------------------------------ | ------------------------------------------ |
| `locale`                         | `en`                                 | Prompt template locale                     |
| `core.host`                      | `0.0.0.0`                            | API bind address                           |
| `core.api_port`                  | `8400`                               | HTTP API port                              |
| `core.quic_port`                 | `8401`                               | QUIC transport port (node-to-node)         |
| `core.stun_servers`              | `[stun.l.google.com:19302]`          | STUN servers for NAT traversal             |
| `core.turn_server`               | (none)                               | TURN relay server address                  |
| `core.turn_username`             | (none)                               | TURN server username                       |
| `core.turn_credential`           | (none)                               | TURN server credential                     |
| `models.provider`                | `openrouter`                         | LLM provider                               |
| `models.api_key`                 | (empty)                              | Provider API key (`${OPENROUTER_API_KEY}`) |
| `models.interface`               | `anthropic/claude-sonnet-4-20250514` | Model for the Interface agent              |
| `models.synapse`                 | `anthropic/claude-sonnet-4-20250514` | Model for Synapse agents                   |
| `models.operator`                | `anthropic/claude-sonnet-4-20250514` | Model for Operator agents                  |
| `memory.embedding_model`         | `BAAI/bge-small-en-v1.5`             | Local embedding model for vector memory    |
| `memory.store_path`              | `~/.auxlry/store/memory`             | LanceDB vector store directory             |
| `storage.database`               | `~/.auxlry/store/auxlry.db`          | SQLite database path                       |
| `concurrency.max_synapses`       | `5`                                  | Max concurrent Synapse (thinking) tasks    |
| `concurrency.max_operators`      | `10`                                 | Max concurrent Operator (action) tasks     |
| `concurrency.max_synapse_steps`  | `5`                                  | Max LLM rounds per Synapse task            |
| `concurrency.max_operator_steps` | `10`                                 | Max tool-use rounds per Operator task      |

### Adapter types

**Discord:**

```yaml
adapter:
  type: discord
  token: ${DISCORD_TOKEN}
  channels: [general, help] # empty list = all channels
```

**Telegram:**

```yaml
adapter:
  type: telegram
  token: ${TELEGRAM_TOKEN}
```

**Webhook:**

```yaml
adapter:
  type: webhook
  url: https://example.com/hook
  secret: optional-hmac-secret
```

### Node modes

- **`workspace`** (default) — file operations sandboxed to `~/.auxlry/workspace/<node-name>/`
- **`system`** — unrestricted filesystem access

---

## File System Layout

```
~/.auxlry/
├── config.yml              Main config
├── auxlry.log              Daemon log
├── process/
│   ├── core.pid            Core daemon PID
│   └── node-<name>.pid     Per-node PID files
├── store/
│   ├── auxlry.db           SQLite (events, messages, node tokens)
│   ├── token               This node's auth token (when acting as remote)
│   └── memory/             LanceDB vector data
└── workspace/
    └── <node-name>/        Sandboxed operator workspace per node
```

All directories are created automatically on first run.

---

## Architecture

### The big picture

auxlry is built around three types of agents, coordinated by an event bus:

```
┌──────────────────────────────────────────────────────────────┐
│                          CORE DAEMON                          │
│                                                               │
│  ┌──────────┐    ┌────────────┐    ┌────────────────────┐    │
│  │ Event Bus │───▶│  SQLite DB │    │    REST API        │    │
│  │(broadcast)│    │ (persist)  │    │ health/events/SSE  │    │
│  └─────┬─────┘    └────────────┘    └────────────────────┘    │
│        │                                                      │
│  ┌─────┴───────────────────────────────────────────────┐     │
│  │                   Interface Agent                    │     │
│  │   Messages in → smart batch → LLM → reply/delegate  │     │
│  └───────┬─────────────────┬───────────────────┬───────┘     │
│          │                 │                   │              │
│  ┌───────┴──────┐  ┌───────┴───────┐  ┌───────┴──────┐      │
│  │   Adapter    │  │    Synapse    │  │   Operator   │      │
│  │  (Discord)   │  │  (Thinker)   │  │   (Actor)    │      │
│  └──────────────┘  └──────────────┘  └───────┬──────┘      │
│                                              │              │
│  ┌──────────────┐  ┌─────────────────────────┴──────┐      │
│  │    Memory    │  │           Nodes                │      │
│  │  fastembed + │  │  local ◄── QUIC ──▶ remote     │      │
│  │   LanceDB   │  └────────────────────────────────┘      │
│  └──────────────┘                                          │
└──────────────────────────────────────────────────────────────┘
```

### Agent roles

**Interface** — the user-facing agent. It receives messages from adapters (Discord, Telegram, etc.), batches them with a debounce window, and decides what to do:

- **Reply** directly if it can answer
- **Delegate** to a Synapse (for thinking) or Operator (for actions), sending the user an immediate acknowledgment
- **Skip** if the message doesn't need a response

**Synapse** — the thinking agent. Handles reasoning, planning, analysis, and memory operations. Gets a task from the Interface, thinks through it with its own LLM call, and returns a structured result. Concurrency-limited by a semaphore.

**Operator** — the action agent. Executes concrete operations on nodes: reading files, writing files, running shell commands, listing directories, searching. Also concurrency-limited.

### Event bus

Everything communicates through a central `tokio::broadcast` channel (capacity 1024). When something happens — a message arrives, an agent starts thinking, a command finishes — an event is published to the bus. Every component subscribes and filters for what it cares about.

All events are also persisted to SQLite in a background task, giving you a complete history without blocking the bus.

Event types:

```
CoreStarted, CoreStopping
MessageReceived, MessageSent
InterfaceAck, InterfaceReply, InterfaceDelegate
SynapseStarted, SynapseProgress, SynapseCompleted, SynapseFailed
OperatorStarted, OperatorProgress, OperatorCompleted, OperatorFailed
NodeConnected, NodeDisconnected
MemoryStored
```

### Smart batching

The session manager prevents the Interface from responding to every single message individually. When a message arrives:

1. Start a debounce timer (configurable, default ~1.5s)
2. If more messages arrive within the window, collect them
3. Once the window elapses with no new messages, flush the batch as a single combined prompt to the LLM
4. This means rapid-fire "hey" / "can you help with X" / "here's the context" becomes one coherent request

### Nodes

A node is a machine where Operators can execute work. Each node implements the `NodeExecutor` trait:

```
run_command(command, cwd)    → ExecResult { stdout, stderr, exit_code }
read_file(path)              → FileContent { path, content }
write_file(path, content)    → ()
list_dir(path)               → Vec<DirEntry>
search_files(pattern, root)  → Vec<String>
```

**Local nodes** execute directly on the core's machine. In `workspace` mode, all file operations are sandboxed to `~/.auxlry/workspace/<node-name>/`.

**Remote nodes** connect over QUIC (quinn) with self-signed TLS certificates. Commands are serialized with bincode, sent over length-prefixed QUIC streams, and executed on the remote side. Authentication uses a one-time code that exchanges for a persistent token stored in SQLite.

### Networking

Node-to-node communication uses QUIC (quinn) with bincode serialization:

- **Self-signed TLS** via rcgen — no certificate authority needed
- **Length-prefixed framing** — 4-byte big-endian length + message body
- **NAT traversal** — STUN binding requests discover public addresses for hole-punching
- **Auth flow** — one-time code → persistent token, stored in `node_tokens` table

### Memory

auxlry has a hybrid memory system combining vector search, a knowledge graph, and earned importance scoring. Everything runs locally — no external services.

#### Dual storage

Memories live in two stores simultaneously:

- **LanceDB** (columnar vector store) — holds the content, embeddings, source, type, and creation timestamp. Used for semantic similarity search via the `BAAI/bge-small-en-v1.5` embedding model (384 dimensions, ONNX inference via fastembed).
- **SQLite** — holds metadata (`memory_metadata` table) and graph edges (`memory_edges` table). Tracks access counts, last-accessed timestamps, and typed relationships between memories.

When a memory is stored, it gets written to both: an embedded vector row in LanceDB and a metadata row in SQLite.

#### Memory types

Every memory is classified into one of six cognitive types:

| Type          | Description                              | Example                                   |
| ------------- | ---------------------------------------- | ----------------------------------------- |
| `fact`        | Verified information, definitions, specs | "The API version is 3.1"                  |
| `decision`    | Choices that were made                   | "We decided to use PostgreSQL"            |
| `inference`   | Conclusions drawn from evidence          | "This suggests latency is the bottleneck" |
| `preference`  | User or system preferences               | "I prefer dark mode by default"           |
| `observation` | General observations (default fallback)  | "The deployment went smoothly"            |
| `event`       | Things that happened at a point in time  | "Deployed v2.3 to production yesterday"   |

Types are assigned by a heuristic classifier (`classify_heuristic`) that pattern-matches on ~20 keyword/phrase patterns (e.g. "decided to" → Decision, "therefore" → Inference, "prefer" → Preference). If no pattern matches, the memory falls back to Observation. Agents and the API can also specify the type explicitly, overriding the heuristic.

#### Knowledge graph

Memories can be connected with typed, weighted edges stored in SQLite:

| Edge Type     | Meaning                         | Example                                     |
| ------------- | ------------------------------- | ------------------------------------------- |
| `related_to`  | General association             | Two memories about the same topic           |
| `supersedes`  | Source replaces target          | Updated config replaces old config          |
| `contradicts` | Source conflicts with target    | New finding conflicts with prior assumption |
| `caused_by`   | Source was caused by target     | An outage caused by a deployment            |
| `part_of`     | Source is a component of target | A subtask belonging to a larger task        |

Edges are upserted (duplicate source+target+type updates the weight instead of creating a duplicate). Both directions are traversed during search — if A→B exists, searching from B will also discover A.

#### Earned importance

Memories don't have arbitrary importance scores assigned by an LLM. Instead, importance is **earned** from three signals:

```
importance = recency × access × connectivity
```

- **Recency** — exponential decay with a 7-day half-life. A memory accessed today scores 1.0; after one week it's ~0.5; after two weeks ~0.25.
- **Access** — `ln(1 + access_count)`, floored at 0.1. Memories that are retrieved more often score higher. Every time a memory appears in search results, its access count increments.
- **Connectivity** — `ln(1 + edge_count)`, floored at 0.1. Memories with more graph connections score higher, reflecting their centrality in the knowledge base.

This means a frequently-accessed, well-connected, recently-used memory scores much higher than an isolated, stale one — without any LLM needing to guess at importance.

#### Hybrid search

Search combines three ranking signals using Reciprocal Rank Fusion (RRF):

1. **Vector search** — semantic similarity via LanceDB nearest-neighbor search. The query is embedded and compared against all stored memory vectors. Results are overfetched at 3× the requested limit.

2. **Graph expansion** — starting from the top vector results, the system traverses graph edges outward (configurable depth, default 1 hop). This surfaces memories that are semantically distant but structurally connected — e.g. a decision that's `related_to` a fact you searched for.

3. **RRF fusion** (k=60) — vector results and graph-discovered results are merged using rank-based scoring. Graph results contribute at 0.5× weight to avoid overwhelming vector relevance.

4. **Importance boost** — each candidate's RRF score is multiplied by `1.0 + (importance × 0.1)`, giving a gentle lift to high-importance memories without overriding relevance.

5. **Filtering** — results can be filtered by memory type and minimum importance. Access is recorded for all returned results (feeding back into the importance score).

The full pipeline in pseudocode:

```
vector_results = vector_search(query, limit * 3)
graph_ids = traverse_edges(top_vector_ids, depth=graph_depth)
graph_results = fetch_by_ids(graph_ids)
merged = rrf_fuse(vector_results, graph_results, k=60)
boosted = apply_importance_boost(merged)
filtered = apply_type_filter_and_min_importance(boosted)
record_access(filtered[:limit])
return filtered[:limit]
```

#### Agent access

The three agent tiers have different levels of memory access:

| Agent         | Search                                        | Store               | Create Edges              |
| ------------- | --------------------------------------------- | ------------------- | ------------------------- |
| **Interface** | Automatic (injects context before delegating) | —                   | —                         |
| **Synapse**   | `memory_search` tool                          | `memory_store` tool | `create_memory_edge` tool |
| **Operator**  | `memory_search` tool (read-only)              | —                   | —                         |

- The **Interface** automatically searches memory for context before delegating tasks to Synapse or Operator. This happens transparently — the Interface doesn't need to explicitly decide to search.
- **Synapse** has full read/write access: it can search, store new memories, and create edges between them. This is where knowledge management happens — the Synapse decides what's worth remembering and how memories relate.
- **Operator** has read-only access: it can search memory to recall relevant context (e.g. "what was the database password format we decided on?") but cannot modify the knowledge base.

#### API endpoints

| Method | Path                     | Description                                                                                |
| ------ | ------------------------ | ------------------------------------------------------------------------------------------ |
| GET    | `/memories/search?q=...` | Hybrid search with optional `memory_type`, `min_importance`, `graph_depth`, `limit` params |
| POST   | `/memories`              | Store a memory (`content`, optional `source` and `memory_type`)                            |
| POST   | `/memories/edges`        | Create an edge (`source_id`, `target_id`, `relation_type`, optional `weight`)              |
| GET    | `/memories/{id}/edges`   | Get all edges for a memory                                                                 |

### Prompt templates

System prompts are MiniJinja templates (`.md.j2` files) in `prompts/<locale>/`. Variables like `{{ adapter_name }}`, `{{ task_description }}`, `{{ memory_context }}` are injected at render time. You can customize them by editing the files or adding new locales.

```
prompts/
└── en/
    ├── interface/default.md.j2
    ├── synapse/default.md.j2
    ├── operator/default.md.j2
    └── adapters/
        ├── discord.md.j2
        ├── telegram.md.j2
        └── webhook.md.j2
```

---

## Module Map

```
src/
├── main.rs                 CLI entrypoint (clap)
├── lib.rs                  Re-exports all modules
├── cli/
│   ├── core_cmd.rs         auxlry core start|stop|restart|status
│   └── node_cmd.rs         auxlry node start|stop|link
├── config/
│   ├── types.rs            Config structs with serde defaults
│   └── loader.rs           YAML loading + ${ENV} interpolation
├── events/
│   ├── types.rs            Event + EventPayload enum
│   ├── bus.rs              tokio::broadcast wrapper
│   └── persist.rs          Background SQLite writer
├── storage/
│   ├── database.rs         SQLite pool, migrations, CRUD
│   └── paths.rs            XDG-aware path resolution
├── core/
│   ├── state.rs            AppState (config + bus + db + paths)
│   └── daemon.rs           Daemon lifecycle, PID, graceful shutdown
├── api/
│   └── routes.rs           Axum router, SSE, Swagger UI
├── interface/
│   ├── agent.rs            Interface LLM agent (reply/skip/delegate)
│   ├── session.rs          Smart batching + debounce
│   └── router.rs           MiniJinja prompt template rendering
├── adapters/
│   └── discord.rs          Serenity-based Discord adapter
├── synapse/
│   ├── agent.rs            Thinker agent (reasoning, planning)
│   └── task.rs             Synapse task lifecycle
├── operator/
│   ├── agent.rs            Actor agent (tool execution)
│   ├── task.rs             Operator task lifecycle
│   └── tools.rs            Tool definitions (read/write/run/list/search)
├── node/
│   ├── executor.rs         NodeExecutor trait
│   ├── local.rs            Local node with sandbox
│   ├── remote.rs           Remote node over QUIC
│   ├── protocol.rs         ProtocolMessage enum (bincode)
│   └── linking.rs          One-time code auth + token persistence
├── network/
│   ├── quic.rs             Quinn server/client, self-signed TLS
│   ├── transport.rs        Send/receive over QUIC streams
│   └── hole_punch.rs       STUN discovery + NAT traversal
└── memory/
    ├── store.rs            fastembed + LanceDB storage + fetch_by_ids
    ├── search.rs           Hybrid vector + graph search with RRF fusion
    ├── types.rs            MemoryType enum + heuristic classifier
    ├── graph.rs            EdgeType, MemoryEdge, SQLite CRUD for edges/metadata
    ├── importance.rs       Earned importance scoring (recency × access × connectivity)
    └── tools.rs            rig Tool impls (MemorySearchTool, MemoryStoreTool, CreateEdgeTool)
```

---

## Tech Stack

| Component  | Crate                  | Purpose                                    |
| ---------- | ---------------------- | ------------------------------------------ |
| Runtime    | `tokio`                | Async everywhere                           |
| LLM        | `rig-core`             | OpenRouter (extensible to other providers) |
| API        | `axum` + `utoipa`      | REST, SSE, Swagger UI                      |
| Database   | `sqlx` (SQLite)        | Events, messages, tokens                   |
| Config     | `serde_yaml`           | YAML with env interpolation                |
| CLI        | `clap`                 | Derive-based subcommands                   |
| Discord    | `serenity`             | Gateway + HTTP adapter                     |
| Transport  | `quinn` + `bincode`    | QUIC streams with binary serialization     |
| TLS        | `rustls` + `rcgen`     | Self-signed certs for node auth            |
| Templates  | `minijinja`            | `.md.j2` system prompts                    |
| Embeddings | `fastembed`            | Local ONNX inference (BGE-small)           |
| Vectors    | `lancedb`              | Local columnar vector store                |
| Errors     | `anyhow` + `thiserror` | Anyhow at boundaries, thiserror in modules |

---

## Development

```sh
# Run tests
cargo test

# Check without building
cargo check

# Run with debug logging
RUST_LOG=auxlry=debug cargo run -- core start --foreground
```

### Tests

34 unit tests cover:

- Config loading and env var interpolation
- Event serialization roundtrips
- Event bus pub/sub
- SQLite event and message CRUD
- Node token storage
- All API endpoints (health, dashboard, config, events, memory)
- Memory type heuristic classification (all 6 types + fallback)
- Memory type serde roundtrip
- Graph edge CRUD, upsert, and edge counting
- Memory access tracking
- Importance scoring (decay curve, zero-access floor, connectivity boost, SQLite timestamp format)
