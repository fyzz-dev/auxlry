use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub locale: String,
    pub core: CoreConfig,
    pub models: ModelsConfig,
    pub interfaces: Vec<InterfaceConfig>,
    pub nodes: Vec<NodeConfig>,
    pub memory: MemoryConfig,
    pub storage: StorageConfig,
    pub concurrency: ConcurrencyConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            locale: "en".into(),
            core: CoreConfig::default(),
            models: ModelsConfig::default(),
            interfaces: vec![],
            nodes: vec![NodeConfig {
                name: "local".into(),
                mode: NodeMode::Workspace,
            }],
            memory: MemoryConfig::default(),
            storage: StorageConfig::default(),
            concurrency: ConcurrencyConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CoreConfig {
    pub host: String,
    pub api_port: u16,
    pub quic_port: u16,
    pub stun_servers: Vec<String>,
    pub turn_server: Option<String>,
    pub turn_username: Option<String>,
    pub turn_credential: Option<String>,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".into(),
            api_port: 8400,
            quic_port: 8401,
            stun_servers: vec!["stun.l.google.com:19302".into()],
            turn_server: None,
            turn_username: None,
            turn_credential: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelsConfig {
    pub provider: String,
    pub api_key: String,
    pub interface: String,
    pub synapse: String,
    pub operator: String,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            provider: "openrouter".into(),
            api_key: String::new(),
            interface: "anthropic/claude-sonnet-4-20250514".into(),
            synapse: "anthropic/claude-sonnet-4-20250514".into(),
            operator: "anthropic/claude-sonnet-4-20250514".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceConfig {
    pub name: String,
    pub adapter: AdapterConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AdapterConfig {
    Discord {
        token: String,
        #[serde(default)]
        channels: Vec<String>,
    },
    Telegram {
        token: String,
    },
    Webhook {
        url: String,
        #[serde(default)]
        secret: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub name: String,
    #[serde(default)]
    pub mode: NodeMode,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeMode {
    #[default]
    Workspace,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryConfig {
    pub embedding_model: String,
    pub store_path: String,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            embedding_model: "BAAI/bge-small-en-v1.5".into(),
            store_path: "~/.auxlry/store/memory".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub database: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            database: "~/.auxlry/store/auxlry.db".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConcurrencyConfig {
    pub max_synapses: usize,
    pub max_operators: usize,
    pub max_operator_steps: usize,
    pub max_synapse_steps: usize,
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            max_synapses: 5,
            max_operators: 10,
            max_operator_steps: 10,
            max_synapse_steps: 5,
        }
    }
}
