use std::sync::Arc;

use anyhow::{Context, Result};
use rig::prelude::*;
use rig::providers::openrouter;
use tokio::sync::Semaphore;
use tracing::{info, warn};
use uuid::Uuid;

use crate::core::state::AppState;
use crate::events::types::{Event, EventPayload};
use crate::interface::router::PromptRouter;
use crate::memory::tools::{
    CreateEdgeTool, MemoryDeleteTool, MemorySearchTool, MemoryStoreTool, MemoryUpdateTool,
};
use crate::operator::agent::OperatorAgent;
use crate::synapse::tools::DelegateOperatorTool;

const MAX_RETRIES: usize = 2;

/// The Synapse agent: handles reasoning, planning, and analysis tasks.
/// Can delegate action tasks to Operators via rig tool calling.
pub struct SynapseAgent {
    state: AppState,
    operator: Arc<OperatorAgent>,
    prompt_router: PromptRouter,
    semaphore: Arc<Semaphore>,
}

impl SynapseAgent {
    pub fn new(state: AppState, operator: Arc<OperatorAgent>) -> Result<Self> {
        let prompt_router = PromptRouter::new(&state.config.locale)?;
        let max = state.config.concurrency.max_synapses;
        Ok(Self {
            state,
            operator,
            prompt_router,
            semaphore: Arc::new(Semaphore::new(max)),
        })
    }

    /// Build a fresh rig agent with all tools attached.
    fn build_agent(
        &self,
        client: &rig::providers::openrouter::Client,
        model: &str,
        system_prompt: &str,
    ) -> rig::agent::Agent<rig::providers::openrouter::CompletionModel> {
        let mut agent_builder = client
            .agent(model)
            .preamble(system_prompt)
            .tool(DelegateOperatorTool {
                operator: self.operator.clone(),
            });

        if let Some(ref memory) = self.state.memory {
            agent_builder = agent_builder
                .tool(MemorySearchTool {
                    memory: memory.clone(),
                    db: self.state.db.clone(),
                })
                .tool(MemoryStoreTool {
                    memory: memory.clone(),
                    db: self.state.db.clone(),
                    bus: self.state.bus.clone(),
                })
                .tool(CreateEdgeTool {
                    db: self.state.db.clone(),
                })
                .tool(MemoryUpdateTool {
                    memory: memory.clone(),
                    db: self.state.db.clone(),
                    bus: self.state.bus.clone(),
                })
                .tool(MemoryDeleteTool {
                    memory: memory.clone(),
                    db: self.state.db.clone(),
                });
        }

        agent_builder.build()
    }

    /// Run a synapse task with operator delegation capability.
    pub async fn run(
        &self,
        task: &str,
        memory_context: Option<&str>,
        conversation: Option<&str>,
    ) -> Result<String> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .context("semaphore closed")?;

        let synapse_id = Uuid::new_v4().to_string();
        let max_turns = self.state.config.concurrency.max_synapse_steps;

        self.state
            .bus
            .publish(Event::new(EventPayload::SynapseStarted {
                synapse_id: synapse_id.clone(),
                task: task.to_string(),
            }));

        let system_prompt = self.prompt_router.render(
            "synapse_default",
            minijinja::context! {
                task_description => task,
                memory_context => memory_context.unwrap_or(""),
                conversation_history => conversation.unwrap_or(""),
            },
        )?;

        let api_key = &self.state.config.models.api_key;
        let model = &self.state.config.models.synapse;

        use rig::completion::Prompt;

        let client: openrouter::Client =
            openrouter::Client::new(api_key).context("failed to create OpenRouter client")?;

        // Retry on transient API errors (e.g. OpenRouter response parse failures)
        let mut last_err = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                warn!(
                    synapse_id = %synapse_id,
                    attempt = attempt + 1,
                    "retrying synapse after transient error"
                );
                tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64)).await;
            }

            let agent = self.build_agent(&client, model, &system_prompt);
            let result: Result<String, _> = agent.prompt(task).max_turns(max_turns).await;

            match result {
                Ok(response) => {
                    info!(synapse_id = %synapse_id, "synapse completed");
                    self.state
                        .bus
                        .publish(Event::new(EventPayload::SynapseCompleted {
                            synapse_id,
                            result: response.clone(),
                        }));
                    return Ok(response);
                }
                Err(rig::completion::PromptError::MaxTurnsError { .. }) => {
                    let msg = format!("Synapse hit the step limit ({max_turns}).");
                    warn!(synapse_id = %synapse_id, "{msg}");
                    self.state
                        .bus
                        .publish(Event::new(EventPayload::SynapseCompleted {
                            synapse_id,
                            result: msg.clone(),
                        }));
                    return Ok(msg);
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    // Retry on JSON/completion errors (transient API issues)
                    if err_msg.contains("JsonError") || err_msg.contains("ApiResponse") {
                        warn!(
                            synapse_id = %synapse_id,
                            attempt = attempt + 1,
                            error = %err_msg,
                            "transient API error, will retry"
                        );
                        last_err = Some(err_msg);
                        continue;
                    }
                    // Non-retryable error
                    self.state
                        .bus
                        .publish(Event::new(EventPayload::SynapseFailed {
                            synapse_id,
                            error: err_msg.clone(),
                        }));
                    return Err(anyhow::anyhow!(err_msg));
                }
            }
        }

        // All retries exhausted
        let err_msg = last_err.unwrap_or_else(|| "unknown error".to_string());
        self.state
            .bus
            .publish(Event::new(EventPayload::SynapseFailed {
                synapse_id,
                error: err_msg.clone(),
            }));
        Err(anyhow::anyhow!(err_msg))
    }
}
