use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::pty_manager::PtyManager;
use crate::registry;
use crate::worktree;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub preset: String,
    pub role: Option<String>,
    pub working_dir: String,
    pub worktree_path: Option<String>,
    pub created_at: String,
    pub is_alive: bool,
}

pub struct ActiveSession {
    pub info: SessionInfo,
    pub pty: Arc<PtyManager>,
}

pub type AppState = Arc<RwLock<HashMap<String, ActiveSession>>>;

pub fn create_app_state() -> AppState {
    Arc::new(RwLock::new(HashMap::new()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchRequest {
    pub preset: String, // "Solo", "Pair", "Workbench", "Swarm"
    pub engine: String, // e.g. "claude", "gemini", "opencode", "aider", "shell"
    pub base_dir: String,
    pub task: Option<String>,
    pub count: Option<usize>, // for Swarm preset (default 3)
    pub reviewer_engine: Option<String>, // for Pair preset
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastRequest {
    pub input: String,
    pub session_ids: Option<Vec<String>>,
}

/// Resolves the command binary and arguments to spawn for an engine name or agent ID.
fn resolve_engine_cmd(engine: &str) -> (String, Vec<String>) {
    let supported = registry::get_supported_agents();
    let found = supported.iter().find(|a| a.id.eq_ignore_ascii_case(engine));

    let binary = if let Some(agent) = found {
        agent.binary.clone()
    } else {
        engine.to_string()
    };

    let resolved_path = registry::resolve_binary_path(&binary).unwrap_or(binary);

    let args = if resolved_path.to_lowercase().ends_with("powershell.exe") {
        vec!["-NoLogo".to_string()]
    } else {
        vec![]
    };

    (resolved_path, args)
}

/// Spawns a single interactive session and registers it.
async fn spawn_single_session(
    name: &str,
    engine: &str,
    preset: &str,
    role: Option<&str>,
    cwd: &Path,
    worktree_path: Option<&Path>,
    initial_task: Option<&str>,
    rows: u16,
    cols: u16,
) -> Result<ActiveSession, String> {
    let session_id = Uuid::new_v4().to_string();
    let (program, args) = resolve_engine_cmd(engine);

    info!(
        session_id = %session_id,
        program = %program,
        cwd = ?cwd,
        preset = %preset,
        role = ?role,
        "Spawning PTY session"
    );

    let pty = PtyManager::spawn(&program, &args, Some(cwd), &[], rows, cols)
        .map_err(|e| format!("Failed to spawn session '{name}': {e}"))?;

    // If an initial task was provided, stream it into the PTY stdin shortly after spawn
    if let Some(task) = initial_task {
        let pty_clone = pty.clone();
        let mut task_input = task.to_string();
        if !task_input.ends_with('\n') {
            task_input.push('\n');
        }
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;
            let _ = pty_clone.write_input(task_input.as_bytes());
        });
    }

    let now_str = Utc::now().to_rfc3339();
    let info = SessionInfo {
        id: session_id,
        name: name.to_string(),
        engine: engine.to_string(),
        preset: preset.to_string(),
        role: role.map(|r| r.to_string()),
        working_dir: cwd.to_string_lossy().to_string(),
        worktree_path: worktree_path.map(|p| p.to_string_lossy().to_string()),
        created_at: now_str,
        is_alive: true,
    };

    Ok(ActiveSession { info, pty })
}

/// Launches a preset: Solo, Pair, Workbench, or Swarm.
pub async fn launch_preset(
    state: &AppState,
    req: LaunchRequest,
) -> Result<Vec<SessionInfo>, String> {
    let base_path = PathBuf::from(&req.base_dir);
    if !base_path.exists() {
        std::fs::create_dir_all(&base_path)
            .map_err(|e| format!("Failed to create base directory: {e}"))?;
    }

    let preset_norm = req.preset.trim().to_lowercase();
    let mut launched = Vec::new();

    match preset_norm.as_str() {
        "solo" => {
            let name = format!("Solo-{}", req.engine);
            let session = spawn_single_session(
                &name,
                &req.engine,
                "Solo",
                Some("Primary"),
                &base_path,
                None,
                req.task.as_deref(),
                24,
                80,
            )
            .await?;
            launched.push(session);
        }

        "pair" => {
            let builder_name = format!("Builder-{}", req.engine);
            let reviewer_engine = req
                .reviewer_engine
                .as_deref()
                .unwrap_or(req.engine.as_str());
            let reviewer_name = format!("Reviewer-{}", reviewer_engine);

            // Spawn Builder in main workspace
            let builder_session = spawn_single_session(
                &builder_name,
                &req.engine,
                "Pair",
                Some("Builder"),
                &base_path,
                None,
                req.task.as_deref(),
                24,
                80,
            )
            .await?;

            // Spawn Reviewer in a separate worktree for clean inspection
            let reviewer_id = Uuid::new_v4().to_string()[..8].to_string();
            let reviewer_wt = worktree::create_worktree(&base_path, &reviewer_id).ok();
            let reviewer_cwd = reviewer_wt.as_deref().unwrap_or(&base_path);

            let reviewer_task = format!(
                "Review code changes in this repository. Target task: {}",
                req.task.as_deref().unwrap_or("Autonomous code review")
            );

            let reviewer_session = spawn_single_session(
                &reviewer_name,
                reviewer_engine,
                "Pair",
                Some("Reviewer"),
                reviewer_cwd,
                reviewer_wt.as_deref(),
                Some(&reviewer_task),
                24,
                80,
            )
            .await?;

            launched.push(builder_session);
            launched.push(reviewer_session);
        }

        "workbench" => {
            let agent_name = format!("Agent-{}", req.engine);
            let shell_name = "System-Shell".to_string();

            let agent_session = spawn_single_session(
                &agent_name,
                &req.engine,
                "Workbench",
                Some("Agent"),
                &base_path,
                None,
                req.task.as_deref(),
                24,
                80,
            )
            .await?;

            let shell_session = spawn_single_session(
                &shell_name,
                "shell",
                "Workbench",
                Some("Shell"),
                &base_path,
                None,
                None,
                24,
                80,
            )
            .await?;

            launched.push(agent_session);
            launched.push(shell_session);
        }

        "swarm" => {
            let count = req.count.unwrap_or(3).clamp(1, 10);
            for i in 1..=count {
                let worker_id = format!("{}-w{}", &Uuid::new_v4().to_string()[..6], i);
                let wt = worktree::create_worktree(&base_path, &worker_id)
                    .map_err(|e| format!("Failed to create worktree for worker {i}: {e}"))?;

                let worker_name = format!("Worker-{}-{}", i, req.engine);
                let worker_role = format!("Worker-{}", i);
                let worker_task = req.task.as_ref().map(|t| format!("{t} [Worker {i}]"));

                let session = spawn_single_session(
                    &worker_name,
                    &req.engine,
                    "Swarm",
                    Some(&worker_role),
                    &wt,
                    Some(&wt),
                    worker_task.as_deref(),
                    24,
                    80,
                )
                .await?;

                launched.push(session);
            }
        }

        other => {
            return Err(format!(
                "Unknown preset: '{other}'. Supported presets: Solo, Pair, Workbench, Swarm"
            ));
        }
    }

    let mut result_infos = Vec::new();
    let mut map = state.write().await;

    for s in launched {
        result_infos.push(s.info.clone());
        map.insert(s.info.id.clone(), s);
    }

    Ok(result_infos)
}

/// Lists metadata of all active sessions, refreshing live status.
pub async fn list_sessions(state: &AppState) -> Vec<SessionInfo> {
    let mut map = state.write().await;
    let mut list = Vec::new();

    for (_, session) in map.iter_mut() {
        session.info.is_alive = session.pty.is_alive();
        list.push(session.info.clone());
    }

    list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    list
}

/// Terminates a session and cleans up its resources (including worktree if ephemeral).
pub async fn kill_session(state: &AppState, id: &str) -> Result<(), String> {
    let mut map = state.write().await;

    if let Some(session) = map.remove(id) {
        let _ = session.pty.kill();

        // If this session had an isolated worktree under .worktrees, clean it up
        if let Some(wt_str) = &session.info.worktree_path {
            let wt_path = PathBuf::from(wt_str);
            let base_dir = PathBuf::from(&session.info.working_dir);
            if let Some(folder_name) = wt_path.file_name().and_then(|n| n.to_str()) {
                if folder_name.starts_with("agent-") {
                    let agent_id = &folder_name["agent-".len()..];
                    let _ = worktree::remove_worktree(&base_dir, agent_id);
                }
            }
        }

        info!(session_id = %id, "Killed session successfully");
        Ok(())
    } else {
        Err(format!("Session '{id}' not found"))
    }
}

/// Broadcasts input to all or specific session IDs.
pub async fn broadcast_input(
    state: &AppState,
    input: &str,
    session_ids: Option<Vec<String>>,
) -> Result<usize, String> {
    let map = state.read().await;
    let mut count = 0;
    let data = input.as_bytes();

    let targets: Vec<String> = if let Some(ids) = session_ids {
        ids
    } else {
        map.keys().cloned().collect()
    };

    for id in targets {
        if let Some(session) = map.get(&id) {
            if session.pty.write_input(data).is_ok() {
                count += 1;
            }
        }
    }

    Ok(count)
}
