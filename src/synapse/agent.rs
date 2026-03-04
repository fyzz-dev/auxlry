use std::sync::Arc;

use anyhow::{Context, Result};
use rig::completion::Message;
use rig::prelude::*;
use rig::providers::openrouter;
use tokio::sync::Semaphore;
use tracing::{info, warn};
use uuid::Uuid;

use crate::core::signal::{extract_last_assistant_text, AgentHook, AgentKind, AgentSignal};
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
                registry: self.state.nodes.clone(),
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
    ///
    /// Accepts optional `interface` and `channel` so the agent can be found
    /// by channel for chat-based steering.
    pub async fn run(
        &self,
        task: &str,
        memory_context: Option<&str>,
        conversation: Option<&str>,
        interface: Option<&str>,
        channel: Option<&str>,
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

        // Register in active agents registry
        let (signal_rx, turns_counter) = self
            .state
            .active_agents
            .register(
                synapse_id.clone(),
                AgentKind::Synapse,
                task.to_string(),
                interface.map(String::from),
                channel.map(String::from),
            )
            .await;

        let result = self
            .run_inner(
                &synapse_id,
                task,
                memory_context,
                conversation,
                max_turns,
                signal_rx,
                turns_counter,
            )
            .await;

        // Always deregister
        self.state.active_agents.deregister(&synapse_id).await;
        result
    }

    /// Inner run loop that handles retries, wrap-up, and steering.
    async fn run_inner(
        &self,
        synapse_id: &str,
        task: &str,
        memory_context: Option<&str>,
        conversation: Option<&str>,
        max_turns: usize,
        signal_rx: tokio::sync::watch::Receiver<AgentSignal>,
        turns_counter: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Result<String> {
        let available_nodes = self.state.nodes.list().await;
        let system_prompt = self.prompt_router.render(
            "synapse_default",
            minijinja::context! {
                task_description => task,
                memory_context => memory_context.unwrap_or(""),
                conversation_history => conversation.unwrap_or(""),
                available_nodes => available_nodes,
            },
        )?;

        let api_key = &self.state.config.models.api_key;
        let model = &self.state.config.models.synapse;

        use rig::completion::Prompt;

        let client: openrouter::Client =
            openrouter::Client::new(api_key).context("failed to create OpenRouter client")?;

        let mut remaining_turns = max_turns;
        let mut chat_history: Vec<Message> = Vec::new();
        let mut current_prompt = task.to_string();

        loop {
            let hook = AgentHook::new(signal_rx.clone(), turns_counter.clone());

            // Retry on transient API errors
            let mut last_err = None;
            let mut prompt_result = None;

            for attempt in 0..=MAX_RETRIES {
                if attempt > 0 {
                    warn!(
                        synapse_id = %synapse_id,
                        attempt = attempt + 1,
                        "retrying synapse after transient error"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64))
                        .await;
                }

                let agent = self.build_agent(&client, model, &system_prompt);
                let mut attempt_history = chat_history.clone();

                let result: Result<String, _> = agent
                    .prompt(&current_prompt)
                    .with_hook(hook.clone())
                    .with_history(&mut attempt_history)
                    .max_turns(remaining_turns)
                    .await;

                match result {
                    Ok(response) => {
                        prompt_result = Some(Ok(response));
                        break;
                    }
                    Err(rig::completion::PromptError::MaxTurnsError { .. }) => {
                        prompt_result = Some(Err(rig::completion::PromptError::MaxTurnsError {
                            max_turns: remaining_turns,
                            chat_history: Box::new(Vec::new()),
                            prompt: Box::new(Message::user("")),
                        }));
                        break;
                    }
                    Err(rig::completion::PromptError::PromptCancelled {
                        chat_history: history,
                        reason,
                    }) => {
                        prompt_result = Some(Err(
                            rig::completion::PromptError::PromptCancelled {
                                chat_history: history,
                                reason,
                            },
                        ));
                        break;
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
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
                        prompt_result = Some(Err(e));
                        break;
                    }
                }
            }

            let result = match prompt_result {
                Some(r) => r,
                None => {
                    // All retries exhausted
                    let err_msg = last_err.unwrap_or_else(|| "unknown error".to_string());
                    self.state
                        .bus
                        .publish(Event::new(EventPayload::SynapseFailed {
                            synapse_id: synapse_id.to_string(),
                            error: err_msg.clone(),
                        }));
                    return Err(anyhow::anyhow!(err_msg));
                }
            };

            match result {
                Ok(response) => {
                    info!(synapse_id = %synapse_id, "synapse completed");
                    self.state
                        .bus
                        .publish(Event::new(EventPayload::SynapseCompleted {
                            synapse_id: synapse_id.to_string(),
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
                            synapse_id: synapse_id.to_string(),
                            result: msg.clone(),
                        }));
                    return Ok(msg);
                }
                Err(rig::completion::PromptError::PromptCancelled {
                    chat_history: history,
                    reason,
                }) => {
                    if reason == "wrap-up" {
                        // Extract partial result from history
                        let partial = extract_last_assistant_text(&history);
                        let result = if partial.is_empty() {
                            "Agent was wrapped up before producing output.".to_string()
                        } else {
                            partial
                        };
                        info!(synapse_id = %synapse_id, "synapse wrapped up");
                        self.state.bus.publish(Event::new(
                            EventPayload::AgentInterrupted {
                                agent_id: synapse_id.to_string(),
                                kind: AgentKind::Synapse,
                                partial_result: result.clone(),
                            },
                        ));
                        return Ok(result);
                    } else if let Some(steer_msg) = reason.strip_prefix("steer:") {
                        // Steer: append user message, reduce turns, loop
                        info!(
                            synapse_id = %synapse_id,
                            steer = %steer_msg,
                            "synapse steered"
                        );
                        self.state
                            .bus
                            .publish(Event::new(EventPayload::AgentSteered {
                                agent_id: synapse_id.to_string(),
                                kind: AgentKind::Synapse,
                                message: steer_msg.to_string(),
                            }));

                        // Reset signal
                        let _ = self
                            .state
                            .active_agents
                            .signal(synapse_id, AgentSignal::None)
                            .await;

                        // Set up next iteration with history + steer message
                        let used = turns_counter
                            .load(std::sync::atomic::Ordering::Relaxed);
                        remaining_turns = max_turns.saturating_sub(used);
                        if remaining_turns == 0 {
                            remaining_turns = 1;
                        }
                        chat_history = *history;
                        current_prompt = steer_msg.to_string();
                        continue;
                    } else {
                        // Unknown cancellation reason
                        let partial = extract_last_assistant_text(&history);
                        self.state
                            .bus
                            .publish(Event::new(EventPayload::SynapseFailed {
                                synapse_id: synapse_id.to_string(),
                                error: reason.clone(),
                            }));
                        return Ok(if partial.is_empty() {
                            format!("Synapse cancelled: {reason}")
                        } else {
                            partial
                        });
                    }
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    self.state
                        .bus
                        .publish(Event::new(EventPayload::SynapseFailed {
                            synapse_id: synapse_id.to_string(),
                            error: err_msg.clone(),
                        }));
                    return Err(anyhow::anyhow!(err_msg));
                }
            }
        }
    }
}
