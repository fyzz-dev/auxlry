use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use serde_json::json;

use crate::node::registry::NodeRegistry;
use crate::operator::agent::OperatorAgent;

/// Error type for synapse tools.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SynapseToolError(pub String);

// ── DelegateOperator ────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DelegateOperatorArgs {
    pub task: String,
    #[serde(default)]
    pub node: Option<String>,
}

/// Tool that lets a Synapse delegate an action task to an Operator.
pub struct DelegateOperatorTool {
    pub operator: Arc<OperatorAgent>,
    pub registry: NodeRegistry,
}

impl Tool for DelegateOperatorTool {
    const NAME: &'static str = "delegate_operator";
    type Error = SynapseToolError;
    type Args = DelegateOperatorArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let node_names = self.registry.list().await;
        let node_desc = if node_names.is_empty() {
            "Target node name (optional, defaults to local)".to_string()
        } else {
            format!(
                "Target node name. Available: {}. Defaults to local if omitted.",
                node_names.join(", ")
            )
        };

        ToolDefinition {
            name: "delegate_operator".to_string(),
            description: "Delegate an action task to an Operator agent. Use this for file operations, running commands, or any task that requires executing something on a machine.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "A clear description of the action to perform"
                    },
                    "node": {
                        "type": "string",
                        "description": node_desc
                    }
                },
                "required": ["task"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.operator
            .run(&args.task, args.node.as_deref())
            .await
            .map_err(|e| SynapseToolError(e.to_string()))
    }
}
