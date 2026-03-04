use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    Router,
    extract::{Query, State},
    response::{
        Json,
        sse::{Event as SseEvent, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures::stream::Stream;
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::core::state::AppState;

#[derive(OpenApi)]
#[openapi(
    paths(health, events_stream, recent_events, dashboard_status, config_info, memory_search, memory_store, memory_create_edge, memory_get_edges),
    info(title = "auxlry API", version = "0.1.0")
)]
struct ApiDoc;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/events", get(events_stream))
        .route("/events/recent", get(recent_events))
        .route("/dashboard/status", get(dashboard_status))
        .route("/config", get(config_info))
        .route("/memories/search", get(memory_search))
        .route("/memories", post(memory_store))
        .route("/memories/edges", post(memory_create_edge))
        .route("/memories/{id}/edges", get(memory_get_edges))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .with_state(Arc::new(state))
}

/// Health check endpoint.
#[utoipa::path(get, path = "/health", responses((status = 200, description = "OK")))]
async fn health() -> Json<serde_json::Value> {
    Json(json!({"status": "ok"}))
}

/// SSE stream of real-time events.
#[utoipa::path(get, path = "/events", responses((status = 200, description = "SSE event stream")))]
async fn events_stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = state.bus.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result: Result<_, _>| match result {
        Ok(event) => {
            let data = serde_json::to_string(&event).unwrap_or_default();
            Some(Ok(SseEvent::default().event(event.kind()).data(data)))
        }
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Get recent events from the database.
#[utoipa::path(get, path = "/events/recent", responses((status = 200, description = "Recent events")))]
async fn recent_events(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    match state.db.recent_events(50).await {
        Ok(events) => Json(json!({"events": events})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

/// Dashboard status — overview of the system.
#[utoipa::path(get, path = "/dashboard/status", responses((status = 200, description = "System status")))]
async fn dashboard_status(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let event_count = state
        .db
        .recent_events(1)
        .await
        .map(|e| !e.is_empty())
        .unwrap_or(false);

    Json(json!({
        "status": "running",
        "bus_receivers": state.bus.receiver_count(),
        "has_events": event_count,
        "interfaces": state.config.interfaces.len(),
        "nodes": state.config.nodes.len(),
    }))
}

/// Get current configuration (sanitized — no secrets).
#[utoipa::path(get, path = "/config", responses((status = 200, description = "Current config")))]
async fn config_info(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    Json(json!({
        "locale": state.config.locale,
        "core": {
            "host": state.config.core.host,
            "api_port": state.config.core.api_port,
        },
        "models": {
            "provider": state.config.models.provider,
            "interface": state.config.models.interface,
            "synapse": state.config.models.synapse,
            "operator": state.config.models.operator,
        },
        "interfaces": state.config.interfaces.iter().map(|i| &i.name).collect::<Vec<_>>(),
        "nodes": state.config.nodes.iter().map(|n| &n.name).collect::<Vec<_>>(),
        "concurrency": {
            "max_synapses": state.config.concurrency.max_synapses,
            "max_operators": state.config.concurrency.max_operators,
        },
    }))
}

#[derive(Deserialize)]
struct MemorySearchQuery {
    q: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    memory_type: Option<String>,
    #[serde(default)]
    min_importance: Option<f64>,
    #[serde(default)]
    graph_depth: Option<u8>,
}

fn default_limit() -> usize {
    10
}

/// Search memories with hybrid vector + graph search.
#[utoipa::path(
    get,
    path = "/memories/search",
    params(
        ("q" = String, Query, description = "Search query"),
        ("limit" = Option<usize>, Query, description = "Max results"),
        ("memory_type" = Option<String>, Query, description = "Filter by memory type"),
        ("min_importance" = Option<f64>, Query, description = "Minimum importance score"),
        ("graph_depth" = Option<u8>, Query, description = "Graph traversal depth (0-2)")
    ),
    responses((status = 200, description = "Search results"))
)]
async fn memory_search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MemorySearchQuery>,
) -> Json<serde_json::Value> {
    use crate::memory::search::SearchParams;
    use crate::memory::types::MemoryType;

    let Some(ref memory) = state.memory else {
        return Json(json!({"error": "memory store not available"}));
    };

    let search_params = SearchParams {
        limit: params.limit,
        type_filter: params
            .memory_type
            .as_deref()
            .and_then(MemoryType::from_str),
        min_importance: params.min_importance.unwrap_or(0.0),
        graph_depth: params.graph_depth.unwrap_or(1),
    };

    match memory
        .hybrid_search(&params.q, &search_params, &state.db)
        .await
    {
        Ok(results) => Json(json!({"results": results})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
struct MemoryStoreRequest {
    content: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    memory_type: Option<String>,
}

/// Store a new memory with optional type classification.
#[utoipa::path(
    post,
    path = "/memories",
    responses((status = 200, description = "Memory stored"))
)]
async fn memory_store(
    State(state): State<Arc<AppState>>,
    Json(body): Json<MemoryStoreRequest>,
) -> Json<serde_json::Value> {
    use crate::memory::types::{MemoryType, classify_heuristic};

    let Some(ref memory) = state.memory else {
        return Json(json!({"error": "memory store not available"}));
    };

    let memory_type = body
        .memory_type
        .as_deref()
        .and_then(MemoryType::from_str)
        .unwrap_or_else(|| classify_heuristic(&body.content));

    let id = uuid::Uuid::new_v4().to_string();
    match memory
        .store(&id, &body.content, body.source.as_deref(), memory_type)
        .await
    {
        Ok(()) => {
            let _ = state
                .db
                .init_memory_metadata(&id, memory_type.as_str())
                .await;
            Json(json!({"id": id, "memory_type": memory_type.as_str(), "stored": true}))
        }
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
struct CreateEdgeRequest {
    source_id: String,
    target_id: String,
    relation_type: String,
    #[serde(default = "default_edge_weight")]
    weight: f64,
}

fn default_edge_weight() -> f64 {
    1.0
}

/// Create a typed edge between two memories.
#[utoipa::path(
    post,
    path = "/memories/edges",
    responses((status = 200, description = "Edge created"))
)]
async fn memory_create_edge(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateEdgeRequest>,
) -> Json<serde_json::Value> {
    use crate::memory::graph::EdgeType;

    let Some(edge_type) = EdgeType::from_str(&body.relation_type) else {
        return Json(json!({"error": format!("invalid relation_type '{}'", body.relation_type)}));
    };

    match state
        .db
        .create_edge(&body.source_id, &body.target_id, edge_type, body.weight)
        .await
    {
        Ok(()) => Json(json!({"created": true})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

/// Get all edges for a memory.
#[utoipa::path(
    get,
    path = "/memories/{id}/edges",
    params(("id" = String, description = "Memory ID")),
    responses((status = 200, description = "Edges for memory"))
)]
async fn memory_get_edges(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    match state.db.edges_for(&id).await {
        Ok(edges) => Json(json!({"edges": edges})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::config::types::Config;
    use crate::events::bus::EventBus;
    use crate::storage::database::Database;
    use crate::storage::paths::AuxlryPaths;

    async fn test_state() -> AppState {
        let dir = tempfile::tempdir().unwrap();
        let paths = AuxlryPaths::from_root(dir.path().to_path_buf()).unwrap();
        paths.ensure_dirs().unwrap();
        let db_path = paths.database.to_string_lossy().to_string();
        let db = Database::open(&db_path).await.unwrap();
        AppState::new(Config::default(), EventBus::new(), db, paths, None)
    }

    #[tokio::test]
    async fn health_check() {
        let state = test_state().await;
        let app = router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn dashboard_status_endpoint() {
        let state = test_state().await;
        let app = router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/dashboard/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn config_endpoint() {
        let state = test_state().await;
        let app = router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn recent_events_endpoint() {
        let state = test_state().await;
        let app = router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/events/recent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn memory_search_no_store() {
        let state = test_state().await;
        let app = router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/memories/search?q=test&limit=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }
}
