use termcrew::pty_manager::PtyManager;
use termcrew::registry::{detect_agents, get_supported_agents};
use termcrew::session::{
    broadcast_input, create_app_state, kill_session, launch_agent_setup, launch_preset,
    list_sessions, LaunchRequest,
};
use termcrew::worktree::{create_worktree, ensure_git_repo, get_diff, remove_worktree};
use std::time::Duration;
use tokio::time::sleep;

#[test]
fn test_agent_registry_detection() {
    let supported = get_supported_agents();
    assert!(supported.len() >= 9, "registry should include core agents");

    let expected_ids = [
        "claude",
        "opencode",
        "openclaw",
        "pi",
        "hermes",
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

    if cfg!(target_os = "windows") {
        for id in ["cmd", "git-bash", "wsl"] {
            assert!(
                supported.iter().any(|a| a.id == id),
                "Expected shell '{id}' to be supported on Windows"
            );
        }
        assert!(
            supported.iter().all(|a| a.id != "pwsh"),
            "PowerShell 7 should not be listed as a Windows agent"
        );
    } else {
        assert!(
            supported.iter().any(|a| a.id == "sh"),
            "POSIX sh should be listed on Unix"
        );
        if cfg!(target_os = "macos") {
            assert!(supported.iter().any(|a| a.id == "bash"));
            let default = supported.iter().find(|a| a.id == "shell").unwrap();
            assert_eq!(default.binary, "zsh");
        }
    }

    let detected = detect_agents();
    assert_eq!(detected.len(), supported.len());

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
        assert!(
            shell.install_cmd.is_none(),
            "System shell should not expose an install command"
        );

        let cmd = detected
            .iter()
            .find(|a| a.id == "cmd")
            .expect("Command Prompt must be in detected list");
        assert!(cmd.is_installed, "cmd.exe should be present on Windows");
        assert!(cmd.install_cmd.is_none());
    }

    let claude = detected
        .iter()
        .find(|a| a.id == "claude")
        .expect("Claude must be in detected list");
    let expected_claude_install = if cfg!(target_os = "windows") {
        "irm https://claude.ai/install.ps1 | iex"
    } else {
        "curl -fsSL https://claude.ai/install.sh | bash"
    };
    assert_eq!(
        claude.install_cmd.as_deref(),
        Some(expected_claude_install),
        "Claude install must match official native installer docs"
    );
    assert!(claude.update_cmd.is_some());
    assert!(claude.uninstall_cmd.is_some());
    assert_eq!(
        claude.docs_url.as_deref(),
        Some("https://code.claude.com/docs/en/overview"),
        "Claude must expose the official docs URL"
    );

    let grok = detected
        .iter()
        .find(|a| a.id == "grok")
        .expect("Grok must be in detected list");
    assert!(
        grok.install_cmd
            .as_deref()
            .is_some_and(|c| c.contains("x.ai/cli")),
        "Grok must use the official xAI installer, not npm grok-cli"
    );

    let cursor = detected
        .iter()
        .find(|a| a.id == "cursor-agent")
        .expect("Cursor must be in detected list");
    assert_eq!(cursor.binary, "agent");

    let deepseek = detected
        .iter()
        .find(|a| a.id == "deepseek")
        .expect("DeepSeek Harness must be in detected list");
    assert_eq!(deepseek.binary, "dsh");
    assert!(
        deepseek
            .install_cmd
            .as_deref()
            .is_some_and(|c| c.contains("@deepseek-ai/dsh")),
        "DeepSeek must install the official @deepseek-ai/dsh package"
    );
    assert_eq!(
        deepseek.docs_url.as_deref(),
        Some("https://deepseek-harness.github.io/deepseek-harness/"),
        "DeepSeek must expose the official Harness docs URL"
    );
}

#[tokio::test]
async fn test_pty_spawn_and_communication() {
    let shell_cmd = if cfg!(target_os = "windows") {
        "powershell.exe"
    } else {
        "bash"
    };

    let pty = PtyManager::spawn(shell_cmd, &[], None, &[], 24, 80, None)
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
                let text = String::from_utf8_lossy(&chunk);
                if text.contains("TEST_PTY_OK") {
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
    let temp_dir = std::env::temp_dir().join(format!("test_termcrew_wt_{}", uuid::Uuid::new_v4()));
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
    let temp_dir = std::env::temp_dir().join(format!("test_termcrew_session_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);

    let req = LaunchRequest {
        preset: "Workbench".to_string(),
        engine: "shell".to_string(),
        base_dir: temp_dir.to_string_lossy().to_string(),
        task: Some("Get-Date".to_string()),
        count: None,
        reviewer_engine: None,
        engines: None,
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

#[tokio::test]
async fn test_agent_setup_console_is_hidden() {
    let state = create_app_state();
    let info = launch_agent_setup(&state, "aider", "install")
        .await
        .expect("setup console should spawn a hidden shell");

    assert!(info.hidden, "setup sessions must stay off the sidebar");
    assert_eq!(info.engine, "shell");

    let listed = list_sessions(&state).await;
    assert!(
        listed.is_empty(),
        "hidden setup consoles must not appear in GET /api/sessions"
    );

    let _ = kill_session(&state, &info.id).await;
    let remaining = list_sessions(&state).await;
    assert!(remaining.is_empty());
}

/// Extracts the last `PREFIX=<digits>` line printed into the PTY scrollback.
fn find_pid_in_history(pty: &PtyManager, prefix: &str) -> Option<u32> {
    let history = String::from_utf8_lossy(&pty.get_history()).to_string();
    history
        .lines()
        .filter_map(|line| line.trim().strip_prefix(prefix))
        .filter_map(|rest| rest.trim().parse::<u32>().ok())
        .next_back()
}

#[tokio::test]
async fn test_kill_terminates_whole_process_tree() {
    // Spawn a shell that launches a grandchild designed to outlive it, let
    // the grandchild report its PID into the shared PTY, then kill the
    // session. Tree-kill means the grandchild must be gone too.
    let (program, args) = if cfg!(target_os = "windows") {
        (
            "powershell.exe",
            vec![
                "-NoLogo".to_string(),
                "-NoProfile".to_string(),
                "-Command".to_string(),
                "Write-Output ('PARENT=' + $PID); \
                 Start-Process powershell -NoNewWindow -ArgumentList \
                   '-NoProfile','-Command','Write-Output (''GRAND='' + $PID); Start-Sleep 120' | Out-Null; \
                 Start-Sleep 120"
                    .to_string(),
            ],
        )
    } else {
        (
            "sh",
            vec!["-c".to_string(), "echo PARENT=$$; sleep 120 & echo GRAND=$!; wait".to_string()],
        )
    };

    let pty = PtyManager::spawn(program, &args, None, &[], 24, 80, None)
        .expect("failed to spawn tree-kill test shell");

    // Wait for the grandchild to start and print its PID.
    let grand_pid = {
        let mut found = None;
        for _ in 0..60 {
            if let Some(pid) = find_pid_in_history(&pty, "GRAND=") {
                found = Some(pid);
                break;
            }
            sleep(Duration::from_millis(500)).await;
        }
        found.expect("grandchild never reported its PID into the PTY")
    };

    pty.kill().expect("kill should succeed");

    // Poll for the grandchild's death instead of assuming signal latency.
    let mut gone = false;
    for _ in 0..20 {
        gone = if cfg!(target_os = "windows") {
            let out = std::process::Command::new("tasklist")
                .args(["/FI", &format!("PID eq {grand_pid}"), "/NH"])
                .output()
                .expect("tasklist must be available on Windows");
            !String::from_utf8_lossy(&out.stdout).contains(&grand_pid.to_string())
        } else {
            std::process::Command::new("kill")
                .args(["-0", &grand_pid.to_string()])
                .status()
                .map(|s| !s.success())
                .unwrap_or(true)
        };
        if gone {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    assert!(
        gone,
        "grandchild (pid {grand_pid}) survived the session kill — process tree was not terminated"
    );
}
