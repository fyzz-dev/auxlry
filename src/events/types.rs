use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Core event type — flows through the event bus and gets persisted to SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub payload: EventPayload,
}

impl Event {
    pub fn new(payload: EventPayload) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            payload,
        }
    }

    pub fn kind(&self) -> &'static str {
        self.payload.kind()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventPayload {
    // Core lifecycle
    CoreStarted,
    CoreStopping,

    // Messages (from adapters)
    MessageReceived {
        interface: String,
        channel: String,
        author: String,
        content: String,
    },
    MessageSent {
        interface: String,
        channel: String,
        content: String,
    },

    // Interface
    InterfaceAck {
        interface: String,
        channel: String,
        content: String,
    },
    InterfaceReply {
        interface: String,
        channel: String,
        content: String,
    },
    InterfaceDelegate {
        interface: String,
        channel: String,
        task_description: String,
        delegate_to: DelegateTarget,
        context: Option<String>,
    },

    // Synapse (thinker)
    SynapseStarted {
        synapse_id: String,
        task: String,
    },
    SynapseProgress {
        synapse_id: String,
        status: String,
    },
    SynapseCompleted {
        synapse_id: String,
        result: String,
    },
    SynapseFailed {
        synapse_id: String,
        error: String,
    },

    // Operator (actor)
    OperatorStarted {
        operator_id: String,
        task: String,
        node: String,
    },
    OperatorProgress {
        operator_id: String,
        status: String,
    },
    OperatorCompleted {
        operator_id: String,
        result: String,
    },
    OperatorFailed {
        operator_id: String,
        error: String,
    },

    // Node
    NodeConnected {
        node: String,
    },
    NodeDisconnected {
        node: String,
    },

    // Agent intervention
    AgentInterrupted {
        agent_id: String,
        kind: crate::core::signal::AgentKind,
        partial_result: String,
    },
    AgentSteered {
        agent_id: String,
        kind: crate::core::signal::AgentKind,
        message: String,
    },

    // Memory
    MemoryStored {
        key: String,
        summary: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DelegateTarget {
    Synapse,
    Operator,
}

impl EventPayload {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CoreStarted => "core_started",
            Self::CoreStopping => "core_stopping",
            Self::MessageReceived { .. } => "message_received",
            Self::MessageSent { .. } => "message_sent",
            Self::InterfaceAck { .. } => "interface_ack",
            Self::InterfaceReply { .. } => "interface_reply",
            Self::InterfaceDelegate { .. } => "interface_delegate",
            Self::SynapseStarted { .. } => "synapse_started",
            Self::SynapseProgress { .. } => "synapse_progress",
            Self::SynapseCompleted { .. } => "synapse_completed",
            Self::SynapseFailed { .. } => "synapse_failed",
            Self::OperatorStarted { .. } => "operator_started",
            Self::OperatorProgress { .. } => "operator_progress",
            Self::OperatorCompleted { .. } => "operator_completed",
            Self::OperatorFailed { .. } => "operator_failed",
            Self::NodeConnected { .. } => "node_connected",
            Self::NodeDisconnected { .. } => "node_disconnected",
            Self::AgentInterrupted { .. } => "agent_interrupted",
            Self::AgentSteered { .. } => "agent_steered",
            Self::MemoryStored { .. } => "memory_stored",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_serialization_roundtrip() {
        let event = Event::new(EventPayload::MessageReceived {
            interface: "discord".into(),
            channel: "general".into(),
            author: "user1".into(),
            content: "hello world".into(),
        });

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.kind(), "message_received");
        assert_eq!(deserialized.id, event.id);
    }

    #[test]
    fn all_event_kinds() {
        let variants = vec![
            EventPayload::CoreStarted,
            EventPayload::CoreStopping,
            EventPayload::MessageReceived {
                interface: "x".into(),
                channel: "y".into(),
                author: "z".into(),
                content: "w".into(),
            },
            EventPayload::SynapseStarted {
                synapse_id: "s1".into(),
                task: "think".into(),
            },
            EventPayload::OperatorStarted {
                operator_id: "o1".into(),
                task: "do".into(),
                node: "local".into(),
            },
            EventPayload::NodeConnected { node: "n1".into() },
            EventPayload::MemoryStored {
                key: "k".into(),
                summary: "s".into(),
            },
        ];

        for payload in variants {
            let event = Event::new(payload);
            assert!(!event.kind().is_empty());
            assert!(!event.id.is_empty());
        }
    }
}
