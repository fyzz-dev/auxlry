use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use serde_json::json;

use super::graph::EdgeType;
use super::search::SearchParams;
use super::store::MemoryStore;
use super::types::{MemoryType, classify_heuristic};
use crate::events::bus::EventBus;
use crate::events::types::{Event, EventPayload};
use crate::storage::database::Database;

/// Shared error type for memory tools.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct MemoryToolError(pub String);

// ── MemorySearchTool ────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct MemorySearchArgs {
    pub query: String,
    #[serde(default)]
    pub memory_type: Option<String>,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

fn default_search_limit() -> usize {
    5
}

/// Tool for searching memories with hybrid vector + graph search.
pub struct MemorySearchTool {
    pub memory: Arc<MemoryStore>,
    pub db: Database,
}

impl Tool for MemorySearchTool {
    const NAME: &'static str = "memory_search";
    type Error = MemoryToolError;
    type Args = MemorySearchArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "memory_search".to_string(),
            description: "Search long-term memory using hybrid semantic + graph search. \
                Returns relevant memories ranked by similarity, graph connectivity, and importance. \
                Use this to recall facts, decisions, preferences, past events, and inferences. \
                Memories that are accessed more often and have more connections rank higher."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Natural language search query describing what you want to recall"
                    },
                    "memory_type": {
                        "type": "string",
                        "enum": ["fact", "decision", "inference", "preference", "observation", "event"],
                        "description": "Optional: filter results to a specific memory type",
                        "nullable": true
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 5)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let type_filter = args
            .memory_type
            .as_deref()
            .and_then(MemoryType::from_str);

        let params = SearchParams {
            limit: args.limit,
            type_filter,
            min_importance: 0.0,
            graph_depth: 1,
        };

        let results = self
            .memory
            .hybrid_search(&args.query, &params, &self.db)
            .await
            .map_err(|e| MemoryToolError(e.to_string()))?;

        if results.is_empty() {
            return Ok("No relevant memories found.".to_string());
        }

        let formatted: Vec<String> = results
            .iter()
            .map(|r| {
                format!(
                    "[{}] ({}, score: {:.2}) {}",
                    r.id, r.memory_type, r.score, r.content
                )
            })
            .collect();

        Ok(formatted.join("\n"))
    }
}

// ── MemoryStoreTool ─────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct MemoryStoreArgs {
    pub content: String,
    #[serde(default)]
    pub memory_type: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

/// Tool for storing new memories with automatic type classification.
pub struct MemoryStoreTool {
    pub memory: Arc<MemoryStore>,
    pub db: Database,
    pub bus: EventBus,
}

impl Tool for MemoryStoreTool {
    const NAME: &'static str = "memory_store";
    type Error = MemoryToolError;
    type Args = MemoryStoreArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "memory_store".to_string(),
            description: "Store information in long-term memory for future recall. \
                Memories are automatically classified by type (fact, decision, inference, \
                preference, observation, event) if not specified. Store important facts, \
                decisions made, user preferences, inferences drawn, and notable events. \
                Each memory gets a unique ID that can be used to create graph edges."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The information to remember. Be concise but include key details."
                    },
                    "memory_type": {
                        "type": "string",
                        "enum": ["fact", "decision", "inference", "preference", "observation", "event"],
                        "description": "Optional: explicit memory type. If omitted, type is auto-classified from content.",
                        "nullable": true
                    },
                    "source": {
                        "type": "string",
                        "description": "Optional: where this information came from (e.g. 'discord:general', 'user:alice')",
                        "nullable": true
                    }
                },
                "required": ["content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let memory_type = args
            .memory_type
            .as_deref()
            .and_then(MemoryType::from_str)
            .unwrap_or_else(|| classify_heuristic(&args.content));

        let id = uuid::Uuid::new_v4().to_string();

        self.memory
            .store(&id, &args.content, args.source.as_deref(), memory_type)
            .await
            .map_err(|e| MemoryToolError(e.to_string()))?;

        self.db
            .init_memory_metadata(&id, memory_type.as_str())
            .await
            .map_err(|e| MemoryToolError(e.to_string()))?;

        self.bus.publish(Event::new(EventPayload::MemoryStored {
            key: id.clone(),
            summary: args.content.chars().take(100).collect(),
        }));

        Ok(format!(
            "Stored memory {id} (type: {memory_type})"
        ))
    }
}

// ── MemoryUpdateTool ────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct MemoryUpdateArgs {
    pub id: String,
    pub content: String,
    #[serde(default)]
    pub memory_type: Option<String>,
}

/// Tool for updating an existing memory (re-embeds content).
pub struct MemoryUpdateTool {
    pub memory: Arc<MemoryStore>,
    pub db: Database,
    pub bus: EventBus,
}

impl Tool for MemoryUpdateTool {
    const NAME: &'static str = "memory_update";
    type Error = MemoryToolError;
    type Args = MemoryUpdateArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "memory_update".to_string(),
            description: "Update an existing memory's content. The old embedding is replaced with \
                a new one computed from the updated content. Use this when a fact has changed, \
                content needs correction, or information needs to be refined. Provide the memory ID \
                from a previous search result."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The ID of the memory to update (from search results)"
                    },
                    "content": {
                        "type": "string",
                        "description": "The new content for this memory"
                    },
                    "memory_type": {
                        "type": "string",
                        "enum": ["fact", "decision", "inference", "preference", "observation", "event"],
                        "description": "Optional: change the memory type",
                        "nullable": true
                    }
                },
                "required": ["id", "content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let memory_type = args
            .memory_type
            .as_deref()
            .and_then(MemoryType::from_str)
            .unwrap_or_else(|| classify_heuristic(&args.content));

        self.memory
            .update(&args.id, &args.content, None, memory_type)
            .await
            .map_err(|e| MemoryToolError(e.to_string()))?;

        self.db
            .update_memory_type(&args.id, memory_type.as_str())
            .await
            .map_err(|e| MemoryToolError(e.to_string()))?;

        self.bus.publish(Event::new(EventPayload::MemoryStored {
            key: args.id.clone(),
            summary: args.content.chars().take(100).collect(),
        }));

        Ok(format!("Updated memory {}", args.id))
    }
}

// ── MemoryDeleteTool ────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct MemoryDeleteArgs {
    pub id: String,
}

/// Tool for deleting a memory and its graph edges.
pub struct MemoryDeleteTool {
    pub memory: Arc<MemoryStore>,
    pub db: Database,
}

impl Tool for MemoryDeleteTool {
    const NAME: &'static str = "memory_delete";
    type Error = MemoryToolError;
    type Args = MemoryDeleteArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "memory_delete".to_string(),
            description: "Delete a memory and all its graph edges. Use this when a memory is \
                completely wrong or no longer relevant. Prefer using a 'supersedes' edge over \
                deletion when the old info is still useful context. Provide the memory ID from \
                a previous search result."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "The ID of the memory to delete (from search results)"
                    }
                },
                "required": ["id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.memory
            .delete(&args.id)
            .await
            .map_err(|e| MemoryToolError(e.to_string()))?;

        self.db
            .delete_edges_for(&args.id)
            .await
            .map_err(|e| MemoryToolError(e.to_string()))?;

        self.db
            .delete_memory_metadata(&args.id)
            .await
            .map_err(|e| MemoryToolError(e.to_string()))?;

        Ok(format!("Deleted memory {} and its edges", args.id))
    }
}

// ── CreateEdgeTool ──────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateEdgeArgs {
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
    #[serde(default = "default_weight")]
    pub weight: f64,
}

fn default_weight() -> f64 {
    1.0
}

/// Tool for creating typed edges between memories in the knowledge graph.
pub struct CreateEdgeTool {
    pub db: Database,
}

impl Tool for CreateEdgeTool {
    const NAME: &'static str = "create_memory_edge";
    type Error = MemoryToolError;
    type Args = CreateEdgeArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "create_memory_edge".to_string(),
            description: "Create a typed relationship between two memories in the knowledge graph. \
                This helps connect related information so that searching for one memory also surfaces \
                connected memories. Use after storing related memories to build the knowledge graph. \
                Relation types: 'related_to' (general connection), 'supersedes' (source replaces target), \
                'contradicts' (source conflicts with target), 'caused_by' (source was caused by target), \
                'part_of' (source is a component of target)."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source_id": {
                        "type": "string",
                        "description": "ID of the source memory"
                    },
                    "target_id": {
                        "type": "string",
                        "description": "ID of the target memory"
                    },
                    "relation_type": {
                        "type": "string",
                        "enum": ["related_to", "supersedes", "contradicts", "caused_by", "part_of"],
                        "description": "Type of relationship between the memories"
                    },
                    "weight": {
                        "type": "number",
                        "description": "Strength of the relationship (default: 1.0)",
                        "default": 1.0
                    }
                },
                "required": ["source_id", "target_id", "relation_type"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let edge_type = EdgeType::from_str(&args.relation_type).ok_or_else(|| {
            MemoryToolError(format!(
                "invalid relation type '{}' — must be one of: related_to, supersedes, contradicts, caused_by, part_of",
                args.relation_type
            ))
        })?;

        self.db
            .create_edge(&args.source_id, &args.target_id, edge_type, args.weight)
            .await
            .map_err(|e| MemoryToolError(e.to_string()))?;

        Ok(format!(
            "Created edge: {} --[{}]--> {}",
            args.source_id, args.relation_type, args.target_id
        ))
    }
}
