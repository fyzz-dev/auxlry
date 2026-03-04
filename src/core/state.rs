use std::sync::Arc;

use crate::config::types::Config;
use crate::events::bus::EventBus;
use crate::memory::store::MemoryStore;
use crate::node::registry::NodeRegistry;
use crate::storage::database::Database;
use crate::storage::paths::AuxlryPaths;

/// Shared application state available to all components.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub bus: EventBus,
    pub db: Database,
    pub paths: AuxlryPaths,
    pub memory: Option<Arc<MemoryStore>>,
    pub nodes: NodeRegistry,
}

impl AppState {
    pub fn new(
        config: Config,
        bus: EventBus,
        db: Database,
        paths: AuxlryPaths,
        memory: Option<Arc<MemoryStore>>,
        nodes: NodeRegistry,
    ) -> Self {
        Self {
            config: Arc::new(config),
            bus,
            db,
            paths,
            memory,
            nodes,
        }
    }
}
