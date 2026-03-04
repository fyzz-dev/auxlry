use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use serde_json::json;
use tracing::info;

use crate::adapters::Adapter;
use crate::memory::search::SearchParams;
use crate::memory::store::MemoryStore;
use crate::operator::agent::OperatorAgent;
use crate::storage::database::Database;
use crate::synapse::agent::SynapseAgent;

/// Error type for interface tools.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct InterfaceToolError(pub String);

/// Shared context for interface delegation tools.
#[derive(Clone)]
pub struct DelegationContext {
    pub adapters: Vec<Arc<dyn Adapter>>,
    pub interface_name: String,
    pub channel: String,
    pub memory: Option<Arc<MemoryStore>>,
    pub db: Option<Database>,
}

impl DelegationContext {
    /// Send the LLM-generated ack to the user so they know we're working on it.
    async fn send_ack(&self, ack: &str) {
        let ack = ack.trim();
        if ack.is_empty() {
            return;
        }
        for adapter in &self.adapters {
            if adapter.name() == self.interface_name {
                let _ = adapter.send_message(&self.channel, ack).await;
            }
        }
    }

    /// Search memory for relevant context, using hybrid search when DB is available.
    async fn memory_context(&self, query: &str) -> Option<String> {
        let mem = self.memory.as_ref()?;

        let results = if let Some(ref db) = self.db {
            let params = SearchParams {
                limit: 5,
                type_filter: None,
                min_importance: 0.0,
                graph_depth: 1,
            };
            mem.hybrid_search(query, &params, db).await.ok()?
        } else {
            mem.search(query, 5).await.ok()?
        };

        if results.is_empty() {
            return None;
        }

        let ctx: String = results
            .iter()
            .map(|r| format!("- [{}] {} (source: {})", r.memory_type, r.content, r.source))
            .collect::<Vec<_>>()
            .join("\n");
        Some(ctx)
    }
}

// ── DelegateSynapse ─────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DelegateSynapseArgs {
    pub task: String,
    #[serde(default)]
    pub ack: String,
}

/// Tool that delegates a thinking/analysis task to a Synapse agent.
pub struct DelegateSynapseTool {
    pub synapse: Arc<SynapseAgent>,
    pub ctx: DelegationContext,
}

impl Tool for DelegateSynapseTool {
    const NAME: &'static str = "delegate_synapse";
    type Error = InterfaceToolError;
    type Args = DelegateSynapseArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "delegate_synapse".to_string(),
            description: "Delegate a thinking, analysis, or research task to a Synapse agent. The Synapse can reason deeply and also delegate sub-tasks to Operators if needed.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "A clear description of what to think about or analyze"
                    },
                    "ack": {
                        "type": "string",
                        "description": "A brief, natural acknowledgment to send the user while you work. Should fit the context of what they asked. e.g. 'Let me think about that.' or 'Good question, give me a sec.'"
                    }
                },
                "required": ["task", "ack"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!(task = %args.task, "interface delegating to synapse");
        self.ctx.send_ack(&args.ack).await;

        let memory_ctx = self.ctx.memory_context(&args.task).await;

        self.synapse
            .run(&args.task, memory_ctx.as_deref())
            .await
            .map_err(|e| InterfaceToolError(e.to_string()))
    }
}

// ── DelegateOperator ────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DelegateOperatorArgs {
    pub task: String,
    #[serde(default)]
    pub ack: String,
}

/// Tool that delegates an action task directly to an Operator agent.
pub struct DelegateOperatorTool {
    pub operator: Arc<OperatorAgent>,
    pub ctx: DelegationContext,
}

impl Tool for DelegateOperatorTool {
    const NAME: &'static str = "delegate_operator";
    type Error = InterfaceToolError;
    type Args = DelegateOperatorArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "delegate_operator".to_string(),
            description: "Delegate an action task to an Operator agent. Use this for running commands, reading/writing files, or any task that requires executing something on a machine.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "A clear description of the action to perform"
                    },
                    "ack": {
                        "type": "string",
                        "description": "A brief, natural acknowledgment to send the user while you work. Should fit the context of what they asked. e.g. 'Checking that now.' or 'On it, one sec.'"
                    }
                },
                "required": ["task", "ack"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!(task = %args.task, "interface delegating to operator");
        self.ctx.send_ack(&args.ack).await;

        self.operator
            .run(&args.task)
            .await
            .map_err(|e| InterfaceToolError(e.to_string()))
    }
}
