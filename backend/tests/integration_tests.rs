use backend::pty_manager::PtyManager;
use backend::registry::{detect_agents, get_supported_agents};
use backend::session::{
    broadcast_input, create_app_state, kill_session, launch_preset, list_sessions, LaunchRequest,
};
use backend::worktree::{create_worktree, ensure_git_repo, get_diff, remove_worktree};
use std::time::Duration;
use tokio::time::sleep;

#[test]
fn test_agent_registry_detection() {
    let supported = get_supported_agents();
    assert_eq!(supported.len(), 9);

    let expected_ids = [
        "claude",
        "opencode",
        "aider",
        "gemini",
        "cursor-agent",
        "codex",
        "deepseek",
        "grok",
        "shell",
    ];

    for id in expected_ids {
        assert!(
            supported.iter().any(|a| a.id == id),
            "Expected agent '{id}' to be supported"
        );
    }

    let detected = detect_agents();
    assert_eq!(detected.len(), 9);

    // On Windows, PowerShell is guaranteed to be installed
    if cfg!(target_os = "windows") {
        let shell = detected
            .iter()
            .find(|a| a.id == "shell")
            .expect("Shell agent must be in detected list");
        assert!(
            shell.is_installed,
            "PowerShell should be detected as installed on Windows"
        );
        assert!(
            shell.binary_path.is_some(),
            "PowerShell path should be resolved"
        );
    }
}

#[tokio::test]
async fn test_pty_spawn_and_communication() {
    let shell_cmd = if cfg!(target_os = "windows") {
        "powershell.exe"
    } else {
        "bash"
    };

    let pty = PtyManager::spawn(shell_cmd, &[], None, &[], 24, 80)
        .expect("Failed to spawn shell PTY");

    assert!(pty.is_alive(), "PTY process should be alive after spawn");

    let mut rx = pty.subscribe();

    // Send input command
    let test_cmd = if cfg!(target_os = "windows") {
        "Write-Output \"TEST_PTY_OK\"\r\n"
    } else {
        "echo TEST_PTY_OK\n"
    };

    // Small delay to allow shell prompt to appear
    sleep(Duration::from_millis(500)).await;

    pty.write_input(test_cmd.as_bytes())
        .expect("Failed to write input to PTY");

    // Listen for broadcast output
    let mut received_expected = false;
    let timeout = sleep(Duration::from_secs(4));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            _ = &mut timeout => break,
            Ok(chunk) = rx.recv() => {
                if chunk.contains("TEST_PTY_OK") {
                    received_expected = true;
                    break;
                }
            }
        }
    }

    // Also verify resize
    assert!(pty.resize(100, 30).is_ok());

    // Kill PTY
    let _ = pty.kill();
    sleep(Duration::from_millis(100)).await;
    assert!(!pty.is_alive());
    assert!(received_expected, "PTY should have received and broadcast 'TEST_PTY_OK'");
}

#[test]
fn test_git_worktree_lifecycle() {
    let temp_dir = std::env::temp_dir().join(format!("test_multiagent_wt_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);

    // 1. Ensure git repo
    let init_res = ensure_git_repo(&temp_dir);
    assert!(init_res.is_ok(), "ensure_git_repo failed: {:?}", init_res);

    // 2. Create worktree
    let agent_id = "test-agent-1";
    let wt_res = create_worktree(&temp_dir, agent_id);
    assert!(wt_res.is_ok(), "create_worktree failed: {:?}", wt_res);
    let wt_path = wt_res.unwrap();
    assert!(wt_path.exists(), "Worktree path must exist");

    // 3. Diff in worktree
    let diff_res = get_diff(&wt_path);
    assert!(diff_res.is_ok(), "get_diff failed: {:?}", diff_res);

    // 4. Remove worktree
    let rm_res = remove_worktree(&temp_dir, agent_id);
    assert!(rm_res.is_ok(), "remove_worktree failed: {:?}", rm_res);

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_preset_workbench_session_lifecycle() {
    let state = create_app_state();
    let temp_dir = std::env::temp_dir().join(format!("test_multiagent_session_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);

    let req = LaunchRequest {
        preset: "Workbench".to_string(),
        engine: "shell".to_string(),
        base_dir: temp_dir.to_string_lossy().to_string(),
        task: Some("Get-Date".to_string()),
        count: None,
        reviewer_engine: None,
    };

    let launched = launch_preset(&state, req)
        .await
        .expect("Workbench preset launch failed");

    // Workbench creates 2 sessions (Agent + Shell)
    assert_eq!(launched.len(), 2);

    let active_list = list_sessions(&state).await;
    assert_eq!(active_list.len(), 2);

    // Test broadcast input
    let broadcast_res = broadcast_input(&state, "Write-Output 'BROADCAST_TEST'\r\n", None).await;
    assert!(broadcast_res.is_ok());
    assert_eq!(broadcast_res.unwrap(), 2);

    // Kill both sessions
    for session in &launched {
        let kill_res = kill_session(&state, &session.id).await;
        assert!(kill_res.is_ok());
    }

    let remaining = list_sessions(&state).await;
    assert_eq!(remaining.len(), 0);

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}
