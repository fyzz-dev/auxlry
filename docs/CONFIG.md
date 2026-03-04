# Configuration Reference

auxlry looks for its config at `~/.auxlry/config.yml`. If none exists, a documented default is generated on first run. Environment variables (`${VAR}`) are interpolated at load time.

## All Options

| Key | Default | Description |
|-----|---------|-------------|
| `locale` | `en` | Prompt template locale |
| `core.host` | `0.0.0.0` | API bind address |
| `core.api_port` | `8400` | HTTP API port |
| `core.quic_port` | `8401` | QUIC transport port (node-to-node) |
| `core.stun_servers` | `[stun.l.google.com:19302]` | STUN servers for NAT traversal |
| `core.turn_server` | (none) | TURN relay server address |
| `core.turn_username` | (none) | TURN server username |
| `core.turn_credential` | (none) | TURN server credential |
| `models.provider` | `openrouter` | LLM provider |
| `models.api_key` | (empty) | Provider API key (`${OPENROUTER_API_KEY}`) |
| `models.interface` | `anthropic/claude-sonnet-4-20250514` | Model for the Interface agent |
| `models.synapse` | `anthropic/claude-sonnet-4-20250514` | Model for Synapse agents |
| `models.operator` | `anthropic/claude-sonnet-4-20250514` | Model for Operator agents |
| `memory.embedding_model` | `BAAI/bge-small-en-v1.5` | Local embedding model for vector memory |
| `memory.store_path` | `~/.auxlry/store/memory` | LanceDB vector store directory |
| `storage.database` | `~/.auxlry/store/auxlry.db` | SQLite database path |
| `concurrency.max_synapses` | `5` | Max concurrent Synapse (thinking) tasks |
| `concurrency.max_operators` | `10` | Max concurrent Operator (action) tasks |
| `concurrency.max_synapse_steps` | `5` | Max LLM rounds per Synapse task |
| `concurrency.max_operator_steps` | `10` | Max tool-use rounds per Operator task |

## Full Example

```yaml
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

## Adapter Types

### Discord

```yaml
adapter:
  type: discord
  token: ${DISCORD_TOKEN}
  channels: [general, help] # empty list = all channels
```

### Telegram

```yaml
adapter:
  type: telegram
  token: ${TELEGRAM_TOKEN}
```

### Webhook

```yaml
adapter:
  type: webhook
  url: https://example.com/hook
  secret: optional-hmac-secret
```

## Node Modes

- **`workspace`** (default) — file operations sandboxed to `~/.auxlry/workspace/<node-name>/`
- **`system`** — unrestricted filesystem access

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
