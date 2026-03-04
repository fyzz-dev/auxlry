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
use crate::memory::tools::MemorySearchTool;
use crate::node::executor::NodeExecutor;
use crate::node::registry::NodeRegistry;
use crate::operator::tools::*;

const MAX_RETRIES: usize = 2;

/// The Operator agent: executes actions on nodes using rig tool calling.
pub struct OperatorAgent {
    state: AppState,
    nodes: NodeRegistry,
    prompt_router: PromptRouter,
    semaphore: Arc<Semaphore>,
}

impl OperatorAgent {
    pub fn new(state: AppState, nodes: NodeRegistry) -> Result<Self> {
        let max = state.config.concurrency.max_operators;
        let prompt_router = PromptRouter::new(&state.config.locale)?;
        Ok(Self {
            state,
            nodes,
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
        node: Arc<dyn NodeExecutor>,
    ) -> rig::agent::Agent<rig::providers::openrouter::CompletionModel> {
        let mut agent_builder = client
            .agent(model)
            .preamble(system_prompt)
            .tool(ReadFileTool { node: node.clone() })
            .tool(WriteFileTool { node: node.clone() })
            .tool(RunCommandTool { node: node.clone() })
            .tool(ListDirTool { node: node.clone() })
            .tool(SearchFilesTool { node: node.clone() });

        if let Some(ref memory) = self.state.memory {
            agent_builder = agent_builder.tool(MemorySearchTool {
                memory: memory.clone(),
                db: self.state.db.clone(),
            });
        }

        agent_builder.build()
    }

    /// Resolve a node by name, falling back to the first registered node.
    async fn resolve_node(&self, node_name: Option<&str>) -> Result<Arc<dyn NodeExecutor>> {
        if let Some(name) = node_name {
            if let Some(node) = self.nodes.get(name).await {
                return Ok(node);
            }
            anyhow::bail!("node '{}' not found in registry", name);
        }
        self.nodes
            .first()
            .await
            .ok_or_else(|| anyhow::anyhow!("no nodes registered"))
    }

    /// Run an operator task: build a rig agent with node tools and let it execute.
    ///
    /// Accepts optional `interface` and `channel` so the agent can be found
    /// by channel for chat-based steering.
    pub async fn run(
        &self,
        task: &str,
        node_name: Option<&str>,
        interface: Option<&str>,
        channel: Option<&str>,
    ) -> Result<String> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .context("semaphore closed")?;

        let node = self.resolve_node(node_name).await?;

        let op_id = Uuid::new_v4().to_string();
        let max_turns = self.state.config.concurrency.max_operator_steps;

        self.state
            .bus
            .publish(Event::new(EventPayload::OperatorStarted {
                operator_id: op_id.clone(),
                task: task.to_string(),
                node: node.name().to_string(),
            }));

        // Register in active agents registry
        let (signal_rx, turns_counter) = self
            .state
            .active_agents
            .register(
                op_id.clone(),
                AgentKind::Operator,
                task.to_string(),
                interface.map(String::from),
                channel.map(String::from),
            )
            .await;

        let result = self
            .run_inner(
                &op_id,
                task,
                node,
                max_turns,
                signal_rx,
                turns_counter,
            )
            .await;

        // Always deregister
        self.state.active_agents.deregister(&op_id).await;
        result
    }

    /// Inner run loop that handles retries, wrap-up, and steering.
    async fn run_inner(
        &self,
        op_id: &str,
        task: &str,
        node: Arc<dyn NodeExecutor>,
        max_turns: usize,
        signal_rx: tokio::sync::watch::Receiver<AgentSignal>,
        turns_counter: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Result<String> {
        let system_prompt = self.prompt_router.render(
            "operator_default",
            minijinja::context! {
                task_description => task,
                node_name => node.name(),
            },
        )?;

        let api_key = &self.state.config.models.api_key;
        let model = &self.state.config.models.operator;

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
                        op_id = %op_id,
                        attempt = attempt + 1,
                        "retrying operator after transient error"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64))
                        .await;
                }

                let agent = self.build_agent(&client, model, &system_prompt, node.clone());
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
                                op_id = %op_id,
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
                    let err_msg = last_err.unwrap_or_else(|| "unknown error".to_string());
                    self.state
                        .bus
                        .publish(Event::new(EventPayload::OperatorFailed {
                            operator_id: op_id.to_string(),
                            error: err_msg.clone(),
                        }));
                    return Err(anyhow::anyhow!(err_msg));
                }
            };

            match result {
                Ok(response) => {
                    info!(op_id = %op_id, "operator completed");
                    self.state
                        .bus
                        .publish(Event::new(EventPayload::OperatorCompleted {
                            operator_id: op_id.to_string(),
                            result: response.clone(),
                        }));
                    return Ok(response);
                }
                Err(rig::completion::PromptError::MaxTurnsError { .. }) => {
                    let msg = format!("Operator hit the step limit ({max_turns}) — partial results may be available in the conversation.");
                    warn!(op_id = %op_id, "{msg}");
                    self.state
                        .bus
                        .publish(Event::new(EventPayload::OperatorCompleted {
                            operator_id: op_id.to_string(),
                            result: msg.clone(),
                        }));
                    return Ok(msg);
                }
                Err(rig::completion::PromptError::PromptCancelled {
                    chat_history: history,
                    reason,
                }) => {
                    if reason == "wrap-up" {
                        let partial = extract_last_assistant_text(&history);
                        let result = if partial.is_empty() {
                            "Agent was wrapped up before producing output.".to_string()
                        } else {
                            partial
                        };
                        info!(op_id = %op_id, "operator wrapped up");
                        self.state.bus.publish(Event::new(
                            EventPayload::AgentInterrupted {
                                agent_id: op_id.to_string(),
                                kind: AgentKind::Operator,
                                partial_result: result.clone(),
                            },
                        ));
                        return Ok(result);
                    } else if let Some(steer_msg) = reason.strip_prefix("steer:") {
                        info!(
                            op_id = %op_id,
                            steer = %steer_msg,
                            "operator steered"
                        );
                        self.state
                            .bus
                            .publish(Event::new(EventPayload::AgentSteered {
                                agent_id: op_id.to_string(),
                                kind: AgentKind::Operator,
                                message: steer_msg.to_string(),
                            }));

                        // Reset signal
                        let _ = self
                            .state
                            .active_agents
                            .signal(op_id, AgentSignal::None)
                            .await;

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
                        let partial = extract_last_assistant_text(&history);
                        self.state
                            .bus
                            .publish(Event::new(EventPayload::OperatorFailed {
                                operator_id: op_id.to_string(),
                                error: reason.clone(),
                            }));
                        return Ok(if partial.is_empty() {
                            format!("Operator cancelled: {reason}")
                        } else {
                            partial
                        });
                    }
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    self.state
                        .bus
                        .publish(Event::new(EventPayload::OperatorFailed {
                            operator_id: op_id.to_string(),
                            error: err_msg.clone(),
                        }));
                    return Err(anyhow::anyhow!(err_msg));
                }
            }
        }
    }
}
