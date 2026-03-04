use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use super::executor::NodeExecutor;

/// Shared registry of all available nodes (local + connected remotes).
#[derive(Clone)]
pub struct NodeRegistry {
    nodes: Arc<RwLock<HashMap<String, Arc<dyn NodeExecutor>>>>,
}

impl NodeRegistry {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a node by name.
    pub async fn register(&self, name: String, node: Arc<dyn NodeExecutor>) {
        self.nodes.write().await.insert(name, node);
    }

    /// Unregister a node by name.
    pub async fn unregister(&self, name: &str) {
        self.nodes.write().await.remove(name);
    }

    /// Get a node by name.
    pub async fn get(&self, name: &str) -> Option<Arc<dyn NodeExecutor>> {
        self.nodes.read().await.get(name).cloned()
    }

    /// List all registered node names.
    pub async fn list(&self) -> Vec<String> {
        self.nodes.read().await.keys().cloned().collect()
    }

    /// Get the first registered node (typically "local").
    pub async fn first(&self) -> Option<Arc<dyn NodeExecutor>> {
        let nodes = self.nodes.read().await;
        // Prefer "local" if it exists, otherwise return any
        if let Some(node) = nodes.get("local") {
            return Some(node.clone());
        }
        nodes.values().next().cloned()
    }
}
