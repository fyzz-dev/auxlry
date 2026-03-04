use std::sync::Arc;

use anyhow::{Context, Result};
use rig::completion::{Message, Prompt};
use rig::prelude::*;
use rig::providers::openrouter;
use tracing::{error, info, warn};

use crate::adapters::Adapter;
use crate::core::state::AppState;
use crate::events::types::{Event, EventPayload};
use crate::interface::router::PromptRouter;
use crate::interface::session::{BatchedInput, PendingMessage};
use crate::interface::tools::{
    DelegateOperatorTool, DelegationContext, DelegateSynapseTool,
};
use crate::interface::typing::TypingHandle;
use crate::operator::agent::OperatorAgent;
use crate::synapse::agent::SynapseAgent;

const MAX_RETRIES: usize = 2;

/// The Interface agent: receives batched messages and produces responses
/// using rig tool calling for delegation.
pub struct InterfaceAgent {
    state: AppState,
    prompt_router: PromptRouter,
    synapse: Arc<SynapseAgent>,
    operator: Arc<OperatorAgent>,
}

impl InterfaceAgent {
    pub fn new(
        state: AppState,
        synapse: Arc<SynapseAgent>,
        operator: Arc<OperatorAgent>,
    ) -> Result<Self> {
        let prompt_router = PromptRouter::new(&state.config.locale)?;
        Ok(Self {
            state,
            prompt_router,
            synapse,
            operator,
        })
    }

    /// Build a fresh rig agent with delegation tools.
    fn build_agent(
        &self,
        client: &openrouter::Client,
        model: &str,
        system_prompt: &str,
        ctx: &DelegationContext,
    ) -> rig::agent::Agent<openrouter::CompletionModel> {
        client
            .agent(model)
            .preamble(system_prompt)
            .tool(DelegateSynapseTool {
                synapse: self.synapse.clone(),
                ctx: ctx.clone(),
            })
            .tool(DelegateOperatorTool {
                operator: self.operator.clone(),
                ctx: ctx.clone(),
                registry: self.state.nodes.clone(),
            })
            .build()
    }

    /// Process a batch of messages using a rig agent with delegation tools.
    async fn process_batch(
        &self,
        batch: &BatchedInput,
        adapters: &[Arc<dyn Adapter>],
    ) -> Result<String> {
        let combined_input: String = batch
            .messages
            .iter()
            .map(|m| format!("{}: {}", m.author, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        let available_nodes = self.state.nodes.list().await;
        let system_prompt = self.prompt_router.render(
            "interface_default",
            minijinja::context! {
                adapter_name => &batch.interface,
                available_nodes => available_nodes,
            },
        )?;

        let api_key = &self.state.config.models.api_key;
        let model = &self.state.config.models.interface;

        let client: openrouter::Client =
            openrouter::Client::new(api_key).context("failed to create OpenRouter client")?;

        let ctx = DelegationContext {
            adapters: adapters.to_vec(),
            interface_name: batch.interface.clone(),
            channel: batch.channel.clone(),
            memory: self.state.memory.clone(),
            db: Some(self.state.db.clone()),
        };

        // Load conversation history from the database
        let stored = self
            .state
            .db
            .get_recent_messages(&batch.interface, &batch.channel, 50)
            .await
            .unwrap_or_default();

        let history: Vec<Message> = stored
            .iter()
            .map(|m| {
                if m.direction == "inbound" {
                    Message::user(format!("{}: {}", m.author, m.content))
                } else {
                    Message::assistant(&m.content)
                }
            })
            .collect();

        // Retry on transient API errors (e.g. OpenRouter response parse failures)
        let mut last_err = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                warn!(attempt = attempt + 1, "retrying interface after transient error");
                tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64)).await;
            }

            let agent = self.build_agent(&client, model, &system_prompt, &ctx);
            let mut attempt_history = history.clone();

            let result: Result<String, _> = agent
                .prompt(combined_input.as_str())
                .with_history(&mut attempt_history)
                .max_turns(3)
                .await;

            match result {
                Ok(response) => return Ok(response),
                Err(rig::completion::PromptError::MaxTurnsError { .. }) => {
                    return Err(anyhow::anyhow!("interface hit the turn limit"))
                        .context("interface prompt failed");
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    if err_msg.contains("JsonError") || err_msg.contains("ApiResponse") {
                        warn!(attempt = attempt + 1, error = %err_msg, "transient API error, will retry");
                        last_err = Some(err_msg);
                        continue;
                    }
                    return Err(anyhow::anyhow!(err_msg)).context("interface prompt failed");
                }
            }
        }

        Err(anyhow::anyhow!(last_err.unwrap_or_else(|| "unknown error".to_string())))
            .context("interface prompt failed after retries")
    }

    /// Send a response: persist, publish event, and deliver via adapter.
    async fn send_response(
        &self,
        adapters: &[Arc<dyn Adapter>],
        interface_name: &str,
        channel: &str,
        response: &str,
    ) {
        if let Err(e) = self
            .state
            .db
            .insert_message(interface_name, channel, "auxlry", response, "outbound")
            .await
        {
            warn!(error = %e, "failed to persist outbound message");
        }

        self.state
            .bus
            .publish(Event::new(EventPayload::InterfaceReply {
                interface: interface_name.to_string(),
                channel: channel.to_string(),
                content: response.to_string(),
            }));

        for adapter in adapters {
            if adapter.name() == interface_name {
                if let Err(e) = adapter.send_message(channel, response).await {
                    error!(error = %e, "failed to send reply via adapter");
                }
            }
        }
    }

    /// Run the interface loop: consume batched inputs, produce events.
    ///
    /// Batching only happens while the LLM is actively generating. Messages
    /// that arrive during generation for the same channel are collected and
    /// incorporated into a single re-generated reply. Messages for other
    /// channels are deferred and processed next.
    pub async fn run(
        self,
        mut batch_rx: tokio::sync::mpsc::Receiver<BatchedInput>,
        adapters: Vec<Arc<dyn Adapter>>,
    ) {
        info!("interface agent started");

        // Batches for other channels that arrived while we were generating.
        let mut deferred: Vec<BatchedInput> = Vec::new();

        loop {
            // Serve deferred batches first, otherwise wait for new input.
            let batch = if let Some(b) = deferred.pop() {
                b
            } else {
                match batch_rx.recv().await {
                    Some(b) => b,
                    None => break,
                }
            };

            let interface_name = batch.interface.clone();
            let channel = batch.channel.clone();

            // Start typing indicator while we process.
            let typing = TypingHandle::start(&adapters, &interface_name, &channel);

            let result = self.process_batch(&batch, &adapters).await;

            // Drain any batches that queued up during generation.
            let mut extra_same_channel: Vec<PendingMessage> = Vec::new();
            while let Ok(additional) = batch_rx.try_recv() {
                if additional.channel == channel && additional.interface == interface_name {
                    extra_same_channel.extend(additional.messages);
                } else {
                    deferred.push(additional);
                }
            }

            // If same-channel messages arrived mid-generation, re-generate once
            // with everything combined so the reply covers all input.
            let final_result = if !extra_same_channel.is_empty() {
                info!(
                    channel = %channel,
                    extra = extra_same_channel.len(),
                    "messages arrived during generation, re-processing"
                );
                let mut combined = batch;
                combined.messages.extend(extra_same_channel);
                self.process_batch(&combined, &adapters).await
            } else {
                result
            };

            match final_result {
                Ok(response) => {
                    self.send_response(&adapters, &interface_name, &channel, &response)
                        .await;
                }
                Err(e) => {
                    error!(error = ?e, "interface processing failed");
                    for adapter in &adapters {
                        if adapter.name() == interface_name {
                            let _ = adapter
                                .send_message(
                                    &channel,
                                    "Sorry, I ran into an issue. Want me to try again?",
                                )
                                .await;
                        }
                    }
                }
            }

            // Stop typing indicator.
            drop(typing);
        }
    }
}
