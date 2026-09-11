use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

use crate::persist;
use crate::pty_manager::PtyManager;
use crate::registry;
use crate::workdirs;
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
    /// Shared id for all TTYs spawned by one New Session / preset launch.
    pub group_id: String,
    /// Human label for the launch group, e.g. "Session 3 · Swarm · shell".
    pub group_label: String,
    /// Hidden from the sidebar — used for Agents install/update/uninstall consoles.
    #[serde(default)]
    pub hidden: bool,
    /// Launch task typed into the PTY (not the setup script). Used for compact review briefs.
    #[serde(default)]
    pub task: Option<String>,
}

pub struct ActiveSession {
    pub info: SessionInfo,
    /// `None` when the backend restarted and the session is parked (exited → relaunch).
    pub pty: Option<Arc<PtyManager>>,
}

pub type AppState = Arc<RwLock<HashMap<String, ActiveSession>>>;

/// Session ids currently inside `restart_session` — kill must not wipe their worktrees.
fn restarting_ids() -> &'static StdMutex<HashSet<String>> {
    static IDS: OnceLock<StdMutex<HashSet<String>>> = OnceLock::new();
    IDS.get_or_init(|| StdMutex::new(HashSet::new()))
}

struct RestartGuard(String);
impl Drop for RestartGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = restarting_ids().lock() {
            set.remove(&self.0);
        }
    }
}

pub fn create_app_state() -> AppState {
    let mut map = HashMap::new();
    for stored in persist::load_all_sessions() {
        let info = persist::to_session_info(stored);
        let id = info.id.clone();
        map.insert(id, ActiveSession { info, pty: None });
    }
    let n = map.len();
    if n > 0 {
        info!(count = n, "Restored parked sessions from disk");
    }
    let state = Arc::new(RwLock::new(map));
    start_scrollback_flusher(state.clone());
    state
}

fn start_scrollback_flusher(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            flush_live_scrollbacks(&state).await;
        }
    });
}

async fn flush_live_scrollbacks(state: &AppState) {
    let map = state.read().await;
    for session in map.values() {
        if session.info.hidden {
            continue;
        }
        if let Some(pty) = &session.pty {
            if pty.is_alive() {
                persist::save_scrollback(&session.info.id, &pty.get_history());
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchRequest {
    pub preset: String, // "Solo", "Pair", "Workbench", "Swarm"
    pub engine: String, // e.g. "claude", "gemini", "opencode", "aider", "shell"
    pub base_dir: String,
    pub task: Option<String>,
    pub count: Option<usize>, // for Swarm / repeated Solo when one engine is selected
    pub reviewer_engine: Option<String>, // for Pair preset (used if engines has one item)
    /// Engines to open in selection order. Empty / omitted falls back to `engine`.
    #[serde(default)]
    pub engines: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddToGroupRequest {
    pub group_id: String,
    pub engine: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastRequest {
    pub input: String,
    pub session_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSetupRequest {
    pub agent_id: String,
    pub action: String, // install | update | uninstall
}

/// Resolves the command binary and arguments to spawn for an engine name or agent ID.
/// Fails with an actionable error when the agent binary is not installed.
fn resolve_engine_cmd(engine: &str) -> Result<(String, Vec<String>), String> {
    let supported = registry::get_supported_agents();
    let found = supported.iter().find(|a| a.id.eq_ignore_ascii_case(engine));

    let (label, binary, mut args, agent_id) = match found {
        Some(agent) => (
            agent.name.clone(),
            agent.binary.clone(),
            agent.default_args.clone(),
            agent.id.clone(),
        ),
        None => (
            engine.to_string(),
            engine.to_string(),
            vec![],
            engine.to_string(),
        ),
    };

    let resolved_path = registry::resolve_agent_binary(&agent_id, &binary).ok_or_else(|| {
        format!(
            "Agent '{label}' is not installed: binary '{binary}' was not found in PATH. \
             Install it first or choose an installed agent."
        )
    })?;

    let lower = resolved_path.to_lowercase();
    if lower.ends_with("powershell.exe") || lower.ends_with("pwsh.exe") {
        args.insert(0, "-NoLogo".to_string());
    }

    Ok((resolved_path, args))
}

fn ps_single_quote(value: &str) -> String {
    value.replace('\'', "''")
}

/// Kill leftover agent processes and drop XDG/shim leftovers official uninstallers
/// often leave locked (Windows EBUSY / Unix busy directory).
fn uninstall_sweep_snippet(bin: &str) -> (String, String) {
    if cfg!(target_os = "windows") {
        let pre = format!(
            "Stop-Process -Name '{bin}' -Force -ErrorAction SilentlyContinue; "
        );
        let post = format!(
            "Stop-Process -Name '{bin}' -Force -ErrorAction SilentlyContinue; \
             Start-Sleep -Milliseconds 300; \
             foreach ($d in @( \
               (Join-Path $env:USERPROFILE ('.local\\share\\{bin}')), \
               (Join-Path $env:USERPROFILE ('.cache\\{bin}')), \
               (Join-Path $env:USERPROFILE ('.config\\{bin}')), \
               (Join-Path $env:USERPROFILE ('.local\\state\\{bin}')) \
             )) {{ \
               if (Test-Path -LiteralPath $d) {{ \
                 Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue; \
                 if (Test-Path -LiteralPath $d) {{ cmd.exe /c ('rd /s /q \"' + $d + '\"') }} \
               }} \
             }}; \
             foreach ($c in @( \
               (Join-Path $env:APPDATA ('npm\\{bin}.cmd')), \
               (Join-Path $env:APPDATA ('npm\\{bin}.exe')), \
               (Join-Path $env:APPDATA ('npm\\{bin}.ps1')), \
               (Join-Path $env:APPDATA ('npm\\{bin}')), \
               (Join-Path $env:USERPROFILE ('.bun\\bin\\{bin}.exe')), \
               (Join-Path $env:USERPROFILE ('.bun\\bin\\{bin}')), \
               (Join-Path $env:USERPROFILE ('.local\\bin\\{bin}.exe')), \
               (Join-Path $env:USERPROFILE ('.local\\bin\\{bin}.cmd')), \
               (Join-Path $env:USERPROFILE ('.local\\bin\\{bin}')) \
             )) {{ \
               if (Test-Path -LiteralPath $c) {{ Remove-Item -LiteralPath $c -Force -ErrorAction SilentlyContinue }} \
             }}; "
        );
        (pre, post)
    } else {
        let pre = format!("pkill -x '{bin}' 2>/dev/null || true; ");
        let post = format!(
            "pkill -x '{bin}' 2>/dev/null || true; \
             rm -rf \"$HOME/.local/share/{bin}\" \"$HOME/.cache/{bin}\" \"$HOME/.config/{bin}\" \"$HOME/.local/state/{bin}\"; \
             rm -f \"$HOME/.local/bin/{bin}\" \"$HOME/.bun/bin/{bin}\" \"$(npm bin -g 2>/dev/null)/{bin}\" \"$(npm root -g 2>/dev/null)/../{bin}\"; "
        );
        (pre, post)
    }
}

/// Status + official command + binary verify + sentinel. Same for install / update / remove.
fn setup_lifecycle_script(action: &str, agent_name: &str, command: &str, binary: &str) -> String {
    let (progress, done, fail) = match action {
        "install" => (
            "Installing",
            "Installed successfully.",
            "Install failed.",
        ),
        "update" => ("Updating", "Updated successfully.", "Update failed."),
        "uninstall" => ("Removing", "Removed successfully.", "Remove failed."),
        _ => ("Running", "Finished.", "Failed."),
    };
    let name = ps_single_quote(agent_name);
    let bin = ps_single_quote(binary);
    let shown = ps_single_quote(command);
    let want_present = action != "uninstall";
    let (pre_un, post_un) = if action == "uninstall" {
        uninstall_sweep_snippet(&bin)
    } else {
        (String::new(), String::new())
    };

    if cfg!(target_os = "windows") {
        // Refresh PATH from Machine+User (npm/bun/etc. update User PATH; this
        // -NoProfile shell still has the pre-install snapshot). Also probe the
        // same well-known install dirs the registry uses — Get-Command alone
        // misses `%APPDATA%\npm\pi.cmd` after a fresh `npm i -g`.
        format!(
            "$ProgressPreference = 'SilentlyContinue'; \
             $env:Path = [Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User'); \
             function Test-AgentPresent([string]$bin) {{ \
               if (Get-Command $bin -ErrorAction SilentlyContinue) {{ return $true }}; \
               $cands = @( \
                 (Join-Path $env:APPDATA ('npm\\' + $bin + '.cmd')), \
                 (Join-Path $env:APPDATA ('npm\\' + $bin + '.exe')), \
                 (Join-Path $env:APPDATA ('npm\\' + $bin + '.ps1')), \
                 (Join-Path $env:APPDATA ('npm\\' + $bin)), \
                 (Join-Path $env:USERPROFILE ('.bun\\bin\\' + $bin + '.exe')), \
                 (Join-Path $env:USERPROFILE ('.bun\\bin\\' + $bin)), \
                 (Join-Path $env:USERPROFILE ('.amp\\bin\\' + $bin + '.exe')), \
                 (Join-Path $env:USERPROFILE ('.amp\\bin\\' + $bin)), \
                 (Join-Path $env:USERPROFILE ('.forge\\bin\\' + $bin + '.exe')), \
                 (Join-Path $env:USERPROFILE ('.forge\\bin\\' + $bin)), \
                 (Join-Path $env:LOCALAPPDATA ($bin + '\\' + $bin + '.exe')), \
                 (Join-Path $env:LOCALAPPDATA ($bin + '\\bin\\' + $bin + '.exe')), \
                 (Join-Path $env:LOCALAPPDATA ($bin + '\\' + $bin + '.cmd')), \
                 (Join-Path $env:LOCALAPPDATA ('Programs\\OpenAI\\Codex\\bin\\' + $bin + '.exe')), \
                 (Join-Path $env:LOCALAPPDATA ('Programs\\CodeWhale\\bin\\' + $bin + '.exe')), \
                 (Join-Path $env:LOCALAPPDATA ('Programs\\Forge\\' + $bin + '.exe')), \
                 (Join-Path $env:LOCALAPPDATA ('Microsoft\\WinGet\\Links\\' + $bin + '.exe')), \
                 (Join-Path $env:LOCALAPPDATA ('Microsoft\\WinGet\\Links\\' + $bin + '.cmd')), \
                 (Join-Path $env:USERPROFILE ('.local\\bin\\' + $bin + '.exe')), \
                 (Join-Path $env:USERPROFILE ('.local\\bin\\' + $bin + '.cmd')), \
                 (Join-Path $env:USERPROFILE ('.local\\bin\\' + $bin)), \
                 (Join-Path $env:USERPROFILE ('go\\bin\\' + $bin + '.exe')), \
                 (Join-Path $env:USERPROFILE ('scoop\\shims\\' + $bin + '.exe')), \
                 (Join-Path $env:USERPROFILE ('bin\\' + $bin + '.exe')), \
                 (Join-Path $env:USERPROFILE ('bin\\' + $bin + '.bat')) \
               ); \
               foreach ($c in $cands) {{ if (Test-Path -LiteralPath $c) {{ return $true }} }}; \
               $wg = Join-Path $env:LOCALAPPDATA 'Microsoft\\WinGet\\Packages'; \
               if (Test-Path -LiteralPath $wg) {{ \
                 $hit = Get-ChildItem -LiteralPath $wg -Recurse -Filter ($bin + '.exe') -ErrorAction SilentlyContinue | Select-Object -First 1; \
                 if ($hit) {{ return $true }} \
               }}; \
               return $false \
             }}; \
             Write-Host ''; Write-Host '{progress} {name}...'; Write-Host ''; \
             Write-Host '{shown}'; Write-Host ''; \
             {pre_un}{command}; \
             {post_un}\
             $env:Path = [Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User'); \
             $present = Test-AgentPresent '{bin}'; \
             $ok = $(if ({want}) {{ $present }} else {{ -not $present }}); \
             Write-Host ''; \
             if ($ok) {{ Write-Host '{done}' }} else {{ Write-Host '{fail}' }}; \
             if ($ok) {{ Write-Host '{sent_ok}' }} else {{ Write-Host '{sent_fail}' }}",
            want = if want_present { "$true" } else { "$false" },
            sent_ok = crate::pty_wire::SETUP_SENTINEL_OK,
            sent_fail = crate::pty_wire::SETUP_SENTINEL_FAIL,
        )
    } else {
        let check = if want_present {
            format!(
                "command -v {bin} >/dev/null 2>&1 \
                 || test -x \"$HOME/.local/bin/{bin}\" \
                 || test -x \"$HOME/.bun/bin/{bin}\" \
                 || test -x \"$(npm root -g 2>/dev/null)/../{bin}\" \
                 || test -x \"$(npm bin -g 2>/dev/null)/{bin}\""
            )
        } else {
            format!(
                "! command -v {bin} >/dev/null 2>&1 \
                 && ! test -x \"$HOME/.local/bin/{bin}\" \
                 && ! test -x \"$HOME/.bun/bin/{bin}\" \
                 && ! test -x \"$(npm root -g 2>/dev/null)/../{bin}\" \
                 && ! test -x \"$(npm bin -g 2>/dev/null)/{bin}\""
            )
        };
        format!(
            "export PATH=\"$HOME/.local/bin:$HOME/.bun/bin:$(npm bin -g 2>/dev/null):$PATH\"; \
             printf '\\n{progress} {name}...\\n\\n'; printf '%s\\n\\n' '{shown}'; {pre_un}{command}; {post_un}printf '\\n'; \
             export PATH=\"$HOME/.local/bin:$HOME/.bun/bin:$(npm bin -g 2>/dev/null):$PATH\"; \
             if {check}; then printf '{done}\\n{sent_ok}\\n'; else printf '{fail}\\n{sent_fail}\\n'; fi",
            sent_ok = crate::pty_wire::SETUP_SENTINEL_OK,
            sent_fail = crate::pty_wire::SETUP_SENTINEL_FAIL,
        )
    }
}

/// Interactive setup shell. Runs the lifecycle script, then stays open.
///
/// On Windows the script is written to a temp `.ps1` and launched with `-File`.
/// Putting the whole install wrapper on `powershell -Command …` trips Defender
/// ML (e.g. Trojan:Win32/Commando.A!ml) because it looks like living-off-the-land
/// malware: -NoProfile, long inline script, PATH probing, remote iex.
fn resolve_setup_shell_cmd(script: &str) -> Result<(String, Vec<String>), String> {
    if cfg!(target_os = "windows") {
        let path = registry::resolve_binary_path("powershell.exe").ok_or_else(|| {
            "powershell.exe was not found. Cannot open the setup console.".to_string()
        })?;
        let file = std::env::temp_dir().join(format!("termcrew-setup-{}.ps1", Uuid::new_v4()));
        // BOM so -File parses as UTF-8 on localized Windows.
        let mut body = String::from('\u{FEFF}');
        body.push_str(script);
        if !body.ends_with('\n') {
            body.push('\n');
        }
        std::fs::write(&file, body.as_bytes())
            .map_err(|e| format!("Failed to write setup script {}: {e}", file.display()))?;
        Ok((
            path,
            vec![
                "-NoLogo".to_string(),
                "-NoProfile".to_string(),
                "-NoExit".to_string(),
                "-ExecutionPolicy".to_string(),
                "Bypass".to_string(),
                "-File".to_string(),
                file.to_string_lossy().into_owned(),
            ],
        ))
    } else {
        let preferred = if cfg!(target_os = "macos") { "zsh" } else { "bash" };
        let (path, exec) = registry::resolve_binary_path(preferred)
            .map(|p| (p, preferred.to_string()))
            .or_else(|| {
                registry::resolve_binary_path("bash").map(|p| (p, "bash".to_string()))
            })
            .ok_or_else(|| {
                "No zsh or bash found. Cannot open the setup console.".to_string()
            })?;
        Ok((
            path,
            vec!["-c".to_string(), format!("{script}; exec {exec}")],
        ))
    }
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
    group_id: &str,
    group_label: &str,
    rows: u16,
    cols: u16,
    reuse_id: Option<&str>,
    reuse_created_at: Option<&str>,
    seed_history: Option<Vec<u8>>,
) -> Result<ActiveSession, String> {
    let session_id = reuse_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let (program, args) = if preset.eq_ignore_ascii_case("Setup") {
        resolve_setup_shell_cmd(
            initial_task.unwrap_or("$ProgressPreference = 'SilentlyContinue'"),
        )?
    } else {
        resolve_engine_cmd(engine)?
    };

    info!(
        session_id = %session_id,
        group_id = %group_id,
        program = %program,
        cwd = ?cwd,
        preset = %preset,
        role = ?role,
        "Spawning PTY session"
    );

    let env = registry::pty_terminal_env();
    let pty = PtyManager::spawn(
        &program,
        &args,
        Some(cwd),
        &env,
        rows,
        cols,
        seed_history,
    )
    .map_err(|e| format!("Failed to spawn session '{name}': {e}"))?;

    // Confirm a first-run theme picker only when it is actually on screen,
    // then type the launch task after the chat/shell prompt is ready.
    let confirm_theme = registry::needs_theme_auto_confirm(engine);
    let task_owned = initial_task
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|t| format!("{t}\r"));

    if !preset.eq_ignore_ascii_case("Setup") && (confirm_theme || task_owned.is_some()) {
        let pty_clone = pty.clone();
        tokio::spawn(async move {
            wait_until_output_quiet(&pty_clone, TASK_WAIT).await;
            if confirm_theme && looks_like_theme_picker(&history_tail(&pty_clone)) {
                let _ = pty_clone.write_input(b"\r");
                wait_until_output_quiet(&pty_clone, TASK_THEME_SETTLE).await;
            }
            if let Some(task_input) = task_owned {
                wait_until_chat_ready(&pty_clone, TASK_PROMPT_WAIT).await;
                if let Err(e) = pty_clone.write_input(task_input.as_bytes()) {
                    warn!(error = %e, "Failed to type launch task");
                } else {
                    info!(bytes = task_input.len(), "Typed launch task into PTY");
                }
            }
        });
    }

    let now_str = reuse_created_at
        .map(|s| s.to_string())
        .unwrap_or_else(|| Utc::now().to_rfc3339());
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
        group_id: group_id.to_string(),
        group_label: group_label.to_string(),
        hidden: false,
        task: if preset.eq_ignore_ascii_case("Setup") {
            None
        } else {
            initial_task
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        },
    };

    Ok(ActiveSession {
        info,
        pty: Some(pty),
    })
}

fn next_session_ordinal(state_map: &HashMap<String, ActiveSession>) -> usize {
    let mut seen = std::collections::HashSet::new();
    for s in state_map.values() {
        if s.info.hidden {
            continue;
        }
        seen.insert(s.info.group_id.clone());
    }
    seen.len() + 1
}

fn format_group_label(ordinal: usize, preset: &str, _engine: &str) -> String {
    format!("{preset} {ordinal}")
}

fn engines_in_order(req: &LaunchRequest) -> Vec<String> {
    let listed: Vec<String> = req
        .engines
        .as_ref()
        .map(|list| {
            list.iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if listed.is_empty() {
        vec![req.engine.clone()]
    } else {
        listed
    }
}

fn resolve_launch_dir(base_dir: &str) -> Result<PathBuf, String> {
    let trimmed = base_dir.trim();
    if trimmed.is_empty() {
        return Err("Choose a folder before starting a session".into());
    }
    let base_path = PathBuf::from(trimmed);
    if !base_path.exists() {
        return Err(format!(
            "Working directory does not exist: {}",
            base_path.display()
        ));
    }
    if !base_path.is_dir() {
        return Err(format!(
            "Working directory is not a folder: {}",
            base_path.display()
        ));
    }
    Ok(base_path)
}

/// Launches a preset: Solo, Pair, Workbench, or Swarm.
pub async fn launch_preset(
    state: &AppState,
    req: LaunchRequest,
) -> Result<Vec<SessionInfo>, String> {
    let base_path = resolve_launch_dir(&req.base_dir)?;

    let preset_norm = req.preset.trim().to_lowercase();
    let engines = engines_in_order(&req);
    let primary = engines.first().cloned().unwrap_or_else(|| req.engine.clone());
    let group_id = Uuid::new_v4().to_string();
    let ordinal = {
        let map = state.read().await;
        next_session_ordinal(&map)
    };
    let mut launched = Vec::new();

    match preset_norm.as_str() {
        "solo" => {
            let group_label = format_group_label(ordinal, "Solo", &primary);
            let roster: Vec<String> = if engines.len() > 1 {
                engines
            } else {
                let copies = req.count.unwrap_or(1).clamp(1, 10);
                vec![primary.clone(); copies]
            };
            for (i, engine) in roster.iter().enumerate() {
                let role = if roster.len() == 1 {
                    "Lead".to_string()
                } else {
                    format!("Lead {}", i + 1)
                };
                let name = format!("{role} · {engine}");
                let session = spawn_single_session(
                    &name,
                    engine,
                    "Solo",
                    Some(&role),
                    &base_path,
                    None,
                    req.task.as_deref(),
                    &group_id,
                    &group_label,
                    24,
                    80,
                    None,
                    None,
                    None,
                )
                .await?;
                launched.push(session);
            }
        }

        "pair" => {
            let group_label = format_group_label(ordinal, "Pair", &primary);
            let lead = primary.clone();
            let reviewers: Vec<String> = if engines.len() >= 2 {
                engines[1..].to_vec()
            } else {
                vec![req
                    .reviewer_engine
                    .clone()
                    .unwrap_or_else(|| lead.clone())]
            };

            let lead_session = spawn_single_session(
                &format!("Lead · {lead}"),
                &lead,
                "Pair",
                Some("Lead"),
                &base_path,
                None,
                req.task.as_deref(),
                &group_id,
                &group_label,
                24,
                80,
                None,
                None,
                None,
            )
            .await?;
            launched.push(lead_session);

            for (i, reviewer_engine) in reviewers.iter().enumerate() {
                let role = format!("Review {}", i + 1);
                let reviewer_task = format!(
                    "Review code changes in this repository. Target task: {}",
                    req.task.as_deref().unwrap_or("Autonomous code review")
                );
                let reviewer_session = spawn_single_session(
                    &format!("{role} · {reviewer_engine}"),
                    reviewer_engine,
                    "Pair",
                    Some(&role),
                    &base_path,
                    None,
                    Some(&reviewer_task),
                    &group_id,
                    &group_label,
                    24,
                    80,
                    None,
                    None,
                    None,
                )
                .await
                .map_err(|e| {
                    abandon_launched(std::mem::take(&mut launched));
                    e
                })?;
                launched.push(reviewer_session);
            }
        }

        "workbench" => {
            let group_label = format_group_label(ordinal, "Workbench", &primary);
            for (i, engine) in engines.iter().enumerate() {
                let role = if engines.len() == 1 {
                    "Agent".to_string()
                } else {
                    format!("Agent {}", i + 1)
                };
                let agent_session = spawn_single_session(
                    &format!("{role} · {engine}"),
                    engine,
                    "Workbench",
                    Some(&role),
                    &base_path,
                    None,
                    req.task.as_deref(),
                    &group_id,
                    &group_label,
                    24,
                    80,
                    None,
                    None,
                    None,
                )
                .await?;
                launched.push(agent_session);
            }

            let shell_session = spawn_single_session(
                "Shell",
                "shell",
                "Workbench",
                Some("Shell"),
                &base_path,
                None,
                None,
                &group_id,
                &group_label,
                24,
                80,
                None,
                None,
                None,
            )
            .await?;
            launched.push(shell_session);
        }

        "swarm" => {
            let roster: Vec<String> = if engines.len() > 1 {
                engines
            } else {
                let count = req.count.unwrap_or(3).clamp(1, 10);
                vec![primary.clone(); count]
            };
            let group_label = format_group_label(ordinal, "Swarm", &primary);
            for (i, engine) in roster.iter().enumerate() {
                let n = i + 1;
                let worker_id = format!("{}-w{}", &Uuid::new_v4().to_string()[..6], n);
                let wt = match worktree::create_worktree(&base_path, &worker_id) {
                    Ok(wt) => wt,
                    Err(e) => {
                        abandon_launched(std::mem::take(&mut launched));
                        return Err(format!("Failed to create worktree for worker {n}: {e}"));
                    }
                };

                let worker_name = format!("Worker {n} · {engine}");
                let worker_role = format!("Worker {n}");
                let worker_task = req.task.as_ref().map(|t| format!("{t} [Worker {n}]"));

                let session = match spawn_single_session(
                    &worker_name,
                    engine,
                    "Swarm",
                    Some(&worker_role),
                    &wt,
                    Some(&wt),
                    worker_task.as_deref(),
                    &group_id,
                    &group_label,
                    24,
                    80,
                    None,
                    None,
                    None,
                )
                .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = worktree::remove_worktree(&base_path, &worker_id);
                        abandon_launched(std::mem::take(&mut launched));
                        return Err(e);
                    }
                };

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
        persist::persist_info(&s.info);
        result_infos.push(s.info.clone());
        map.insert(s.info.id.clone(), s);
    }

    // Remember the folder for the launcher's recents — after all sessions
    // spawned successfully, so failed launches don't pollute the list.
    workdirs::record_workdir(&base_path);

    Ok(result_infos)
}

const MAX_GROUP_PANES: usize = 6;

fn is_shell_engine(engine: &str) -> bool {
    matches!(
        engine,
        "shell" | "cmd" | "git-bash" | "wsl" | "bash" | "zsh" | "sh"
    )
}

fn max_role_n(kind: &str, roles: &[String]) -> u32 {
    let prefix = kind.to_lowercase();
    let mut max = 0u32;
    for r in roles {
        let r = r.trim().to_lowercase();
        if r == prefix {
            max = max.max(1);
            continue;
        }
        if let Some(rest) = r.strip_prefix(&format!("{prefix} ")) {
            if let Ok(n) = rest.parse::<u32>() {
                max = max.max(n);
            }
        }
    }
    max
}

/// Next role for a pane added to an existing group. `true` = new Swarm worktree.
pub(crate) fn next_add_role(preset: &str, engine: &str, roles: &[String]) -> (String, bool) {
    if is_shell_engine(engine) {
        let n = max_role_n("Shell", roles) + 1;
        let role = if n == 1 {
            "Shell".into()
        } else {
            format!("Shell {n}")
        };
        return (role, false);
    }
    match preset.to_lowercase().as_str() {
        "pair" => (format!("Review {}", max_role_n("Review", roles) + 1), false),
        "solo" => {
            let n = max_role_n("Lead", roles) + 1;
            (
                if n == 1 {
                    "Lead".into()
                } else {
                    format!("Lead {n}")
                },
                false,
            )
        }
        "workbench" => {
            let n = max_role_n("Agent", roles) + 1;
            (
                if n == 1 {
                    "Agent".into()
                } else {
                    format!("Agent {n}")
                },
                false,
            )
        }
        "swarm" => (format!("Worker {}", max_role_n("Worker", roles) + 1), true),
        _ => ("Lead".into(), false),
    }
}

fn swarm_repo(members: &[SessionInfo]) -> PathBuf {
    for m in members {
        let p = m
            .worktree_path
            .as_deref()
            .unwrap_or(m.working_dir.as_str());
        if let Some(repo) = worktree::repo_for_worktree(Path::new(p)) {
            return repo;
        }
    }
    PathBuf::from(&members[0].working_dir)
}

/// Spawns one more pane in an existing group. Other PTYs are not touched.
pub async fn add_to_group(
    state: &AppState,
    req: AddToGroupRequest,
) -> Result<SessionInfo, String> {
    let engine = req.engine.trim();
    if engine.is_empty() {
        return Err("Choose an agent".into());
    }
    resolve_engine_cmd(engine)?;

    let members: Vec<SessionInfo> = {
        let map = state.read().await;
        let mut v: Vec<SessionInfo> = map
            .values()
            .filter(|s| !s.info.hidden && s.info.group_id == req.group_id)
            .map(|s| s.info.clone())
            .collect();
        v.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        v
    };
    if members.is_empty() {
        return Err("Session group not found".into());
    }
    if members.len() >= MAX_GROUP_PANES {
        return Err("This session is full (6 panes)".into());
    }
    let preset = members[0].preset.clone();
    if preset.eq_ignore_ascii_case("Setup") {
        return Err("Cannot add a pane to a setup console".into());
    }

    let roles: Vec<String> = members.iter().filter_map(|m| m.role.clone()).collect();
    let (role, isolate) = next_add_role(&preset, engine, &roles);

    let mut swarm_wt: Option<(PathBuf, String)> = None;
    let (cwd, wt) = if isolate {
        let base = swarm_repo(&members);
        let n = max_role_n("Worker", &roles) + 1;
        let worker_id = format!("{}-w{n}", &Uuid::new_v4().to_string()[..6]);
        let path = worktree::create_worktree(&base, &worker_id)?;
        swarm_wt = Some((base, worker_id));
        (path.clone(), Some(path))
    } else {
        (PathBuf::from(&members[0].working_dir), None)
    };

    let name = format!("{role} · {engine}");
    let session = match spawn_single_session(
        &name,
        engine,
        &preset,
        Some(&role),
        &cwd,
        wt.as_deref(),
        None,
        &members[0].group_id,
        &members[0].group_label,
        24,
        80,
        None,
        None,
        None,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            if let Some((base, worker_id)) = swarm_wt {
                let _ = worktree::remove_worktree(&base, &worker_id);
            }
            return Err(e);
        }
    };

    let info = session.info.clone();
    {
        let mut map = state.write().await;
        persist::persist_info(&info);
        map.insert(info.id.clone(), session);
    }
    info!(
        session_id = %info.id,
        group_id = %req.group_id,
        engine = %engine,
        role = %role,
        "Added pane to existing group"
    );
    Ok(info)
}

/// Lists metadata of all sessions (live + parked), refreshing live status.
pub async fn list_sessions(state: &AppState) -> Vec<SessionInfo> {
    let mut map = state.write().await;
    let mut list = Vec::new();

    for (_, session) in map.iter_mut() {
        if session.info.hidden {
            continue;
        }
        let was_alive = session.info.is_alive;
        session.info.is_alive = session
            .pty
            .as_ref()
            .map(|p| p.is_alive())
            .unwrap_or(false);
        if was_alive && !session.info.is_alive {
            if let Some(pty) = &session.pty {
                persist::save_scrollback(&session.info.id, &pty.get_history());
            }
            persist::persist_info(&session.info);
        }
        list.push(session.info.clone());
    }

    list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    list
}

/// Restarts or relaunches a session, keeping the same id (and scrollback seed).
/// Keeps the old PTY alive until the new one spawns successfully.
pub async fn restart_session(state: &AppState, id: &str) -> Result<SessionInfo, String> {
    {
        let mut set = restarting_ids()
            .lock()
            .map_err(|_| "Restart lock poisoned".to_string())?;
        if !set.insert(id.to_string()) {
            return Err("Session is already relaunching".to_string());
        }
    }
    let _guard = RestartGuard(id.to_string());

    let old_info = {
        let map = state.read().await;
        map.get(id)
            .map(|s| s.info.clone())
            .ok_or_else(|| format!("Session '{id}' not found"))?
    };

    // Snapshot history without killing the live PTY yet.
    let seed = {
        let map = state.read().await;
        let mut data = if let Some(session) = map.get(id) {
            if let Some(pty) = &session.pty {
                let hist = pty.get_history();
                persist::save_scrollback(id, &hist);
                hist
            } else {
                persist::load_scrollback(id)
            }
        } else {
            persist::load_scrollback(id)
        };
        if data.is_empty() {
            None
        } else {
            data.extend_from_slice(b"\r\n\r\n--- relaunched ---\r\n\r\n");
            Some(data)
        }
    };

    let cwd = old_info
        .worktree_path
        .clone()
        .unwrap_or_else(|| old_info.working_dir.clone());

    let mut session = spawn_single_session(
        &old_info.name,
        &old_info.engine,
        &old_info.preset,
        old_info.role.as_deref(),
        Path::new(&cwd),
        old_info.worktree_path.as_deref().map(Path::new),
        None,
        &old_info.group_id,
        &old_info.group_label,
        24,
        80,
        Some(id),
        Some(&old_info.created_at),
        seed,
    )
    .await?;

    // Spawn succeeded — swap PTYs under the write lock. Keep the stored
    // launch task (restart does not re-type it into the PTY).
    let mut info = session.info.clone();
    info.task = old_info.task.clone();
    {
        let mut map = state.write().await;
        if let Some(old) = map.remove(id) {
            if let Some(pty) = old.pty {
                let _ = pty.kill();
            }
        }
        persist::persist_info(&info);
        session.info.task = info.task.clone();
        map.insert(info.id.clone(), session);
    }

    info!(session_id = %id, "Restarted session (id preserved)");
    Ok(info)
}

const TASK_WAIT: std::time::Duration = std::time::Duration::from_secs(20);
const TASK_THEME_SETTLE: std::time::Duration = std::time::Duration::from_secs(3);
const TASK_PROMPT_WAIT: std::time::Duration = std::time::Duration::from_secs(10);
const TASK_BOOT_MIN: std::time::Duration = std::time::Duration::from_millis(1500);
const TASK_QUIET: std::time::Duration = std::time::Duration::from_millis(700);

fn strip_ansi(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for x in chars.by_ref() {
                    if x.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        if c != '\u{0}' {
            out.push(c);
        }
    }
    out
}

fn history_tail(pty: &PtyManager) -> String {
    let h = pty.get_history();
    let start = h.len().saturating_sub(4096);
    strip_ansi(&h[start..])
}

fn looks_like_theme_picker(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    let menu = t.contains("theme") || t.contains("color scheme") || t.contains("select a style");
    menu && (t.contains("auto") || t.contains("1.") || t.contains("1)"))
}

fn looks_like_chat_ready(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    if t.contains("? for shortcuts")
        || t.contains("enter a prompt")
        || t.contains("type a message")
        || t.contains("type your")
    {
        return true;
    }
    let last = t
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    last == ">" || last == "❯" || last == ">>>" || last.ends_with('>') || last.ends_with('❯')
}

/// Wait until the CLI has printed and gone quiet after a minimum boot time.
async fn wait_until_output_quiet(pty: &PtyManager, timeout: std::time::Duration) {
    let start = tokio::time::Instant::now();
    let deadline = start + timeout;
    let mut last = 0usize;
    let mut saw = false;
    let mut quiet_at: Option<tokio::time::Instant> = None;
    while tokio::time::Instant::now() < deadline {
        if !pty.is_alive() {
            return;
        }
        let n = pty.get_history().len();
        if n > last {
            saw = true;
            last = n;
            quiet_at = None;
        } else if saw && start.elapsed() >= TASK_BOOT_MIN {
            let started = quiet_at.get_or_insert_with(tokio::time::Instant::now);
            if started.elapsed() >= TASK_QUIET {
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    }
}

async fn wait_until_chat_ready(pty: &PtyManager, timeout: std::time::Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if !pty.is_alive() || looks_like_chat_ready(&history_tail(pty)) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// Token budget for the review packet typed into the target PTY.
const MAX_REVIEW_BYTES: usize = 6 * 1024;
const MAX_TASK_CHARS: usize = 200;

fn clip_chars(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

/// Compact review packet: role, folder, optional task, git — no chat transcript.
fn compact_review_prompt(
    role: &str,
    engine: &str,
    dir: &str,
    task: Option<&str>,
    git: &str,
) -> String {
    let mut body = format!("REVIEW {role} {engine}\n{dir}\n");
    if let Some(task) = task.map(str::trim).filter(|s| !s.is_empty()) {
        body.push_str("Task: ");
        body.push_str(&clip_chars(task, MAX_TASK_CHARS));
        body.push('\n');
    }
    let git = git.trim();
    if git.is_empty() {
        body.push_str("git: clean\n");
    } else {
        body.push_str(&clip_chars(git, MAX_REVIEW_BYTES.saturating_sub(body.len() + 40)));
        body.push('\n');
    }
    body.push_str("List bugs + missing tests only.\n");
    if body.len() > MAX_REVIEW_BYTES {
        body = clip_chars(&body, MAX_REVIEW_BYTES);
        body.push('\n');
    }
    body
}

/// Agents wait for CR (ConPTY / most TUIs). Broadcast and launch-task already append it.
fn submit_cr(s: &str) -> String {
    if s.ends_with('\r') || s.ends_with("\r\n") {
        s.to_string()
    } else {
        format!("{s}\r")
    }
}

pub async fn handoff_review(
    state: &AppState,
    source_id: &str,
    target_id: &str,
) -> Result<(), String> {
    let map = state.read().await;
    let source = map
        .get(source_id)
        .ok_or_else(|| format!("Source session '{source_id}' not found"))?;
    let target = map
        .get(target_id)
        .ok_or_else(|| format!("Target session '{target_id}' not found"))?;

    let dir = source
        .info
        .worktree_path
        .clone()
        .unwrap_or_else(|| source.info.working_dir.clone());

    let git = worktree::get_diff(Path::new(&dir)).unwrap_or_default();
    let role = source.info.role.as_deref().unwrap_or("Agent");
    let prompt = compact_review_prompt(
        role,
        &source.info.engine,
        &dir,
        source.info.task.as_deref(),
        &git,
    );

    let packet = submit_cr(&prompt);
    target
        .pty
        .as_ref()
        .ok_or_else(|| {
            format!("Target session '{target_id}' is exited — relaunch it before handoff")
        })?
        .write_input(packet.as_bytes())
        .map_err(|e| format!("Failed to deliver handoff to target session: {e}"))?;

    info!(
        source = %source_id,
        target = %target_id,
        bytes = prompt.len(),
        "Review packet delivered"
    );
    Ok(())
}

/// Terminates a session and cleans up its resources (including worktree if ephemeral).
pub async fn kill_session(state: &AppState, id: &str) -> Result<(), String> {
    if restarting_ids()
        .lock()
        .map(|s| s.contains(id))
        .unwrap_or(false)
    {
        return Err("Session is relaunching; wait a moment, then close it".to_string());
    }

    let mut map = state.write().await;

    if let Some(session) = map.remove(id) {
        if let Some(pty) = &session.pty {
            let _ = pty.kill();
        }
        if !session.info.hidden {
            persist::delete_session(id);
            cleanup_session_worktree(&session.info);
        }
        info!(session_id = %id, "Killed session successfully");
        Ok(())
    } else {
        Err(format!("Session '{id}' not found"))
    }
}

/// Parks every session for backend shutdown: dump scrollback, kill PTYs,
/// keep metadata + worktrees so the UI can relaunch after restart.
pub async fn park_all_sessions(state: &AppState) {
    let mut map = state.write().await;
    for session in map.values_mut() {
        if let Some(pty) = session.pty.take() {
            if !session.info.hidden {
                persist::save_scrollback(&session.info.id, &pty.get_history());
            }
            let _ = pty.kill();
        }
        session.info.is_alive = false;
        if !session.info.hidden {
            persist::persist_info(&session.info);
        }
    }
    // Drop hidden setup consoles entirely (no persistence).
    map.retain(|_, s| !s.info.hidden);
    info!("Parked all sessions for shutdown");
}

/// Terminates every session, including hidden setup consoles.
/// Used when the user wants a hard clear — also parks nothing (full delete).
pub async fn kill_all_sessions(state: &AppState) {
    let ids: Vec<String> = {
        let map = state.read().await;
        map.keys().cloned().collect()
    };
    for id in ids {
        let _ = kill_session(state, &id).await;
    }
}

fn cleanup_session_worktree(info: &SessionInfo) {
    let Some(wt_str) = &info.worktree_path else {
        return;
    };
    let wt_path = PathBuf::from(wt_str);
    let Some(agent_id) = wt_path.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_prefix("agent-")) else {
        return;
    };
    // Worktrees live outside the repo (app data dir), so ask git where the
    // repository is instead of guessing from the folder layout.
    let Some(repo) = worktree::repo_for_worktree(&wt_path) else {
        tracing::warn!(
            worktree = %wt_str,
            "Cannot resolve repository for worktree; skipping cleanup"
        );
        return;
    };
    if let Err(e) = worktree::remove_worktree(&repo, agent_id) {
        tracing::warn!(
            worktree = %wt_str,
            error = %e,
            "Failed to remove session worktree"
        );
    }
}

fn abandon_launched(launched: Vec<ActiveSession>) {
    for session in launched {
        if let Some(pty) = &session.pty {
            let _ = pty.kill();
        }
        cleanup_session_worktree(&session.info);
        persist::delete_session(&session.info.id);
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
    // Interactive PTYs need a carriage return to execute — typing alone leaves the line pending.
    let payload = if input.ends_with('\r') || input.ends_with('\n') {
        input.to_string()
    } else {
        format!("{input}\r")
    };
    let data = payload.as_bytes();

    let targets: Vec<String> = if let Some(ids) = session_ids {
        ids
    } else {
        map.keys().cloned().collect()
    };

    for id in targets {
        if let Some(session) = map.get(&id) {
            if let Some(pty) = &session.pty {
                if pty.write_input(data).is_ok() {
                    count += 1;
                }
            }
        }
    }

    Ok(count)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameGroupRequest {
    pub group_id: String,
    pub label: String,
}

/// Renames a launch group. Updates `group_label` on every node in that group.
pub async fn rename_group(
    state: &AppState,
    group_id: &str,
    label: &str,
) -> Result<String, String> {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Err("Session name cannot be empty".to_string());
    }
    if trimmed.len() > 80 {
        return Err("Session name is too long".to_string());
    }

    let mut map = state.write().await;
    let mut updated = 0;
    for session in map.values_mut() {
        if session.info.group_id == group_id {
            session.info.group_label = trimmed.to_string();
            persist::persist_info(&session.info);
            updated += 1;
        }
    }

    if updated == 0 {
        return Err(format!("Session group '{group_id}' not found"));
    }

    info!(group_id = %group_id, label = %trimmed, nodes = updated, "Renamed session group");
    Ok(trimmed.to_string())
}

async fn kill_sessions_for_engine(state: &AppState, engine: &str) {
    let ids: Vec<String> = {
        let map = state.read().await;
        map.values()
            .filter(|s| {
                !s.info.hidden
                    && s.info.is_alive
                    && s.info.engine.eq_ignore_ascii_case(engine)
            })
            .map(|s| s.info.id.clone())
            .collect()
    };
    for id in ids {
        let _ = kill_session(state, &id).await;
    }
}

async fn kill_hidden_sessions(state: &AppState, group_prefix: &str) {
    let ids: Vec<String> = {
        let map = state.read().await;
        map.values()
            .filter(|s| s.info.hidden && s.info.group_label.starts_with(group_prefix))
            .map(|s| s.info.id.clone())
            .collect()
    };
    for id in ids {
        let _ = kill_session(state, &id).await;
    }
}

/// Spawns a hidden shell and runs install / update / uninstall for an agent.
/// The PTY stays off the sidebar so it only appears in the Agents console.
pub async fn launch_agent_setup(
    state: &AppState,
    agent_id: &str,
    action: &str,
) -> Result<SessionInfo, String> {
    let action = action.trim().to_ascii_lowercase();
    if !matches!(action.as_str(), "install" | "update" | "uninstall") {
        return Err("Action must be install, update, or uninstall".to_string());
    }

    let supported = registry::get_supported_agents();
    let agent = supported
        .iter()
        .find(|a| a.id.eq_ignore_ascii_case(agent_id))
        .ok_or_else(|| format!("Unknown agent '{agent_id}'"))?;

    if registry::is_system_shell(&agent.id) {
        return Err("System shells are built in and cannot be installed or removed".to_string());
    }

    let (install_cmd, update_cmd, uninstall_cmd, hint) = registry::agent_lifecycle(&agent.id);
    let command = match action.as_str() {
        "install" => install_cmd,
        "update" => update_cmd,
        "uninstall" => uninstall_cmd,
        _ => None,
    }
    .ok_or_else(|| {
        hint.unwrap_or_else(|| format!("No {action} command for '{}'", agent.name))
    })?;

    if action == "uninstall" {
        kill_sessions_for_engine(state, &agent.id).await;
    }
    kill_hidden_sessions(state, "Agents ·").await;

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let group_id = Uuid::new_v4().to_string();
    let verb = match action.as_str() {
        "install" => "Install",
        "update" => "Update",
        "uninstall" => "Uninstall",
        _ => "Setup",
    };
    let name = format!("{verb} · {}", agent.name);
    let group_label = format!("Agents · {verb}");

    let wrapper = setup_lifecycle_script(&action, &agent.name, &command, &agent.binary);
    let mut session = spawn_single_session(
        &name,
        "shell",
        "Setup",
        Some(verb),
        &cwd,
        None,
        Some(&wrapper),
        &group_id,
        &group_label,
        24,
        100,
        None,
        None,
        None,
    )
    .await?;
    session.info.hidden = true;

    let info = session.info.clone();
    state.write().await.insert(info.id.clone(), session);

    info!(
        session_id = %info.id,
        agent_id = %agent.id,
        action = %action,
        "Opened hidden agent setup console"
    );
    Ok(info)
}

/// Hidden setup PTY that runs an arbitrary shell command (skills marketplace install).
pub async fn launch_command_setup(
    state: &AppState,
    title: &str,
    command: &str,
    display: Option<&str>,
) -> Result<SessionInfo, String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("Command is empty".to_string());
    }
    if command.len() > 4000 {
        return Err("Command too long".to_string());
    }

    kill_hidden_sessions(state, "Skills ·").await;

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let group_id = Uuid::new_v4().to_string();
    let name = {
        let t = title.trim();
        if t.is_empty() {
            "Skills · Install".to_string()
        } else {
            t.to_string()
        }
    };
    let group_label = "Skills · Setup".to_string();
    let wrapper = command_setup_script(command, display);

    let mut session = spawn_single_session(
        &name,
        "shell",
        "Setup",
        Some("Install"),
        &cwd,
        None,
        Some(&wrapper),
        &group_id,
        &group_label,
        24,
        100,
        None,
        None,
        None,
    )
    .await?;
    session.info.hidden = true;

    let info = session.info.clone();
    state.write().await.insert(info.id.clone(), session);

    info!(session_id = %info.id, "Opened hidden command setup console");
    Ok(info)
}

fn command_setup_script(command: &str, display: Option<&str>) -> String {
    let shown = ps_single_quote(display.unwrap_or(command));
    if cfg!(target_os = "windows") {
        // Embed the command as statements (same pattern as agent setup) — never
        // Invoke-Expression, which trips Defender and false-success on null exit.
        format!(
            "$ProgressPreference = 'SilentlyContinue'; \
             $env:Path = [Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User'); \
             Write-Host 'Running…'; Write-Host '{shown}'; Write-Host ''; \
             {command}; \
             if ($null -ne $LASTEXITCODE) {{ $code = $LASTEXITCODE }} \
             elseif (-not $?) {{ $code = 1 }} else {{ $code = 0 }}; \
             Write-Host ''; \
             if ($code -eq 0) {{ Write-Host 'Finished successfully.'; Write-Host '{sent_ok}' }} \
             else {{ Write-Host 'Command failed.'; Write-Host '{sent_fail}' }}",
            sent_ok = crate::pty_wire::SETUP_SENTINEL_OK,
            sent_fail = crate::pty_wire::SETUP_SENTINEL_FAIL,
        )
    } else {
        let shown_sh = display.unwrap_or(command).replace('\'', "'\\''");
        format!(
            "export PATH=\"$HOME/.local/bin:$HOME/.bun/bin:$(npm bin -g 2>/dev/null):$PATH\"; \
             printf 'Running…\\n%s\\n\\n' '{shown_sh}'; \
             {command}; \
             code=$?; printf '\\n'; \
             if [ \"$code\" -eq 0 ]; then printf 'Finished successfully.\\n{sent_ok}\\n'; \
             else printf 'Command failed.\\n{sent_fail}\\n'; fi",
            sent_ok = crate::pty_wire::SETUP_SENTINEL_OK,
            sent_fail = crate::pty_wire::SETUP_SENTINEL_FAIL,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_script_status_lines_for_each_action() {
        let install = setup_lifecycle_script(
            "install",
            "Antigravity CLI",
            "irm https://example/install.ps1 | iex",
            "agy",
        );
        assert!(install.contains("Installing Antigravity CLI..."));
        assert!(install.contains("Installed successfully."));
        assert!(install.contains("irm https://example/install.ps1 | iex"));
        assert!(install.contains(crate::pty_wire::SETUP_SENTINEL_OK));

        let update = setup_lifecycle_script("update", "Claude Code", "hermes update", "claude");
        assert!(update.contains("Updating Claude Code..."));
        assert!(update.contains("Updated successfully."));

        let remove = setup_lifecycle_script("uninstall", "Aider", "pip uninstall aider", "aider");
        assert!(remove.contains("Removing Aider..."));
        assert!(remove.contains("Removed successfully."));
        assert!(remove.contains("Remove failed."));
        if cfg!(windows) {
            assert!(remove.contains("Stop-Process -Name 'aider'"));
            assert!(remove.contains(r".local\share\aider"));
            assert!(remove.contains(r".bun\bin\aider.exe"));
        } else {
            assert!(remove.contains("pkill -x 'aider'"));
            assert!(remove.contains(".local/share/aider"));
            assert!(remove.contains(".bun/bin/aider"));
        }
        let install = setup_lifecycle_script("install", "Aider", "pip install aider", "aider");
        assert!(!install.contains("Stop-Process"));
        assert!(!install.contains("pkill -x"));
    }

    #[test]
    fn setup_script_verifies_npm_global_bin_after_install() {
        let install = setup_lifecycle_script(
            "install",
            "Pi",
            "npm install -g --ignore-scripts @earendil-works/pi-coding-agent",
            "pi",
        );
        if cfg!(windows) {
            assert!(
                install.contains("GetEnvironmentVariable('Path','User')"),
                "must refresh PATH after installers mutate User PATH"
            );
            assert!(
                install.contains("npm\\") || install.contains("APPDATA"),
                "must probe %APPDATA%\\npm for global npm shims: {install}"
            );
        } else {
            assert!(
                install.contains("npm bin -g") || install.contains(".bun/bin"),
                "must probe npm/bun global bins on Unix: {install}"
            );
        }
    }

    #[test]
    fn setup_script_echoes_command_before_running_it() {
        let cmd = r#"Remove-Item -Path "$env:LOCALAPPDATA\agy" -Recurse -Force"#;
        let script = setup_lifecycle_script("uninstall", "Antigravity CLI", cmd, "agy");
        let echoed = if cfg!(windows) {
            format!("Write-Host '{}'", ps_single_quote(cmd))
        } else {
            format!("printf '%s\\n\\n' '{}'", cmd.replace('\'', "'\\''"))
        };
        assert!(
            script.contains(&echoed),
            "setup script must print the command, not only execute it:\n{script}"
        );
        let echo_at = script.find(&echoed).expect("echoed command");
        let run_at = script.rfind(cmd).expect("executed command");
        assert!(
            echo_at < run_at,
            "command must be printed before it runs:\n{script}"
        );
    }

    #[test]
    fn command_setup_avoids_iex_and_eval() {
        let script = command_setup_script(
            "npx -y skills add owner/repo -y -g -a codex --copy",
            None,
        );
        assert!(!script.contains("Invoke-Expression"));
        assert!(!script.contains("eval "));
        assert!(script.contains("npx -y skills add"));
        assert!(script.contains(crate::pty_wire::SETUP_SENTINEL_OK));
    }

    #[test]
    #[cfg(windows)]
    fn setup_shell_uses_temp_ps1_file_not_inline_command() {
        let (program, args) = resolve_setup_shell_cmd("Write-Host 'termcrew-setup-test'").unwrap();
        assert!(
            program.to_lowercase().contains("powershell"),
            "expected powershell, got {program}"
        );
        assert!(
            !args.iter().any(|a| a == "-Command"),
            "inline -Command trips Defender Commando.A!ml: {args:?}"
        );
        let file_idx = args.iter().position(|a| a == "-File").expect("-File");
        let script_path = &args[file_idx + 1];
        assert!(
            script_path.ends_with(".ps1"),
            "expected .ps1 path, got {script_path}"
        );
        let body = std::fs::read_to_string(script_path).expect("temp script readable");
        assert!(body.contains("termcrew-setup-test"));
        let _ = std::fs::remove_file(script_path);
    }

    #[test]
    fn chat_ready_detects_agy_welcome_and_theme_picker() {
        let agy = "Antigravity CLI 1.2.1\nD:\\code\\tttt\n>\n? for shortcuts\n";
        assert!(looks_like_chat_ready(agy));
        assert!(!looks_like_theme_picker(agy));
        assert!(looks_like_theme_picker("Select theme\n1. Auto\n2. Dark\n"));
        assert!(!looks_like_chat_ready("Loading models…\n"));
        let stripped = strip_ansi(b"\x1b[32m> \x1b[0m");
        assert!(stripped.contains('>'));
    }

    #[test]
    fn next_add_role_pair_review_and_shell() {
        let pair = vec!["Lead".into(), "Review 1".into()];
        assert_eq!(
            next_add_role("Pair", "codex", &pair),
            ("Review 2".into(), false)
        );
        assert_eq!(
            next_add_role("Pair", "shell", &pair),
            ("Shell".into(), false)
        );
        assert_eq!(
            next_add_role("Swarm", "claude", &["Worker 1".into(), "Worker 2".into()]),
            ("Worker 3".into(), true)
        );
        assert_eq!(
            next_add_role("Solo", "agy", &["Lead".into()]),
            ("Lead 2".into(), false)
        );
    }

    #[test]
    fn launch_dir_requires_explicit_folder() {
        assert!(resolve_launch_dir("").unwrap_err().contains("Choose a folder"));
        assert!(resolve_launch_dir("   ").unwrap_err().contains("Choose a folder"));
        let missing = std::env::temp_dir().join(format!("termcrew-missing-{}", Uuid::new_v4()));
        let err = resolve_launch_dir(&missing.to_string_lossy()).unwrap_err();
        assert!(err.contains("does not exist"), "{err}");
        let ok = resolve_launch_dir(std::env::temp_dir().to_str().unwrap()).unwrap();
        assert!(ok.is_dir());
    }

    #[test]
    fn review_prompt_is_compact_and_skips_empty_task() {
        let p = compact_review_prompt("Lead", "agy", r"D:\code\yotest", None, "?? app.py\n");
        assert!(p.starts_with("REVIEW Lead agy"));
        assert!(p.contains("?? app.py"));
        assert!(p.contains("List bugs + missing tests only."));
        assert!(!p.contains("Task:"));
        assert!(!p.contains("[HANDOFF]"));
        assert!(p.len() < 400);
        let sent = submit_cr(&p);
        assert!(sent.ends_with('\r'), "{sent:?}");
        assert!(!sent.ends_with("\r\r"));
    }

    #[test]
    fn review_prompt_clips_task_and_huge_git() {
        let task = "x".repeat(400);
        let git = "y".repeat(20_000);
        let p = compact_review_prompt("Lead", "agy", "/repo", Some(&task), &git);
        assert!(p.contains("Task: "));
        assert!(p.len() <= MAX_REVIEW_BYTES + 8);
        assert!(!p.contains(&task));
    }

    #[test]
    fn crew_label_is_preset_then_number() {
        assert_eq!(format_group_label(1, "Pair", "agy"), "Pair 1");
        assert_eq!(format_group_label(3, "Swarm", "shell"), "Swarm 3");
    }
}
