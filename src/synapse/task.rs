use serde::{Deserialize, Serialize};

/// Tracks a synapse task lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynapseTask {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Thinking,
    Completed,
    Failed,
}

impl SynapseTask {
    pub fn new(id: String, description: String) -> Self {
        Self {
            id,
            description,
            status: TaskStatus::Pending,
            result: None,
        }
    }
}
