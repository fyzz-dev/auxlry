use serde::{Deserialize, Serialize};

/// Tracks an operator task lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorTask {
    pub id: String,
    pub description: String,
    pub node: String,
    pub status: TaskStatus,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl OperatorTask {
    pub fn new(id: String, description: String, node: String) -> Self {
        Self {
            id,
            description,
            node,
            status: TaskStatus::Pending,
            result: None,
        }
    }
}
