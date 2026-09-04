use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

/// Ensures that `base_dir` is a valid git repository with at least one commit.
/// `git worktree add` requires a valid HEAD reference to create branches from.
pub fn ensure_git_repo(base_dir: &Path) -> Result<(), String> {
    if !base_dir.exists() {
        std::fs::create_dir_all(base_dir)
            .map_err(|e| format!("Failed to create base directory {:?}: {}", base_dir, e))?;
    }

    // Check if it's already a git repository
    let is_git = Command::new("git")
        .arg("-C")
        .arg(base_dir)
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .output();

    match is_git {
        Ok(output) if output.status.success() => {
            // Already a git repo; check if HEAD exists
            let head_check = Command::new("git")
                .arg("-C")
                .arg(base_dir)
                .arg("rev-parse")
                .arg("--verify")
                .arg("HEAD")
                .output();

            if let Ok(head_output) = head_check {
                if !head_output.status.success() {
                    // Create initial commit if empty repo
                    info!("Repository has no commits; creating initial commit");
                    let _ = Command::new("git")
                        .arg("-C")
                        .arg(base_dir)
                        .args(["commit", "--allow-empty", "-m", "Initialize repository for multi-agent"])
                        .output();
                }
            }
        }
        _ => {
            // Initialize new git repository
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

            // Create initial commit
            let _ = Command::new("git")
                .arg("-C")
                .arg(base_dir)
                .args(["commit", "--allow-empty", "-m", "Initialize repository for multi-agent"])
                .output();
        }
    }

    // Ensure .gitignore ignores .worktrees
    let gitignore_path = base_dir.join(".gitignore");
    let mut gitignore_content = if gitignore_path.exists() {
        std::fs::read_to_string(&gitignore_path).unwrap_or_default()
    } else {
        String::new()
    };

    if !gitignore_content.contains(".worktrees") {
        if !gitignore_content.is_empty() && !gitignore_content.ends_with('\n') {
            gitignore_content.push('\n');
        }
        gitignore_content.push_str(".worktrees/\n");
        let _ = std::fs::write(&gitignore_path, gitignore_content);
    }

    Ok(())
}

/// Creates a new isolated git worktree:
/// `git worktree add .worktrees/agent-<id> -b branch-<id>`
pub fn create_worktree(base_dir: &Path, agent_id: &str) -> Result<PathBuf, String> {
    ensure_git_repo(base_dir)?;

    let worktree_dir = base_dir.join(".worktrees");
    if !worktree_dir.exists() {
        let _ = std::fs::create_dir_all(&worktree_dir);
    }

    let target_dir = worktree_dir.join(format!("agent-{}", agent_id));
    let branch_name = format!("branch-{}", agent_id);

    // Prune stale worktree references first
    let _ = Command::new("git")
        .arg("-C")
        .arg(base_dir)
        .args(["worktree", "prune"])
        .output();

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

    let relative_target = format!(".worktrees/agent-{}", agent_id);
    let output = Command::new("git")
        .arg("-C")
        .arg(base_dir)
        .args(["worktree", "add", &relative_target, "-b", &branch_name])
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

/// Cleans up a git worktree and its temporary branch:
/// `git worktree remove --force .worktrees/agent-<id>`
pub fn remove_worktree(base_dir: &Path, agent_id: &str) -> Result<(), String> {
    let relative_target = format!(".worktrees/agent-{}", agent_id);
    let branch_name = format!("branch-{}", agent_id);

    let output = Command::new("git")
        .arg("-C")
        .arg(base_dir)
        .args(["worktree", "remove", "--force", &relative_target])
        .output()
        .map_err(|e| format!("Failed to remove git worktree: {}", e))?;

    if !output.status.success() {
        warn!(
            "git worktree remove warning: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Delete branch
    let _ = Command::new("git")
        .arg("-C")
        .arg(base_dir)
        .args(["branch", "-D", &branch_name])
        .output();

    // Prune worktrees
    let _ = Command::new("git")
        .arg("-C")
        .arg(base_dir)
        .args(["worktree", "prune"])
        .output();

    Ok(())
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

    // Also get status for untracked / new files
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
