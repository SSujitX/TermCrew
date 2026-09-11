//! On-disk session metadata and scrollback for TermCrew.
//!
//! Layout under [`worktree::app_data_dir`]:
//! - `sessions/<id>.json` — session metadata (survives backend restart)
//! - `scrollback/<id>.bin` — last ~128 KB of PTY output
//! - `worktrees/…` — git worktrees (owned by worktree module)
//!
//! User kill removes session + scrollback files. Backend shutdown parks
//! sessions (keeps files + worktrees) so the UI can relaunch them.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::warn;

use crate::worktree;

const SESSIONS_DIR: &str = "sessions";
const SCROLLBACK_DIR: &str = "scrollback";

/// Disk form of a session — runtime `is_alive` is always false after load.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSession {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub preset: String,
    pub role: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    pub working_dir: String,
    pub worktree_path: Option<String>,
    pub created_at: String,
    pub group_id: String,
    pub group_label: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub task: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageInfo {
    pub root: String,
    pub sessions: String,
    pub scrollback: String,
    pub worktrees: String,
    pub recents: String,
}

fn sessions_dir() -> PathBuf {
    worktree::app_data_dir().join(SESSIONS_DIR)
}

fn scrollback_dir() -> PathBuf {
    worktree::app_data_dir().join(SCROLLBACK_DIR)
}

fn safe_id(id: &str) -> Option<&str> {
    if id.is_empty() || id.len() > 80 {
        return None;
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(id)
}

fn session_path(id: &str) -> Option<PathBuf> {
    safe_id(id).map(|id| sessions_dir().join(format!("{id}.json")))
}

fn scrollback_path(id: &str) -> Option<PathBuf> {
    safe_id(id).map(|id| scrollback_dir().join(format!("{id}.bin")))
}

fn ensure_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("Failed to create {}: {e}", path.display()))
}

pub fn storage_info() -> StorageInfo {
    let root = worktree::app_data_dir();
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    StorageInfo {
        root: root.display().to_string(),
        sessions: root.join(SESSIONS_DIR).display().to_string(),
        scrollback: root.join(SCROLLBACK_DIR).display().to_string(),
        worktrees: root.join("worktrees").display().to_string(),
        recents: home.join(".termcrew").join("recent_workdirs.json").display().to_string(),
    }
}

/// Opens the TermCrew data root in the OS file manager.
pub fn open_data_dir() -> Result<(), String> {
    let root = worktree::app_data_dir();
    ensure_dir(&root)?;
    open_path(&root)
}

/// Opens a folder in Explorer (Windows), Finder (macOS), or xdg-open (Linux).
pub fn open_dir(path: &str) -> Result<(), String> {
    open_path(&resolve_open_dir(path)?)
}

fn resolve_open_dir(path: &str) -> Result<PathBuf, String> {
    let raw = path.trim();
    if raw.is_empty() {
        return Err("Folder path is empty".into());
    }
    let p = PathBuf::from(raw);
    if !p.exists() {
        return Err(format!("Folder does not exist: {}", p.display()));
    }
    if !p.is_dir() {
        return Err(format!("Not a folder: {}", p.display()));
    }
    Ok(p)
}

fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open explorer: {e}"))?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open Finder: {e}"))?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open file manager: {e}"))?;
        Ok(())
    }
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    ensure_dir(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("termcrew")
    ));
    fs::write(&tmp, data).map_err(|e| format!("Failed to write temp {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Failed to replace {}: {e}", path.display())
    })
}

pub fn save_session(session: &PersistedSession) -> Result<(), String> {
    if session.hidden {
        return Ok(());
    }
    let path = session_path(&session.id).ok_or_else(|| "Invalid session id".to_string())?;
    ensure_dir(&sessions_dir())?;
    let json = serde_json::to_string_pretty(session)
        .map_err(|e| format!("Failed to serialize session: {e}"))?;
    write_atomic(&path, json.as_bytes())
}

pub fn load_all_sessions() -> Vec<PersistedSession> {
    let dir = sessions_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if safe_id(&stem).is_none() {
            warn!(file = %path.display(), "Skipping session file with unsafe name");
            continue;
        }
        match fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<PersistedSession>(&raw) {
                Ok(s) if s.hidden => {}
                Ok(s) if safe_id(&s.id).is_none() || s.id != stem => {
                    warn!(
                        file = %path.display(),
                        id = %s.id,
                        "Skipping session file whose id does not match filename"
                    );
                }
                Ok(s) => out.push(s),
                Err(e) => warn!(file = %path.display(), error = %e, "Corrupt session file; skipping"),
            },
            Err(e) => warn!(file = %path.display(), error = %e, "Failed to read session file"),
        }
    }
    out
}

pub fn delete_session(id: &str) {
    if let Some(path) = session_path(id) {
        if path.exists() {
            if let Err(e) = fs::remove_file(&path) {
                warn!(file = %path.display(), error = %e, "Failed to delete session file");
            }
        }
    }
    delete_scrollback(id);
}

pub fn save_scrollback(id: &str, data: &[u8]) {
    let Some(path) = scrollback_path(id) else {
        return;
    };
    if let Err(e) = ensure_dir(&scrollback_dir()) {
        warn!(error = %e, "Cannot create scrollback dir");
        return;
    }
    if let Err(e) = write_atomic(&path, data) {
        warn!(file = %path.display(), error = %e, "Failed to write scrollback");
    }
}

pub fn load_scrollback(id: &str) -> Vec<u8> {
    let Some(path) = scrollback_path(id) else {
        return Vec::new();
    };
    fs::read(&path).unwrap_or_default()
}

pub fn delete_scrollback(id: &str) {
    if let Some(path) = scrollback_path(id) {
        if path.exists() {
            let _ = fs::remove_file(&path);
        }
    }
}

pub fn from_session_info(info: &crate::session::SessionInfo) -> PersistedSession {
    PersistedSession {
        id: info.id.clone(),
        name: info.name.clone(),
        engine: info.engine.clone(),
        preset: info.preset.clone(),
        role: info.role.clone(),
        label: info.label.clone(),
        working_dir: info.working_dir.clone(),
        worktree_path: info.worktree_path.clone(),
        created_at: info.created_at.clone(),
        group_id: info.group_id.clone(),
        group_label: info.group_label.clone(),
        hidden: info.hidden,
        task: info.task.clone(),
    }
}

pub fn to_session_info(p: PersistedSession) -> crate::session::SessionInfo {
    crate::session::SessionInfo {
        id: p.id,
        name: p.name,
        engine: p.engine,
        preset: p.preset,
        role: p.role,
        label: p.label,
        working_dir: p.working_dir,
        worktree_path: p.worktree_path,
        created_at: p.created_at,
        is_alive: false,
        group_id: p.group_id,
        group_label: p.group_label,
        hidden: p.hidden,
        task: p.task,
    }
}

/// Best-effort persist after launch / rename / park.
pub fn persist_info(info: &crate::session::SessionInfo) {
    if info.hidden {
        return;
    }
    if let Err(e) = save_session(&from_session_info(info)) {
        warn!(session_id = %info.id, error = %e, "Failed to persist session");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_session_json() {
        let p = PersistedSession {
            id: "abc".into(),
            name: "Test".into(),
            engine: "shell".into(),
            preset: "Solo".into(),
            role: Some("Primary".into()),
            label: None,
            working_dir: "/tmp".into(),
            worktree_path: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            group_id: "g1".into(),
            group_label: "Session 1".into(),
            hidden: false,
            task: None,
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: PersistedSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "abc");
        assert_eq!(back.group_label, "Session 1");
    }

    #[test]
    fn safe_id_rejects_traversal() {
        assert!(safe_id("abc-123").is_some());
        assert!(safe_id("../etc").is_none());
        assert!(safe_id("a/b").is_none());
        assert!(safe_id("").is_none());
    }

    #[test]
    fn resolve_open_dir_rejects_empty_and_missing() {
        assert!(resolve_open_dir("").is_err());
        assert!(resolve_open_dir("   ").is_err());
        let err = resolve_open_dir("/no/such/termcrew-open-dir-test").unwrap_err();
        assert!(err.contains("does not exist"), "{err}");
    }

    #[test]
    fn resolve_open_dir_accepts_temp() {
        let dir = std::env::temp_dir();
        let s = dir.to_str().expect("temp dir utf-8");
        let resolved = resolve_open_dir(s).expect("temp dir should open");
        assert!(resolved.is_dir());
    }
}
