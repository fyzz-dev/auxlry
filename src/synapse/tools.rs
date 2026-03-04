use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use serde_json::json;

use crate::operator::agent::OperatorAgent;

/// Error type for synapse tools.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SynapseToolError(pub String);

// ── DelegateOperator ────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DelegateOperatorArgs {
    pub task: String,
}

/// Tool that lets a Synapse delegate an action task to an Operator.
pub struct DelegateOperatorTool {
    pub operator: Arc<OperatorAgent>,
}

impl Tool for DelegateOperatorTool {
    const NAME: &'static str = "delegate_operator";
    type Error = SynapseToolError;
    type Args = DelegateOperatorArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "delegate_operator".to_string(),
            description: "Delegate an action task to an Operator agent. Use this for file operations, running commands, or any task that requires executing something on a machine.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "A clear description of the action to perform"
                    }
                },
                "required": ["task"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.operator
            .run(&args.task)
            .await
            .map_err(|e| SynapseToolError(e.to_string()))
    }
}
