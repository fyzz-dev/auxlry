use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    Router,
    extract::{Query, State},
    http::{StatusCode, Uri, header},
    response::{
        IntoResponse, Json,
        sse::{Event as SseEvent, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures::stream::Stream;
use rust_embed::Embed;
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::core::state::AppState;

#[derive(Embed)]
#[folder = "dashboard/dist/"]
struct DashboardAssets;

#[derive(OpenApi)]
#[openapi(
    paths(health, events_stream, recent_events, dashboard_status, config_info, memory_search, memory_store, memory_create_edge, memory_get_edges),
    info(title = "auxlry API", version = "0.1.0")
)]
struct ApiDoc;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/events", get(events_stream))
        .route("/events/recent", get(recent_events))
        .route("/status", get(dashboard_status))
        .route("/memory-actions", get(dashboard_memory_actions))
        .route("/agent-spawns", get(dashboard_agent_spawns))
        .route("/message-heatmap", get(dashboard_message_heatmap))
        .route("/memory-categories", get(dashboard_memory_categories))
        .route("/config", get(config_info))
        .route("/memories/search", get(memory_search))
        .route("/memories", post(memory_store))
        .route("/memories/edges", post(memory_create_edge))
        .route("/memories/{id}/edges", get(memory_get_edges))
        .route("/memories/graph", get(memories_graph));

    Router::new()
        .nest("/api", api)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .fallback(static_handler)
        .with_state(Arc::new(state))
}

// ─── Static file serving (SPA fallback) ───

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    // Try exact file first
    if !path.is_empty() {
        if let Some(content) = DashboardAssets::get(path) {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref())],
                content.data.into_owned(),
            )
                .into_response();
        }
    }

    // SPA fallback: serve index.html
    match DashboardAssets::get("index.html") {
        Some(content) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html")],
            content.data.into_owned(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

// ─── Health ───

/// Health check endpoint.
#[utoipa::path(get, path = "/api/health", responses((status = 200, description = "OK")))]
async fn health() -> Json<serde_json::Value> {
    Json(json!({"status": "ok"}))
}

// ─── Events ───

/// SSE stream of real-time events.
#[utoipa::path(get, path = "/api/events", responses((status = 200, description = "SSE event stream")))]
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
#[utoipa::path(get, path = "/api/events/recent", responses((status = 200, description = "Recent events")))]
async fn recent_events(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    match state.db.recent_events(50).await {
        Ok(events) => Json(json!({"events": events})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

// ─── Dashboard data endpoints ───

/// Dashboard status — overview of the system.
#[utoipa::path(get, path = "/api/status", responses((status = 200, description = "System status")))]
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

async fn dashboard_memory_actions(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    match state.db.events_by_hour(&["memory_stored"], 48).await {
        Ok(rows) => {
            let data: Vec<_> = rows
                .iter()
                .map(|(hour, kind, count)| json!({"date": hour, "kind": kind, "count": count}))
                .collect();
            Json(json!({"data": data}))
        }
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn dashboard_agent_spawns(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    match state
        .db
        .events_by_hour(&["synapse_started", "operator_started"], 48)
        .await
    {
        Ok(rows) => {
            let data: Vec<_> = rows
                .iter()
                .map(|(hour, kind, count)| json!({"date": hour, "kind": kind, "count": count}))
                .collect();
            Json(json!({"data": data}))
        }
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn dashboard_message_heatmap(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    match state.db.messages_per_day_this_month().await {
        Ok(rows) => {
            let data: Vec<_> = rows
                .iter()
                .map(|(day, count)| json!({"date": day, "count": count}))
                .collect();
            Json(json!({"data": data}))
        }
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn dashboard_memory_categories(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    match state.db.memory_counts_by_type().await {
        Ok(rows) => {
            let data: Vec<_> = rows
                .iter()
                .map(|(typ, count)| json!({"type": typ, "count": count}))
                .collect();
            Json(json!({"data": data}))
        }
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn memories_graph(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let metadata = match state.db.all_memory_metadata().await {
        Ok(m) => m,
        Err(e) => return Json(json!({"error": e.to_string()})),
    };
    let edges = match state.db.all_edges().await {
        Ok(e) => e,
        Err(e) => return Json(json!({"error": e.to_string()})),
    };

    // Fetch content from vector store if available
    let ids: Vec<String> = metadata.iter().map(|m| m.id.clone()).collect();
    let mut content_map = std::collections::HashMap::new();
    if let Some(ref memory) = state.memory {
        if let Ok(results) = memory.fetch_by_ids(&ids).await {
            for r in results {
                content_map.insert(r.id.clone(), r.content.clone());
            }
        }
    }

    let nodes: Vec<_> = metadata
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "memory_type": m.memory_type,
                "access_count": m.access_count,
                "content": content_map.get(&m.id).cloned().unwrap_or_default(),
                "created_at": m.created_at,
            })
        })
        .collect();

    let links: Vec<_> = edges
        .iter()
        .map(|e| {
            json!({
                "source": e.source_id,
                "target": e.target_id,
                "relation_type": e.relation_type,
                "weight": e.weight,
            })
        })
        .collect();

    Json(json!({"nodes": nodes, "links": links}))
}

// ─── Config ───

/// Get current configuration (sanitized — no secrets).
#[utoipa::path(get, path = "/api/config", responses((status = 200, description = "Current config")))]
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

// ─── Memory endpoints ───

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
    path = "/api/memories/search",
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
    path = "/api/memories",
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
    path = "/api/memories/edges",
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
    path = "/api/memories/{id}/edges",
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
                    .uri("/api/health")
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
                    .uri("/api/status")
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
                    .uri("/api/config")
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
                    .uri("/api/events/recent")
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
                    .uri("/api/memories/search?q=test&limit=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn dashboard_memory_actions_endpoint() {
        let state = test_state().await;
        let app = router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/memory-actions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn dashboard_agent_spawns_endpoint() {
        let state = test_state().await;
        let app = router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/agent-spawns")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn dashboard_message_heatmap_endpoint() {
        let state = test_state().await;
        let app = router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/message-heatmap")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn dashboard_memory_categories_endpoint() {
        let state = test_state().await;
        let app = router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/memory-categories")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn memories_graph_endpoint() {
        let state = test_state().await;
        let app = router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/memories/graph")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }
}
