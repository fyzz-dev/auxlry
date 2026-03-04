use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use rig::agent::{HookAction, PromptHook, ToolCallHookAction};
use rig::completion::Message;
use rig::providers::openrouter;
use serde::Serialize;
use tokio::sync::{RwLock, watch};

// ── Signal types ──────────────────────────────────────────────────────

/// Signal that can be sent to a running agent.
#[derive(Debug, Clone)]
pub enum AgentSignal {
    /// No signal — agent continues normally.
    None,
    /// Gracefully terminate and return partial results.
    WrapUp,
    /// Inject additional context; agent restarts with the message appended.
    Steer(String),
}

/// Which kind of agent is running.
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Synapse,
    Operator,
}

/// Public summary of an active agent (no sender exposed).
#[derive(Debug, Clone, Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub kind: AgentKind,
    pub task: String,
    pub interface: Option<String>,
    pub channel: Option<String>,
    pub started_at_secs: u64,
    pub turns_used: usize,
}

// ── Active agent entry (internal) ─────────────────────────────────────

struct ActiveAgent {
    kind: AgentKind,
    task: String,
    interface: Option<String>,
    channel: Option<String>,
    started_at: Instant,
    signal_tx: watch::Sender<AgentSignal>,
    turns_used: Arc<AtomicUsize>,
}

// ── Registry ──────────────────────────────────────────────────────────

/// Registry of currently-running agents. Lives in AppState.
#[derive(Clone)]
pub struct ActiveAgents {
    inner: Arc<RwLock<HashMap<String, ActiveAgent>>>,
}

impl ActiveAgents {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new active agent. Returns a watch receiver for the hook
    /// and a shared turn counter.
    pub async fn register(
        &self,
        id: String,
        kind: AgentKind,
        task: String,
        interface: Option<String>,
        channel: Option<String>,
    ) -> (watch::Receiver<AgentSignal>, Arc<AtomicUsize>) {
        let (tx, rx) = watch::channel(AgentSignal::None);
        let turns = Arc::new(AtomicUsize::new(0));
        let entry = ActiveAgent {
            kind,
            task,
            interface,
            channel,
            started_at: Instant::now(),
            signal_tx: tx,
            turns_used: turns.clone(),
        };
        self.inner.write().await.insert(id, entry);
        (rx, turns)
    }

    /// Remove an agent from the registry.
    pub async fn deregister(&self, id: &str) {
        self.inner.write().await.remove(id);
    }

    /// Send a signal to an agent by ID.
    pub async fn signal(&self, id: &str, signal: AgentSignal) -> anyhow::Result<()> {
        let map = self.inner.read().await;
        let agent = map
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("no active agent with id '{id}'"))?;
        agent
            .signal_tx
            .send(signal)
            .map_err(|_| anyhow::anyhow!("agent receiver dropped"))?;
        Ok(())
    }

    /// Send a signal to the agent running on a specific interface+channel.
    pub async fn signal_channel(
        &self,
        interface: &str,
        channel: &str,
        signal: AgentSignal,
    ) -> anyhow::Result<()> {
        let map = self.inner.read().await;
        for agent in map.values() {
            if agent.interface.as_deref() == Some(interface)
                && agent.channel.as_deref() == Some(channel)
            {
                agent
                    .signal_tx
                    .send(signal)
                    .map_err(|_| anyhow::anyhow!("agent receiver dropped"))?;
                return Ok(());
            }
        }
        Err(anyhow::anyhow!(
            "no active agent on {interface}:{channel}"
        ))
    }

    /// List all active agents (public summary).
    pub async fn list(&self) -> Vec<AgentInfo> {
        let map = self.inner.read().await;
        map.iter()
            .map(|(id, a)| AgentInfo {
                id: id.clone(),
                kind: a.kind,
                task: a.task.clone(),
                interface: a.interface.clone(),
                channel: a.channel.clone(),
                started_at_secs: a.started_at.elapsed().as_secs(),
                turns_used: a.turns_used.load(Ordering::Relaxed),
            })
            .collect()
    }
}

// ── PromptHook implementation ─────────────────────────────────────────

/// Hook that checks for wrap-up / steer signals on each agent turn.
#[derive(Clone)]
pub struct AgentHook {
    signal_rx: watch::Receiver<AgentSignal>,
    turns_used: Arc<AtomicUsize>,
}

impl AgentHook {
    pub fn new(signal_rx: watch::Receiver<AgentSignal>, turns_used: Arc<AtomicUsize>) -> Self {
        Self {
            signal_rx,
            turns_used,
        }
    }

    fn check_signal(&self) -> HookAction {
        let signal = self.signal_rx.borrow().clone();
        match signal {
            AgentSignal::None => HookAction::Continue,
            AgentSignal::WrapUp => HookAction::terminate("wrap-up"),
            AgentSignal::Steer(ref msg) => {
                HookAction::terminate(format!("steer:{msg}"))
            }
        }
    }
}

impl PromptHook<openrouter::CompletionModel> for AgentHook {
    async fn on_completion_call(
        &self,
        _prompt: &Message,
        _history: &[Message],
    ) -> HookAction {
        self.turns_used.fetch_add(1, Ordering::Relaxed);
        self.check_signal()
    }

    async fn on_tool_call(
        &self,
        _tool_name: &str,
        _tool_call_id: Option<String>,
        _internal_call_id: &str,
        _args: &str,
    ) -> ToolCallHookAction {
        match self.check_signal() {
            HookAction::Continue => ToolCallHookAction::Continue,
            HookAction::Terminate { reason } => ToolCallHookAction::Terminate { reason },
        }
    }
}

/// Extract the last assistant text from a chat history.
pub fn extract_last_assistant_text(history: &[Message]) -> String {
    for msg in history.iter().rev() {
        if let Message::Assistant { content, .. } = msg {
            // Collect and check in reverse since we want the last text
            let parts: Vec<_> = content.iter().collect();
            for part in parts.iter().rev() {
                if let rig::message::AssistantContent::Text(text) = part {
                    return text.text.clone();
                }
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_and_list() {
        let agents = ActiveAgents::new();
        let (_rx, _turns) = agents
            .register(
                "test-1".into(),
                AgentKind::Synapse,
                "think about stuff".into(),
                Some("discord".into()),
                Some("#general".into()),
            )
            .await;

        let list = agents.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "test-1");
        assert_eq!(list[0].kind, AgentKind::Synapse);
    }

    #[tokio::test]
    async fn signal_by_id() {
        let agents = ActiveAgents::new();
        let (mut rx, _turns) = agents
            .register(
                "test-2".into(),
                AgentKind::Operator,
                "do stuff".into(),
                None,
                None,
            )
            .await;

        agents
            .signal("test-2", AgentSignal::WrapUp)
            .await
            .unwrap();
        rx.changed().await.unwrap();
        assert!(matches!(*rx.borrow(), AgentSignal::WrapUp));
    }

    #[tokio::test]
    async fn signal_by_channel() {
        let agents = ActiveAgents::new();
        let (mut rx, _turns) = agents
            .register(
                "test-3".into(),
                AgentKind::Synapse,
                "analyze".into(),
                Some("discord".into()),
                Some("#dev".into()),
            )
            .await;

        agents
            .signal_channel("discord", "#dev", AgentSignal::Steer("new info".into()))
            .await
            .unwrap();
        rx.changed().await.unwrap();
        assert!(matches!(*rx.borrow(), AgentSignal::Steer(_)));
    }

    #[tokio::test]
    async fn deregister_removes() {
        let agents = ActiveAgents::new();
        agents
            .register("test-4".into(), AgentKind::Synapse, "t".into(), None, None)
            .await;
        agents.deregister("test-4").await;
        assert!(agents.list().await.is_empty());
    }

    #[tokio::test]
    async fn signal_unknown_agent_fails() {
        let agents = ActiveAgents::new();
        assert!(agents.signal("nope", AgentSignal::WrapUp).await.is_err());
    }
}
