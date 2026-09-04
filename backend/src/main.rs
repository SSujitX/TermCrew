use backend::session::{self, AppState, BroadcastRequest, LaunchRequest};
use backend::ws_handler::ws_handler;
use backend::{registry, worktree};

use axum::{
    extract::{Path as AxPath, Query, State},
    http::{header, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use std::path::Path;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};


#[derive(Debug, Deserialize)]
struct DiffQuery {
    dir: Option<String>,
}

#[tokio::main]
async fn main() {
    // Initialize logging with tracing-subscriber
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "backend=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Initializing Multi-Agent Orchestrator Backend...");

    let state = session::create_app_state();

    // Configure CORS layer
    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:5173".parse().unwrap(),
            "http://127.0.0.1:5173".parse().unwrap(),
        ])
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
        ])
        .allow_credentials(false);

    // Build the Axum API router
    let app = Router::new()
        // REST API
        .route("/api/agents", get(handle_get_agents))
        .route("/api/sessions", get(handle_get_sessions))
        .route("/api/sessions/launch", post(handle_launch_session))
        .route("/api/sessions/:id/kill", post(handle_kill_session))
        .route("/api/sessions/broadcast", post(handle_broadcast_input))
        .route("/api/worktrees/diff", get(handle_get_diff))
        // WebSocket terminal endpoint
        .route("/ws/:session_id", get(ws_handler))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    info!("Server listening on http://{}", addr);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind TCP listener on {}: {}", addr, e);
            return;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        error!("Server encountered an error: {}", e);
    }
}

/// GET /api/agents - Lists detected agents and shells on the host
async fn handle_get_agents() -> impl IntoResponse {
    let agents = registry::detect_agents();
    Json(agents)
}

/// GET /api/sessions - Lists active session metadata
async fn handle_get_sessions(State(state): State<AppState>) -> impl IntoResponse {
    let sessions = session::list_sessions(&state).await;
    Json(sessions)
}

/// POST /api/sessions/launch - Spawns agents based on preset
async fn handle_launch_session(
    State(state): State<AppState>,
    Json(req): Json<LaunchRequest>,
) -> impl IntoResponse {
    match session::launch_preset(&state, req).await {
        Ok(sessions) => (StatusCode::OK, Json(json!({ "sessions": sessions }))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e })),
        ),
    }
}

/// POST /api/sessions/:id/kill - Terminates an active session
async fn handle_kill_session(
    AxPath(id): AxPath<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match session::kill_session(&state, &id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({ "success": true, "session_id": id })),
        ),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

/// POST /api/sessions/broadcast - Sends input text to active sessions
async fn handle_broadcast_input(
    State(state): State<AppState>,
    Json(req): Json<BroadcastRequest>,
) -> impl IntoResponse {
    match session::broadcast_input(&state, &req.input, req.session_ids).await {
        Ok(count) => (
            StatusCode::OK,
            Json(json!({ "success": true, "delivered_count": count })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

/// GET /api/worktrees/diff - Inspects git diff in directory
async fn handle_get_diff(Query(q): Query<DiffQuery>) -> impl IntoResponse {
    let dir = q.dir.unwrap_or_else(|| ".".to_string());
    match worktree::get_diff(Path::new(&dir)) {
        Ok(diff) => (
            StatusCode::OK,
            Json(json!({ "success": true, "diff": diff })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}
