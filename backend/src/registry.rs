use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub id: String,
    pub name: String,
    pub binary: String,
    pub description: String,
    pub category: String,
    pub default_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedAgent {
    pub id: String,
    pub name: String,
    pub binary: String,
    pub description: String,
    pub category: String,
    pub is_installed: bool,
    pub binary_path: Option<String>,
}

/// Returns the predefined list of supported agents and shells.
pub fn get_supported_agents() -> Vec<AgentDefinition> {
    let shell_binary = if cfg!(target_os = "windows") {
        "powershell.exe".to_string()
    } else {
        "bash".to_string()
    };

    vec![
        AgentDefinition {
            id: "claude".to_string(),
            name: "Claude Code".to_string(),
            binary: "claude".to_string(),
            description: "Anthropic's agentic CLI for coding and reasoning".to_string(),
            category: "Agent".to_string(),
            default_args: vec![],
        },
        AgentDefinition {
            id: "opencode".to_string(),
            name: "OpenCode".to_string(),
            binary: "opencode".to_string(),
            description: "Open-source autonomous terminal coding assistant".to_string(),
            category: "Agent".to_string(),
            default_args: vec![],
        },
        AgentDefinition {
            id: "aider".to_string(),
            name: "Aider".to_string(),
            binary: "aider".to_string(),
            description: "AI pair programming in your terminal".to_string(),
            category: "Agent".to_string(),
            default_args: vec![],
        },
        AgentDefinition {
            id: "gemini".to_string(),
            name: "Gemini CLI".to_string(),
            binary: "gemini".to_string(),
            description: "Google Gemini command line coding agent".to_string(),
            category: "Agent".to_string(),
            default_args: vec![],
        },
        AgentDefinition {
            id: "cursor-agent".to_string(),
            name: "Cursor Agent".to_string(),
            binary: "cursor-agent".to_string(),
            description: "Headless Cursor agent CLI for automated changes".to_string(),
            category: "Agent".to_string(),
            default_args: vec![],
        },
        AgentDefinition {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            binary: "codex".to_string(),
            description: "OpenAI Codex CLI harness".to_string(),
            category: "Agent".to_string(),
            default_args: vec![],
        },
        AgentDefinition {
            id: "deepseek".to_string(),
            name: "DeepSeek Harness".to_string(),
            binary: "deepseek".to_string(),
            description: "DeepSeek coder terminal harness".to_string(),
            category: "Agent".to_string(),
            default_args: vec![],
        },
        AgentDefinition {
            id: "grok".to_string(),
            name: "Grok Build".to_string(),
            binary: "grok".to_string(),
            description: "xAI Grok build and reasoning agent".to_string(),
            category: "Agent".to_string(),
            default_args: vec![],
        },
        AgentDefinition {
            id: "shell".to_string(),
            name: if cfg!(target_os = "windows") { "PowerShell" } else { "Bash" }.to_string(),
            binary: shell_binary,
            description: "Interactive system terminal shell".to_string(),
            category: "Shell".to_string(),
            default_args: vec![],
        },
    ]
}

/// Resolves the absolute path to a binary in PATH.
/// Tries `which::which` first, then falls back to `where.exe` on Windows.
pub fn resolve_binary_path(binary: &str) -> Option<String> {
    if let Ok(path) = which::which(binary) {
        return Some(path.to_string_lossy().to_string());
    }

    // Windows fallback using where.exe
    if cfg!(target_os = "windows") {
        if let Ok(output) = Command::new("where.exe").arg(binary).output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(first_line) = stdout.lines().next() {
                    let trimmed = first_line.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }

        // Also try common Windows extensions if binary didn't have one
        if !binary.contains('.') {
            for ext in &[".exe", ".cmd", ".bat", ".ps1"] {
                let with_ext = format!("{}{}", binary, ext);
                if let Ok(path) = which::which(&with_ext) {
                    return Some(path.to_string_lossy().to_string());
                }
            }
        }
    }

    None
}

/// Detects all supported agents on the host system.
pub fn detect_agents() -> Vec<DetectedAgent> {
    let supported = get_supported_agents();
    let mut detected = Vec::with_capacity(supported.len());

    for agent in supported {
        let binary_path = resolve_binary_path(&agent.binary);
        let is_installed = binary_path.is_some();

        info!(
            agent_id = %agent.id,
            name = %agent.name,
            installed = is_installed,
            path = ?binary_path,
            "Detected agent status"
        );

        detected.push(DetectedAgent {
            id: agent.id,
            name: agent.name,
            binary: agent.binary,
            description: agent.description,
            category: agent.category,
            is_installed,
            binary_path,
        });
    }

    detected
}
