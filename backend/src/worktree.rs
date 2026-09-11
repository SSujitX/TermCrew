//! Isolated git worktrees for agent sessions.
//!
//! Worktrees live in the app's data directory (`%LOCALAPPDATA%\termcrew\worktrees`
//! on Windows, XDG data dir on Unix), grouped by a hash of the repository path —
//! never inside the user's repository and never requiring `.gitignore` edits.
//! Git still registers each worktree under the repo's `.git/worktrees/`.

use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

/// FNV-1a 64-bit — stable across releases, no dependency.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// App data root: `%LOCALAPPDATA%\termcrew` (Windows) or
/// `$XDG_DATA_HOME/termcrew` / `~/.local/share/termcrew` (Unix).
pub fn app_data_dir() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("AppData").join("Local")));
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));

    let root = base.unwrap_or_else(|| PathBuf::from("."));
    let neu = root.join("termcrew");
    let legacy = root.join("multiagent");
    // One-shot rename so existing worktrees keep working after the rebrand.
    if !neu.exists() && legacy.is_dir() {
        match std::fs::rename(&legacy, &neu) {
            Ok(()) => info!(from = %legacy.display(), to = %neu.display(), "Migrated app data dir"),
            Err(e) => warn!(from = %legacy.display(), to = %neu.display(), error = %e, "Failed to migrate app data dir"),
        }
    }
    neu
}

/// Directory holding all worktrees for one repository, keyed by a hash of the
/// repo path (lower-cased on Windows so `D:\Code` and `d:\code` share a group).
pub fn worktree_root_for(base_dir: &Path) -> PathBuf {
    let canonical = base_dir.canonicalize().unwrap_or_else(|_| base_dir.to_path_buf());
    let mut key = canonical.to_string_lossy().to_string();
    if cfg!(windows) {
        key = key.to_lowercase();
    }
    let hash = format!("{:016x}", fnv1a(key.as_bytes()));
    app_data_dir().join("worktrees").join(&hash[..12])
}

/// Ensures that `base_dir` is a valid git repository with at least one commit.
/// `git worktree add` requires a valid HEAD reference to create branches from.
pub fn ensure_git_repo(base_dir: &Path) -> Result<(), String> {
    if !base_dir.exists() {
        std::fs::create_dir_all(base_dir)
            .map_err(|e| format!("Failed to create base directory {:?}: {}", base_dir, e))?;
    }

    let is_git = Command::new("git")
        .arg("-C")
        .arg(base_dir)
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .output();

    match is_git {
        Ok(output) if output.status.success() => {
            let head_check = Command::new("git")
                .arg("-C")
                .arg(base_dir)
                .arg("rev-parse")
                .arg("--verify")
                .arg("HEAD")
                .output();

            if let Ok(head_output) = head_check {
                if !head_output.status.success() {
                    info!("Repository has no commits; creating initial commit");
                    let _ = Command::new("git")
                        .arg("-C")
                        .arg(base_dir)
                        .args(["commit", "--allow-empty", "-m", "Initialize repository for TermCrew"])
                        .output();
                }
            }
        }
        _ => {
            info!("Initializing new git repository in {:?}", base_dir);
            let init_output = Command::new("git")
                .arg("-C")
                .arg(base_dir)
                .arg("init")
                .output()
                .map_err(|e| format!("Failed to run git init: {}", e))?;

            if !init_output.status.success() {
                return Err(format!(
                    "git init failed: {}",
                    String::from_utf8_lossy(&init_output.stderr)
                ));
            }

            let _ = Command::new("git")
                .arg("-C")
                .arg(base_dir)
                .args(["commit", "--allow-empty", "-m", "Initialize repository for TermCrew"])
                .output();
        }
    }

    Ok(())
}

/// Creates a new isolated git worktree for `agent_id` under the app data dir.
pub fn create_worktree(base_dir: &Path, agent_id: &str) -> Result<PathBuf, String> {
    ensure_git_repo(base_dir)?;

    let root = worktree_root_for(base_dir);
    std::fs::create_dir_all(&root).map_err(|e| format!("Failed to create worktree root: {e}"))?;

    let target_dir = root.join(format!("agent-{}", agent_id));
    let branch_name = format!("branch-{}", agent_id);

    let _ = Command::new("git").arg("-C").arg(base_dir).args(["worktree", "prune"]).output();

    // If destination already exists, remove it
    if target_dir.exists() {
        let _ = remove_worktree(base_dir, agent_id);
    }

    // Also delete branch if it already exists
    let _ = Command::new("git")
        .arg("-C")
        .arg(base_dir)
        .args(["branch", "-D", &branch_name])
        .output();

    let output = Command::new("git")
        .arg("-C")
        .arg(base_dir)
        .args(["worktree", "add"])
        .arg(&target_dir)
        .args(["-b", &branch_name])
        .output()
        .map_err(|e| format!("Failed to execute git worktree add: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git worktree add error: {}", err));
    }

    info!(
        path = ?target_dir,
        branch = %branch_name,
        "Created git worktree"
    );

    Ok(target_dir)
}

/// Cleans up a git worktree and its temporary branch.
pub fn remove_worktree(base_dir: &Path, agent_id: &str) -> Result<(), String> {
    let root = worktree_root_for(base_dir);
    let target_dir = root.join(format!("agent-{}", agent_id));
    let branch_name = format!("branch-{}", agent_id);

    let output = Command::new("git")
        .arg("-C")
        .arg(base_dir)
        .args(["worktree", "remove", "--force"])
        .arg(&target_dir)
        .output()
        .map_err(|e| format!("Failed to remove git worktree: {}", e))?;

    if !output.status.success() {
        warn!(
            "git worktree remove warning: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        // The directory may still linger if git refused; clean it directly so
        // the app data dir never accumulates dead worktrees.
        if target_dir.exists() {
            let _ = std::fs::remove_dir_all(&target_dir);
            let _ = Command::new("git").arg("-C").arg(base_dir).args(["worktree", "prune"]).output();
        }
    }

    let _ = Command::new("git")
        .arg("-C")
        .arg(base_dir)
        .args(["branch", "-D", &branch_name])
        .output();

    let _ = Command::new("git").arg("-C").arg(base_dir).args(["worktree", "prune"]).output();

    Ok(())
}

/// Derives the repository for a worktree by asking git itself, so cleanup
/// works no matter where the worktree lives.
pub fn repo_for_worktree(worktree_path: &Path) -> Option<PathBuf> {
    let out = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let common_dir = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    // `<repo>/.git` → `<repo>`
    common_dir.parent().map(|p| p.to_path_buf())
}

/// Removes stale worktree directories: any `agent-*` dir in the app data dir
/// whose git registration (repo `.git/worktrees/<name>`) no longer exists.
/// Called once at startup — needs no session persistence.
pub fn sweep_stale_worktrees() {
    sweep_stale_worktrees_at(&app_data_dir().join("worktrees"));
}

fn sweep_stale_worktrees_at(root: &Path) {
    let Ok(project_dirs) = std::fs::read_dir(root) else {
        return;
    };

    for project in project_dirs.flatten() {
        let project_path = project.path();
        let Ok(entries) = std::fs::read_dir(&project_path) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() || !dir.file_name().is_some_and(|n| n.to_string_lossy().starts_with("agent-")) {
                continue;
            }
            let git_file = dir.join(".git");
            let registered = std::fs::read_to_string(&git_file)
                .ok()
                .and_then(|content| content.strip_prefix("gitdir: ").map(|s| s.trim().to_string()))
                .map(|gitdir| !gitdir.is_empty() && Path::new(&gitdir).exists())
                .unwrap_or(false);
            if !registered {
                warn!(dir = %dir.display(), "Removing stale worktree directory");
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
        // Drop project groups that are now empty.
        let _ = std::fs::remove_dir(&project_path);
    }
}

/// Retrieves the git diff and status of a directory (e.g. worktree or main repo).
pub fn get_diff(target_dir: &Path) -> Result<String, String> {
    if !target_dir.exists() {
        return Err(format!("Directory {:?} does not exist", target_dir));
    }

    let diff_output = Command::new("git")
        .arg("-C")
        .arg(target_dir)
        .args(["diff", "HEAD"])
        .output()
        .map_err(|e| format!("Failed to run git diff: {}", e))?;

    let mut diff = String::from_utf8_lossy(&diff_output.stdout).to_string();

    let status_output = Command::new("git")
        .arg("-C")
        .arg(target_dir)
        .args(["status", "--short"])
        .output();

    if let Ok(status) = status_output {
        let status_text = String::from_utf8_lossy(&status.stdout);
        if !status_text.is_empty() {
            diff.push_str("\n--- Untracked / Staged Status ---\n");
            diff.push_str(&status_text);
        }
    }

    Ok(diff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_repo_shares_root_and_differs_per_repo() {
        let a = std::env::temp_dir().join(format!("wt_repo_a_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&a).unwrap();
        let b = std::env::temp_dir().join(format!("wt_repo_b_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&b).unwrap();

        let root_a = worktree_root_for(&a);
        assert!(root_a.starts_with(app_data_dir()));
        assert_eq!(root_a, worktree_root_for(&a), "same repo → same group");
        assert_ne!(root_a, worktree_root_for(&b), "different repos → different groups");
        assert_eq!(root_a.file_name().unwrap().to_string_lossy().len(), 12);

        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn worktree_lifecycle_in_app_data_dir_without_gitignore() {
        let repo = std::env::temp_dir().join(format!("wt_life_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&repo).unwrap();

        ensure_git_repo(&repo).expect("ensure_git_repo");
        assert!(
            !repo.join(".gitignore").exists(),
            "ensure_git_repo must not create or modify the user's .gitignore"
        );

        let wt = create_worktree(&repo, "test-agent-1").expect("create_worktree");
        assert!(wt.exists(), "worktree must exist");
        assert!(
            wt.starts_with(app_data_dir()),
            "worktree must live under the app data dir, got {}",
            wt.display()
        );
        assert!(!repo.join(".worktrees").exists(), "nothing may be written into the repo");

        // Diff works inside the isolated worktree
        assert!(get_diff(&wt).is_ok());

        remove_worktree(&repo, "test-agent-1").expect("remove_worktree");
        assert!(!wt.exists(), "worktree dir must be removed");
        // Repo branch was also cleaned
        let branches = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["branch", "--list", "branch-test-agent-1"])
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&branches.stdout).trim().is_empty());

        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn sweep_removes_unregistered_worktree_dirs() {
        let fake_root = std::env::temp_dir().join(format!("wt_sweep_{}", uuid::Uuid::new_v4()));
        let project = fake_root.join("abc123def456");
        let live = project.join("agent-live");
        let orphan = project.join("agent-orphan");
        std::fs::create_dir_all(&live).unwrap();
        std::fs::create_dir_all(&orphan).unwrap();

        let fake_gitdir = std::env::temp_dir().join(format!("fake_repo_git_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&fake_gitdir).unwrap();
        std::fs::write(live.join(".git"), format!("gitdir: {}", fake_gitdir.display())).unwrap();
        std::fs::write(orphan.join(".git"), "gitdir: D:\\nonexistent\\.git\\worktrees\\agent-orphan").unwrap();

        sweep_stale_worktrees_at(&fake_root);

        assert!(live.exists(), "registered worktree must survive the sweep");
        assert!(!orphan.exists(), "unregistered worktree must be swept");
        let _ = std::fs::remove_dir_all(&fake_root);
        let _ = std::fs::remove_dir_all(&fake_gitdir);
    }
}
