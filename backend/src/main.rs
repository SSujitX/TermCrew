use termcrew::session::{
    self, park_all_sessions, AgentSetupRequest, AppState, BroadcastRequest, LaunchRequest,
    RenameGroupRequest, RenameSessionRequest,
};
use termcrew::ws_handler::ws_handler;
use termcrew::{file_editor, persist, registry, skills, workdirs, worktree};

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

#[derive(Debug, Deserialize)]
struct AgentsQuery {
    fresh: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SkillsQuery {
    fresh: Option<bool>,
    workdir: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SkillPathQuery {
    path: String,
    workdir: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MarketplaceQuery {
    q: Option<String>,
    view: Option<String>,
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct HandoffRequest {
    source_session_id: String,
    target_session_id: String,
}

#[tokio::main]
async fn main() {
    // Initialize logging with tracing-subscriber
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "termcrew=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Initializing TermCrew backend...");

    // Sweep worktree dirs whose git registration is gone (no session store needed).
    let _ = tokio::task::spawn_blocking(worktree::sweep_stale_worktrees).await;

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

    // CORS wraps REST only. The PTY WebSocket is a raw upgrade — wrapping it
    // in CorsLayer interferes with 101 Switching Protocols.
    let api = Router::new()
        .route("/api/agents", get(handle_get_agents))
        .route("/api/agents/setup", post(handle_agent_setup))
        .route("/api/skills", get(handle_get_skills))
        .route("/api/skills/content", get(handle_skill_content))
        .route("/api/skills/prefs", post(handle_skill_prefs))
        .route("/api/skills/copy", post(handle_skill_copy))
        .route("/api/skills/delete", post(handle_skill_delete))
        .route("/api/skills/install", post(handle_skill_install))
        .route("/api/skills/open", post(handle_skill_open))
        .route("/api/skills/marketplace", get(handle_skills_marketplace))
        .route(
            "/api/skills/marketplace/install",
            post(handle_skills_marketplace_install),
        )
        .route("/api/workspace", get(handle_get_workspace))
        .route("/api/sessions", get(handle_get_sessions))
        .route("/api/sessions/launch", post(handle_launch_session))
        .route("/api/sessions/add", post(handle_add_to_group))
        .route("/api/sessions/:id/kill", post(handle_kill_session))
        .route("/api/sessions/:id/restart", post(handle_restart_session))
        .route("/api/sessions/:id/rename", post(handle_rename_session))
        .route("/api/sessions/handoff", post(handle_handoff_review))
        .route("/api/sessions/broadcast", post(handle_broadcast_input))
        .route("/api/sessions/rename", post(handle_rename_group))
        .route("/api/worktrees/diff", get(handle_get_diff))
        .route("/api/workdirs", get(handle_get_workdirs))
        .route("/api/fs/dirs", get(handle_browse_dirs))
        .route("/api/fs/list", get(handle_list_dir))
        .route("/api/fs/file", get(handle_read_file).put(handle_write_file))
        .route("/api/fs/create", post(handle_create_entry))
        .route("/api/fs/delete", post(handle_delete_entry))
        .route("/api/fs/pick-folder", post(handle_pick_folder))
        .route("/api/fs/open", post(handle_open_dir))
        .route("/api/storage", get(handle_get_storage))
        .route("/api/storage/open", post(handle_open_storage))
        .layer(cors);

    let app = Router::new()
        .route("/ws/:session_id", get(ws_handler))
        .merge(api)
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    info!("Server listening on http://{}", addr);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind TCP listener on {}: {}", addr, e);
            return;
        }
    };

    // Ctrl-C / SIGTERM must terminate agent process trees, not orphan them.
    // Explicit exit after cleanup: PTY children are not tied to this process,
    // so a plain graceful-return would leave every agent running.
    let shutdown_state = state.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        info!("Shutdown signal received; parking agent sessions...");
        park_all_sessions(&shutdown_state).await;
        std::process::exit(0);
    });

    if let Err(e) = axum::serve(listener, app).await {
        error!("Server encountered an error: {}", e);
    }
}

/// Resolves on the first shutdown signal (Ctrl-C, or SIGTERM on Unix).
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                error!("Failed to install SIGTERM handler: {}", e);
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

/// GET /api/agents - Lists detected agents and shells on the host
async fn handle_get_agents(Query(query): Query<AgentsQuery>) -> impl IntoResponse {
    let fresh = query.fresh.unwrap_or(false);
    let agents = tokio::task::spawn_blocking(move || {
        if fresh {
            registry::detect_agents_fresh()
        } else {
            registry::detect_agents()
        }
    })
    .await
    .unwrap_or_else(|_| Vec::new());
    Json(agents)
}

/// GET /api/workspace - Server process working directory (launch default when Folder is empty)
async fn handle_get_workspace() -> impl IntoResponse {
    match std::env::current_dir() {
        Ok(path) => (StatusCode::OK, Json(json!({ "path": path.to_string_lossy() }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Cannot resolve working directory: {e}") })),
        ),
    }
}

/// POST /api/agents/setup - Runs install / update / uninstall in a hidden shell
async fn handle_agent_setup(
    State(state): State<AppState>,
    Json(req): Json<AgentSetupRequest>,
) -> impl IntoResponse {
    match session::launch_agent_setup(&state, &req.agent_id, &req.action).await {
        Ok(session) => (StatusCode::OK, Json(json!({ "session": session }))),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))),
    }
}

async fn handle_get_skills(Query(query): Query<SkillsQuery>) -> impl IntoResponse {
    let fresh = query.fresh.unwrap_or(false);
    let workdir = query.workdir.clone();
    let catalog = tokio::task::spawn_blocking(move || {
        skills::list_skills(workdir.as_deref(), fresh)
    })
    .await
    .unwrap_or_else(|_| skills::SkillsCatalog {
        roots: vec![],
        skills: vec![],
        workdir: None,
    });
    Json(catalog)
}

async fn handle_skill_content(Query(query): Query<SkillPathQuery>) -> impl IntoResponse {
    let path = query.path.clone();
    let workdir = query.workdir.clone();
    match tokio::task::spawn_blocking(move || {
        skills::read_skill_content(&path, workdir.as_deref())
    })
    .await
    {
        Ok(Ok(content)) => (StatusCode::OK, Json(json!(content))).into_response(),
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}

async fn handle_skill_prefs(Json(req): Json<skills::PrefsUpdate>) -> impl IntoResponse {
    match tokio::task::spawn_blocking(move || skills::set_skill_enabled(&req.path, req.enabled))
        .await
    {
        Ok(Ok(prefs)) => (StatusCode::OK, Json(json!(prefs))).into_response(),
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}

async fn handle_skill_copy(
    Query(query): Query<SkillsQuery>,
    Json(req): Json<skills::CopyRequest>,
) -> impl IntoResponse {
    let workdir = query.workdir.clone();
    match tokio::task::spawn_blocking(move || skills::copy_skill(&req, workdir.as_deref())).await {
        Ok(Ok(skill)) => (StatusCode::OK, Json(json!({ "skill": skill }))).into_response(),
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}

async fn handle_skill_delete(
    Query(query): Query<SkillsQuery>,
    Json(req): Json<skills::DeleteRequest>,
) -> impl IntoResponse {
    let workdir = query.workdir.clone();
    match tokio::task::spawn_blocking(move || skills::delete_skill(&req, workdir.as_deref())).await
    {
        Ok(Ok(())) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}

async fn handle_skill_install(
    State(state): State<AppState>,
    Query(query): Query<SkillsQuery>,
    Json(req): Json<skills::InstallRequest>,
) -> impl IntoResponse {
    let workdir = query.workdir.clone();
    if req.source.trim().eq_ignore_ascii_case("git") {
        let command = match skills::git_install_command(&req, workdir.as_deref()) {
            Ok(c) => c,
            Err(e) => {
                return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response();
            }
        };
        let title = "Skills · Install · git";
        let shown = skills::git_install_display(&req.path_or_url);
        return match session::launch_command_setup(&state, title, &command, Some(&shown)).await {
            Ok(session) => (
                StatusCode::OK,
                Json(json!({ "session": session, "command": shown })),
            )
                .into_response(),
            Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
        };
    }
    match tokio::task::spawn_blocking(move || skills::install_skill(&req, workdir.as_deref())).await
    {
        Ok(Ok(skills_out)) => {
            (StatusCode::OK, Json(json!({ "skills": skills_out }))).into_response()
        }
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}

async fn handle_skill_open(
    Query(query): Query<SkillsQuery>,
    Json(req): Json<SkillPathQuery>,
) -> impl IntoResponse {
    let workdir = query.workdir.or(req.workdir.clone());
    let path = req.path.clone();
    match tokio::task::spawn_blocking(move || {
        skills::open_skill_folder(&path, workdir.as_deref())
    })
    .await
    {
        Ok(Ok(())) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}

async fn handle_skills_marketplace(Query(query): Query<MarketplaceQuery>) -> impl IntoResponse {
    let q = query.q.clone().unwrap_or_default();
    let view = query.view.clone().unwrap_or_default();
    let page = query.page.unwrap_or(0);
    let per_page = query.per_page.unwrap_or(9);
    match tokio::task::spawn_blocking(move || skills::search_marketplace(&q, &view, page, per_page))
        .await
    {
        Ok(Ok(result)) => (StatusCode::OK, Json(json!(result))).into_response(),
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}

async fn handle_skills_marketplace_install(
    State(state): State<AppState>,
    Json(req): Json<skills::MarketplaceInstallRequest>,
) -> impl IntoResponse {
    let command = match skills::marketplace_install_command(&req) {
        Ok(c) => c,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response();
        }
    };
    let title = format!("Skills · Install · {}", req.source);
    match session::launch_command_setup(&state, &title, &command, Some(&command)).await {
        Ok(session) => (StatusCode::OK, Json(json!({ "session": session, "command": command })))
            .into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
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

/// POST /api/sessions/add — one more pane in an existing group (other PTYs untouched).
async fn handle_add_to_group(
    State(state): State<AppState>,
    Json(req): Json<session::AddToGroupRequest>,
) -> impl IntoResponse {
    match session::add_to_group(&state, req).await {
        Ok(info) => (StatusCode::OK, Json(json!({ "session": info }))),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))),
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

/// POST /api/sessions/:id/restart - Kills and respawns a session with the same engine
async fn handle_restart_session(
    AxPath(id): AxPath<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match session::restart_session(&state, &id).await {
        Ok(info) => (
            StatusCode::OK,
            Json(json!({ "success": true, "session": info })),
        ),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

/// POST /api/sessions/handoff - Pipes source session's diff into target session
async fn handle_handoff_review(
    State(state): State<AppState>,
    Json(req): Json<HandoffRequest>,
) -> impl IntoResponse {
    match session::handoff_review(&state, &req.source_session_id, &req.target_session_id).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

/// POST /api/sessions/:id/rename - Renames one pane's display label
async fn handle_rename_session(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    Json(req): Json<RenameSessionRequest>,
) -> impl IntoResponse {
    match session::rename_session(&state, &id, &req.label).await {
        Ok(label) => (
            StatusCode::OK,
            Json(json!({ "success": true, "label": label })),
        ),
        Err(e) => {
            let code = if e.contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            (code, Json(json!({ "success": false, "error": e })))
        }
    }
}

/// POST /api/sessions/rename - Renames a session group
async fn handle_rename_group(
    State(state): State<AppState>,
    Json(req): Json<RenameGroupRequest>,
) -> impl IntoResponse {
    match session::rename_group(&state, &req.group_id, &req.label).await {
        Ok(label) => (
            StatusCode::OK,
            Json(json!({ "success": true, "label": label })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
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

#[derive(Debug, Deserialize)]
struct BrowseQuery {
    path: Option<String>,
}

/// GET /api/workdirs - Recently used launch folders (validated, newest first)
async fn handle_get_workdirs() -> impl IntoResponse {
    Json(workdirs::load_recents())
}

/// GET /api/fs/dirs?path=... - Subfolder listing for the launcher's folder browser.
/// Empty path returns the filesystem root (first drive letter on Windows).
async fn handle_browse_dirs(Query(q): Query<BrowseQuery>) -> impl IntoResponse {
    match workdirs::browse_dirs(q.path.as_deref()) {
        Ok(listing) => (StatusCode::OK, Json(json!(listing))),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))),
    }
}

/// GET /api/fs/list?path=... - Folders and files for the sidebar file tree.
/// Empty path falls back to the server working directory.
async fn handle_list_dir(Query(q): Query<BrowseQuery>) -> impl IntoResponse {
    match workdirs::list_dir_with_files(q.path.as_deref()) {
        Ok(listing) => (StatusCode::OK, Json(json!(listing))),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))),
    }
}

/// GET /api/fs/file?path=... - Reads a text file for the editor (binary and
/// oversized files are refused with an actionable error).
async fn handle_read_file(Query(q): Query<BrowseQuery>) -> impl IntoResponse {
    match q.path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        None => (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing 'path' query parameter" }))),
        Some(path) => match file_editor::read_file(path) {
            Ok(content) => (StatusCode::OK, Json(json!(content))),
            Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))),
        },
    }
}

/// PUT /api/fs/file - Writes editor content back. Conflicts (file changed on
/// disk since the client read it) are refused instead of clobbering.
async fn handle_write_file(Json(req): Json<file_editor::FileWrite>) -> impl IntoResponse {
    match file_editor::write_file(&req) {
        Ok(result) => (StatusCode::OK, Json(json!(result))),
        Err(e) => (StatusCode::CONFLICT, Json(json!({ "error": e }))),
    }
}

/// POST /api/fs/create — new file or folder under a session tree root.
/// `rel_path` may nest (`src/lib/util.ts`); `..` and paths outside `root` fail.
async fn handle_create_entry(Json(req): Json<file_editor::CreateEntry>) -> impl IntoResponse {
    match file_editor::create_entry(&req) {
        Ok(result) => (StatusCode::OK, Json(json!(result))),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))),
    }
}

/// POST /api/fs/delete — remove a file or folder inside a session tree root.
async fn handle_delete_entry(Json(req): Json<file_editor::DeleteEntry>) -> impl IntoResponse {
    match file_editor::delete_entry(&req) {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))),
    }
}

#[derive(Debug, Deserialize)]
struct PickFolderRequest {
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenDirRequest {
    path: String,
}

/// POST /api/fs/pick-folder - Opens a native Explorer/Finder window on the
/// host machine. Resolves `{ path }` on selection, `{ path: null }` on cancel.
async fn handle_pick_folder(Json(req): Json<PickFolderRequest>) -> impl IntoResponse {
    let title = req
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or("Select working folder");
    match workdirs::pick_folder(title).await {
        Ok(Some(path)) => (
            StatusCode::OK,
            Json(json!({ "path": path.to_string_lossy() })),
        ),
        Ok(None) => (StatusCode::OK, Json(json!({ "path": null }))),
        Err(e) => (StatusCode::CONFLICT, Json(json!({ "error": e }))),
    }
}

/// POST /api/fs/open — open a folder in Explorer / Finder / xdg-open.
async fn handle_open_dir(Json(req): Json<OpenDirRequest>) -> impl IntoResponse {
    let path = req.path;
    match tokio::task::spawn_blocking(move || persist::open_dir(&path)).await {
        Ok(Ok(())) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{e}") })),
        )
            .into_response(),
    }
}

/// GET /api/storage — paths where TermCrew keeps worktrees, sessions, scrollback.
async fn handle_get_storage() -> impl IntoResponse {
    Json(persist::storage_info())
}

/// POST /api/storage/open — open the data root in Explorer / Finder.
async fn handle_open_storage() -> impl IntoResponse {
    match persist::open_data_dir() {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))),
        Err(e) => (StatusCode::CONFLICT, Json(json!({ "error": e }))),
    }
}
