use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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
    pub install_cmd: Option<String>,
    pub update_cmd: Option<String>,
    pub uninstall_cmd: Option<String>,
    pub manage_hint: Option<String>,
    pub docs_url: Option<String>,
}

fn documented(
    install: &'static str,
    update: Option<&'static str>,
    uninstall: Option<&'static str>,
) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    (
        Some(install.to_string()),
        update.map(str::to_string),
        uninstall.map(str::to_string),
        None,
    )
}

fn documented_hint(
    install: Option<&'static str>,
    update: Option<&'static str>,
    uninstall: Option<&'static str>,
    hint: &'static str,
) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    (
        install.map(str::to_string),
        update.map(str::to_string),
        uninstall.map(str::to_string),
        Some(hint.to_string()),
    )
}

/// Official install / update / uninstall from each project's docs, for this OS.
/// Commands are copied from vendor docs — not inferred from a package-manager guess.
pub fn agent_lifecycle(id: &str) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    let win = cfg!(target_os = "windows");
    match id {
        // https://code.claude.com/docs/en/install
        "claude" => {
            if win {
                documented(
                    "irm https://claude.ai/install.ps1 | iex",
                    Some("irm https://claude.ai/install.ps1 | iex"),
                    Some(r#"Remove-Item -Path "$env:USERPROFILE\.local\bin\claude.exe" -Force; Remove-Item -Path "$env:USERPROFILE\.local\share\claude" -Recurse -Force"#),
                )
            } else {
                documented(
                    "curl -fsSL https://claude.ai/install.sh | bash",
                    Some("curl -fsSL https://claude.ai/install.sh | bash"),
                    Some("rm -f ~/.local/bin/claude; rm -rf ~/.local/share/claude"),
                )
            }
        }
        // https://github.com/anomalyco/opencode
        "opencode" => {
            if win {
                documented(
                    "npm i -g opencode-ai@latest",
                    Some("npm i -g opencode-ai@latest"),
                    Some("npm uninstall -g opencode-ai"),
                )
            } else {
                documented(
                    "curl -fsSL https://opencode.ai/install | bash",
                    Some("curl -fsSL https://opencode.ai/install | bash"),
                    Some("opencode uninstall --force"),
                )
            }
        }
        // https://docs.openclaw.ai/install  +  https://docs.openclaw.ai/install/uninstall
        "openclaw" => {
            if win {
                documented(
                    "iwr -useb https://openclaw.ai/install.ps1 | iex",
                    Some("iwr -useb https://openclaw.ai/install.ps1 | iex"),
                    Some("openclaw uninstall --all --yes --non-interactive; npm rm -g openclaw"),
                )
            } else {
                documented(
                    "curl -fsSL https://openclaw.ai/install.sh | bash",
                    Some("curl -fsSL https://openclaw.ai/install.sh | bash"),
                    Some("openclaw uninstall --all --yes --non-interactive; npm rm -g openclaw"),
                )
            }
        }
        // https://github.com/earendil-works/pi  — package moved off @mariozechner
        "pi" => documented(
            "npm install -g --ignore-scripts @earendil-works/pi-coding-agent",
            Some("pi update --self"),
            Some("npm uninstall -g @earendil-works/pi-coding-agent"),
        ),
        // https://hermes-agent.nousresearch.com/docs/getting-started/installation
        // https://hermes-agent.nousresearch.com/docs/getting-started/updating
        "hermes" => {
            if win {
                documented(
                    "iex (irm https://hermes-agent.nousresearch.com/install.ps1)",
                    Some("hermes update"),
                    Some("hermes uninstall"),
                )
            } else {
                documented(
                    "curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash",
                    Some("hermes update"),
                    Some("hermes uninstall"),
                )
            }
        }
        // https://aider.chat/docs/install.html
        // Install is aider-install → `uv tool install … aider-chat`. Community + uv docs:
        // `uv tool uninstall aider-chat` removes the tool env and ~/.local/bin/aider shim.
        "aider" => {
            if win {
                documented(
                    "python -m pip install aider-install; aider-install",
                    Some("aider --upgrade"),
                    Some(
                        r#"python -m uv tool uninstall aider-chat; if ($LASTEXITCODE -ne 0) { uv tool uninstall aider-chat }; python -m pip uninstall -y aider-install; Remove-Item -Force -ErrorAction SilentlyContinue "$env:USERPROFILE\.local\bin\aider.exe","$env:USERPROFILE\.local\bin\aider""#,
                    ),
                )
            } else {
                documented(
                    "python -m pip install aider-install; aider-install",
                    Some("aider --upgrade"),
                    Some(
                        "(python -m uv tool uninstall aider-chat || uv tool uninstall aider-chat || true); python -m pip uninstall -y aider-install; rm -f ~/.local/bin/aider",
                    ),
                )
            }
        }
        // https://www.npmjs.com/package/@google/gemini-cli
        // https://github.com/google-gemini/gemini-cli/blob/HEAD/docs/resources/uninstall.md
        "gemini" => documented(
            "npm install -g @google/gemini-cli",
            Some("npm install -g @google/gemini-cli@latest"),
            Some("npm uninstall -g @google/gemini-cli"),
        ),
        // https://antigravity.google/docs/cli/install/
        "agy" => {
            if win {
                documented(
                    "irm https://antigravity.google/cli/install.ps1 | iex",
                    Some("irm https://antigravity.google/cli/install.ps1 | iex"),
                    Some(r#"Remove-Item -Path "$env:LOCALAPPDATA\agy" -Recurse -Force"#),
                )
            } else {
                documented(
                    "curl -fsSL https://antigravity.google/cli/install.sh | bash",
                    Some("curl -fsSL https://antigravity.google/cli/install.sh | bash"),
                    Some("rm -f ~/.local/bin/agy"),
                )
            }
        }
        // https://cursor.com/docs/cli/installation
        // Official docs omit uninstall; install lands under LOCALAPPDATA\cursor-agent (Windows)
        // or ~/.local/share/cursor-agent + ~/.local/bin/agent (Unix). Remove those only — not ~/.cursor IDE config.
        "cursor-agent" => {
            if win {
                documented(
                    "irm 'https://cursor.com/install?win32=true' | iex",
                    Some("agent update"),
                    Some(
                        r#"Remove-Item -Path "$env:LOCALAPPDATA\cursor-agent" -Recurse -Force -ErrorAction SilentlyContinue; Remove-Item -Force -ErrorAction SilentlyContinue "$env:USERPROFILE\.local\bin\agent.exe","$env:USERPROFILE\.local\bin\agent.cmd","$env:USERPROFILE\.local\bin\cursor-agent.exe","$env:USERPROFILE\.local\bin\cursor-agent.cmd""#,
                    ),
                )
            } else {
                documented(
                    "curl https://cursor.com/install -fsS | bash",
                    Some("agent update"),
                    Some(
                        "rm -rf ~/.local/share/cursor-agent; rm -f ~/.local/bin/agent ~/.local/bin/cursor-agent",
                    ),
                )
            }
        }
        // https://developers.openai.com/codex/ — standalone CLI install; no official uninstall.
        // CLI bin: Windows %LOCALAPPDATA%\Programs\OpenAI\Codex\bin ; macOS/Linux ~/.local/bin/codex
        // Do not delete ~/.codex (shared auth/history) or the Codex desktop app.
        "codex" => {
            if win {
                documented_hint(
                    Some(
                        r#"powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 | iex""#,
                    ),
                    Some(
                        r#"$env:CODEX_NON_INTERACTIVE=1; powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 | iex""#,
                    ),
                    Some(
                        r#"npm uninstall -g @openai/codex 2>$null; Remove-Item -Path "$env:LOCALAPPDATA\Programs\OpenAI\Codex" -Recurse -Force -ErrorAction SilentlyContinue; Remove-Item -Force -ErrorAction SilentlyContinue "$env:USERPROFILE\.local\bin\codex.exe","$env:USERPROFILE\.local\bin\codex""#,
                    ),
                    "Removes the Codex CLI only — keeps ~/.codex auth/history and the Codex desktop app",
                )
            } else {
                documented_hint(
                    Some("curl -fsSL https://chatgpt.com/codex/install.sh | sh"),
                    Some("curl -fsSL https://chatgpt.com/codex/install.sh | CODEX_NON_INTERACTIVE=1 sh"),
                    Some(
                        "npm uninstall -g @openai/codex 2>/dev/null; rm -f ~/.local/bin/codex /usr/local/bin/codex",
                    ),
                    "Removes the Codex CLI only — keeps ~/.codex auth/history and the Codex desktop/app casks",
                )
            }
        }
        // https://ampcode.com/manual — install only; binary lands in ~/.amp/bin (Windows/macOS).
        // No vendor uninstall; remove the install tree + optional npm globals.
        "amp" => {
            if win {
                documented_hint(
                    Some(r#"powershell -c "irm https://ampcode.com/install.ps1 | iex""#),
                    Some(r#"powershell -c "irm https://ampcode.com/install.ps1 | iex""#),
                    Some(
                        r#"npm uninstall -g @ampcode/cli,@sourcegraph/amp 2>$null; Remove-Item -Path "$env:USERPROFILE\.amp" -Recurse -Force -ErrorAction SilentlyContinue"#,
                    ),
                    "Amp docs do not publish uninstall; Remove deletes ~/.amp (CLI + local data) and npm globals if present",
                )
            } else {
                documented_hint(
                    Some("curl -fsSL https://ampcode.com/install.sh | bash"),
                    Some("curl -fsSL https://ampcode.com/install.sh | bash"),
                    Some(
                        "npm uninstall -g @ampcode/cli @sourcegraph/amp 2>/dev/null; rm -rf ~/.amp ~/.config/amp",
                    ),
                    "Amp docs do not publish uninstall; Remove deletes ~/.amp (and ~/.config/amp) plus npm globals if present",
                )
            }
        }
        // https://github.com/block/goose/blob/main/documentation/docs/getting-started/installation.md
        "goose" => {
            if win {
                documented(
                    "irm https://github.com/block/goose/raw/main/download_cli.ps1 | iex",
                    Some("goose update"),
                    Some(r#"Remove-Item -Path "$env:APPDATA\Block\goose" -Recurse -Force -ErrorAction SilentlyContinue; Remove-Item -Path "$env:LOCALAPPDATA\Block\goose" -Recurse -Force -ErrorAction SilentlyContinue"#),
                )
            } else {
                documented(
                    "curl -fsSL https://github.com/block/goose/releases/download/stable/download_cli.sh | bash",
                    Some("goose update"),
                    Some("rm -rf ~/.config/goose ~/.local/share/goose ~/.local/state/goose"),
                )
            }
        }
        // https://github.com/cline/cline/blob/main/docs/cline-cli/installation.mdx
        "cline" => documented(
            "npm install -g cline",
            Some("npm install -g cline@latest"),
            Some("npm uninstall -g cline"),
        ),
        // https://github.com/charmbracelet/crush — winget / brew / scoop / go
        "crush" => {
            if win {
                documented(
                    "winget install --id charmbracelet.crush -e",
                    Some("winget upgrade --id charmbracelet.crush -e"),
                    Some("winget uninstall --id charmbracelet.crush -e"),
                )
            } else if cfg!(target_os = "macos") {
                documented(
                    "brew install charmbracelet/tap/crush",
                    Some("brew upgrade crush"),
                    Some("brew uninstall crush"),
                )
            } else {
                // Linux: Charm apt/yum repos or go — brew may still work via linuxbrew.
                documented_hint(
                    Some("brew install charmbracelet/tap/crush"),
                    Some("brew upgrade crush"),
                    Some("brew uninstall crush"),
                    "Or install from https://github.com/charmbracelet/crush#installation (apt/yum/go)",
                )
            }
        }
        // https://qwenlm.github.io/qwen-code-docs/en/users/quickstart/
        // https://github.com/QwenLM/qwen-code/blob/main/scripts/installation/INSTALLATION_GUIDE.md
        "qwen" => {
            if win {
                documented(
                    "irm https://qwen-code-assets.oss-cn-hangzhou.aliyuncs.com/installation/install-qwen-standalone.ps1 | iex",
                    Some("irm https://qwen-code-assets.oss-cn-hangzhou.aliyuncs.com/installation/install-qwen-standalone.ps1 | iex"),
                    Some(r#"powershell -ExecutionPolicy Bypass -c "irm https://qwen-code-assets.oss-cn-hangzhou.aliyuncs.com/installation/uninstall-qwen-standalone.ps1 | iex""#),
                )
            } else {
                documented(
                    "curl -fsSL https://qwen-code-assets.oss-cn-hangzhou.aliyuncs.com/installation/install-qwen-standalone.sh | bash",
                    Some("curl -fsSL https://qwen-code-assets.oss-cn-hangzhou.aliyuncs.com/installation/install-qwen-standalone.sh | bash"),
                    Some("curl -fsSL https://qwen-code-assets.oss-cn-hangzhou.aliyuncs.com/installation/uninstall-qwen-standalone.sh | bash"),
                )
            }
        }
        // https://www.kimi.com/code/docs/en/kimi-code-cli/guides/getting-started.html
        "kimi" => {
            if win {
                documented(
                    "irm https://code.kimi.com/kimi-code/install.ps1 | iex",
                    Some("kimi upgrade"),
                    Some("npm uninstall -g @moonshot-ai/kimi-code"),
                )
            } else {
                documented(
                    "curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash",
                    Some("kimi upgrade"),
                    Some("npm uninstall -g @moonshot-ai/kimi-code"),
                )
            }
        }
        // https://docs.plandex.ai/install
        "plandex" => {
            if win {
                documented_hint(
                    None,
                    None,
                    None,
                    "Plandex docs: Windows is supported via WSL only, not PowerShell or CMD",
                )
            } else {
                documented_hint(
                    Some("curl -sL https://plandex.ai/install.sh | bash"),
                    Some("curl -sL https://plandex.ai/install.sh | bash"),
                    Some("sudo rm -f /usr/local/bin/plandex; rm -rf ~/.plandex-home"),
                    "Uninstall is maintainer-documented: remove the binary and ~/.plandex-home",
                )
            }
        }
        // https://docs.openhands.dev/openhands/usage/cli/installation
        // Mirror of vibe: uv tool install ↔ uv tool uninstall (uv CLI docs).
        "openhands" => {
            if win {
                documented(
                    "uv tool install openhands --python 3.12",
                    Some("uv tool upgrade openhands --python 3.12"),
                    Some(
                        r#"python -m uv tool uninstall openhands; if ($LASTEXITCODE -ne 0) { uv tool uninstall openhands }; Remove-Item -Force -ErrorAction SilentlyContinue "$env:USERPROFILE\.local\bin\openhands.exe","$env:USERPROFILE\.local\bin\openhands""#,
                    ),
                )
            } else {
                documented(
                    "uv tool install openhands --python 3.12",
                    Some("uv tool upgrade openhands --python 3.12"),
                    Some(
                        "(python -m uv tool uninstall openhands || uv tool uninstall openhands || true); rm -f ~/.local/bin/openhands",
                    ),
                )
            }
        }
        // https://www.openinterpreter.com/docs/terminal/install — Uninstalling section
        "interpreter" => {
            if win {
                documented(
                    "irm https://www.openinterpreter.com/install.ps1 | iex",
                    Some("interpreter update"),
                    Some(
                        r#"$binDir = Join-Path $env:LOCALAPPDATA 'Programs\Open Interpreter\bin'; $interpreterHome = Join-Path $env:USERPROFILE '.openinterpreter'; $standaloneRoot = Join-Path $interpreterHome 'packages\standalone'; if (Test-Path -LiteralPath $binDir) { $binItem = Get-Item -LiteralPath $binDir -Force; $binTarget = [string]$binItem.Target; $isManagedJunction = ($binItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -and $binTarget.StartsWith($standaloneRoot, [StringComparison]::OrdinalIgnoreCase); if ($isManagedJunction) { Remove-Item -LiteralPath $binDir -Recurse -Force } }; Remove-Item -LiteralPath $standaloneRoot -Recurse -Force -ErrorAction SilentlyContinue; $userPath = [Environment]::GetEnvironmentVariable('Path', 'User'); if (-not [string]::IsNullOrWhiteSpace($userPath)) { $nextPath = ($userPath -split ';' | Where-Object { -not [string]::Equals($_.TrimEnd('\'), $binDir.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase) }) -join ';'; [Environment]::SetEnvironmentVariable('Path', $nextPath, 'User') }"#,
                    ),
                )
            } else {
                documented(
                    "curl -fsSL https://www.openinterpreter.com/install | sh",
                    Some("interpreter update"),
                    Some(
                        r#"for name in interpreter i codex-code-mode-host; do path="$HOME/.local/bin/$name"; case "$(readlink "$path" 2>/dev/null || true)" in "$HOME/.openinterpreter/packages/standalone/"*) rm -f "$path" ;; esac; done; rm -rf "$HOME/.openinterpreter/packages/standalone""#,
                    ),
                )
            }
        }
        // https://www.npmjs.com/package/@continuedev/cli
        "continue" => {
            if win {
                documented(
                    "irm https://raw.githubusercontent.com/continuedev/continue/main/extensions/cli/scripts/install.ps1 | iex",
                    Some("npm i -g @continuedev/cli"),
                    Some("npm uninstall -g @continuedev/cli"),
                )
            } else {
                documented(
                    "curl -fsSL https://raw.githubusercontent.com/continuedev/continue/main/extensions/cli/scripts/install.sh | bash",
                    Some("npm i -g @continuedev/cli"),
                    Some("npm uninstall -g @continuedev/cli"),
                )
            }
        }
        // https://kilo.ai/docs/code-with-ai/platforms/cli
        "kilo" => documented(
            "npm install -g @kilocode/cli",
            Some("kilo upgrade"),
            Some("kilo uninstall --force"),
        ),
        // https://docs.mistral.ai/getting-started/quickstarts/vibe-code/install-cli
        "vibe" => {
            if win {
                documented(
                    "uv tool install mistral-vibe",
                    Some("uv tool upgrade mistral-vibe"),
                    Some("uv tool uninstall mistral-vibe"),
                )
            } else {
                documented(
                    "curl -LsSf https://mistral.ai/vibe/install.sh | bash",
                    Some("uv tool upgrade mistral-vibe"),
                    Some("uv tool uninstall mistral-vibe"),
                )
            }
        }
        // https://kiro.dev/docs/cli/ — Amazon Q Developer CLI rebranded to Kiro
        "kiro" => {
            if win {
                documented_hint(
                    Some("irm 'https://cli.kiro.dev/install.ps1' | iex"),
                    Some("kiro-cli update --non-interactive"),
                    Some("kiro-cli uninstall"),
                    "Windows Defender may flag the official irm|iex installer; allow it or run the same command in an elevated PowerShell outside TermCrew",
                )
            } else {
                documented(
                    "curl -fsSL https://cli.kiro.dev/install | bash",
                    Some("kiro-cli update --non-interactive"),
                    Some("kiro-cli uninstall"),
                )
            }
        }
        // https://forgecode.dev/docs/ — curl installer → ~/.forge/bin (macOS/Linux/Git Bash on Windows)
        "forge" => {
            if win {
                documented_hint(
                    Some(
                        r#"$bash = @("$env:ProgramFiles\Git\bin\bash.exe", "${env:ProgramFiles(x86)}\Git\bin\bash.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1; if (-not $bash) { throw 'Git for Windows (bash) is required to install ForgeCode' }; & $bash -lc 'curl -fsSL https://forgecode.dev/cli | sh'"#,
                    ),
                    Some("forge update --no-confirm"),
                    Some(
                        r#"Remove-Item -Path "$env:USERPROFILE\.forge" -Recurse -Force -ErrorAction SilentlyContinue; Remove-Item -Path "$env:LOCALAPPDATA\Programs\Forge" -Recurse -Force -ErrorAction SilentlyContinue"#,
                    ),
                    "Official Windows path is Git Bash (or WSL). Remove deletes ~/.forge and %LOCALAPPDATA%\\Programs\\Forge",
                )
            } else {
                documented_hint(
                    Some("curl -fsSL https://forgecode.dev/cli | sh"),
                    Some("forge update --no-confirm"),
                    Some("rm -rf ~/.forge"),
                    "ForgeCode docs omit uninstall; Remove deletes ~/.forge install tree",
                )
            }
        }
        // https://gptme.org/docs/getting-started.html
        "gptme" => documented_hint(
            Some("pipx install gptme"),
            Some("pipx upgrade gptme"),
            Some("pipx uninstall gptme"),
            "gptme docs recommend pipx install gptme (or uv tool install gptme)",
        ),
        // https://github.com/Hmbown/CodeWhale/blob/main/docs/INSTALL.md
        // npm recommended; Windows NSIS → %LOCALAPPDATA%\Programs\CodeWhale\bin; shell install → ~/.local/bin
        "codewhale" => {
            if win {
                documented_hint(
                    Some("npm install -g codewhale"),
                    Some("npm update -g codewhale"),
                    Some(
                        r#"npm uninstall -g codewhale 2>$null; if (Test-Path "$env:LOCALAPPDATA\Programs\CodeWhale\Uninstall.exe") { Start-Process -FilePath "$env:LOCALAPPDATA\Programs\CodeWhale\Uninstall.exe" -ArgumentList '/S' -Wait } else { Remove-Item -Path "$env:LOCALAPPDATA\Programs\CodeWhale" -Recurse -Force -ErrorAction SilentlyContinue }; Remove-Item -Force -ErrorAction SilentlyContinue "$env:USERPROFILE\bin\codewhale.exe","$env:USERPROFILE\bin\codew.exe","$env:USERPROFILE\bin\codewhale-tui.exe","$env:USERPROFILE\bin\codewhale.bat""#,
                    ),
                    "Removes npm global and/or NSIS CLI under Programs\\CodeWhale — not %APPDATA%\\codewhale config",
                )
            } else {
                documented(
                    "npm install -g codewhale",
                    Some("npm update -g codewhale"),
                    Some(
                        "npm uninstall -g codewhale; rm -f ~/.local/bin/codewhale ~/.local/bin/codew ~/.local/bin/codewhale-tui",
                    ),
                )
            }
        }
        // https://github.com/deepseek-ai/deepseek-harness — npm package @deepseek-ai/dsh, binary dsh
        "deepseek" => documented(
            "npm install -g @deepseek-ai/dsh",
            Some("npm install -g @deepseek-ai/dsh@latest"),
            Some("npm uninstall -g @deepseek-ai/dsh"),
        ),
        // https://docs.x.ai/build/overview
        "grok" => {
            if win {
                documented_hint(
                    Some("irm https://x.ai/cli/install.ps1 | iex"),
                    Some("irm https://x.ai/cli/install.ps1 | iex"),
                    None,
                    "xAI docs publish irm https://x.ai/cli/install.ps1 | iex. npm grok-cli is not official",
                )
            } else {
                documented_hint(
                    Some("curl -fsSL https://x.ai/cli/install.sh | bash"),
                    Some("curl -fsSL https://x.ai/cli/install.sh | bash"),
                    None,
                    "xAI docs publish curl -fsSL https://x.ai/cli/install.sh | bash. npm grok-cli is not official",
                )
            }
        }
        "shell" | "cmd" | "bash" | "zsh" | "sh" => {
            documented_hint(None, None, None, "Built into the system")
        }
        "git-bash" => documented_hint(
            None,
            None,
            None,
            "Install Git for Windows from https://git-scm.com/downloads/win",
        ),
        "wsl" => documented_hint(
            None,
            None,
            None,
            "Install WSL from https://learn.microsoft.com/windows/wsl/install",
        ),
        _ => documented_hint(None, None, None, "No official installer is recorded for this agent"),
    }
}

/// Official vendor docs for this agent — the page a human should open, not a blog or mirror.
pub fn agent_docs_url(id: &str) -> Option<&'static str> {
    Some(match id {
        "claude" => "https://code.claude.com/docs/en/overview",
        "opencode" => "https://opencode.ai/docs",
        "openclaw" => "https://docs.openclaw.ai",
        "pi" => "https://github.com/badlogic/pi-mono",
        "hermes" => "https://hermes-agent.nousresearch.com/docs/getting-started/installation",
        "aider" => "https://aider.chat/docs/install.html",
        "gemini" => "https://github.com/google-gemini/gemini-cli",
        "agy" => "https://antigravity.google/docs/cli/install/",
        "cursor-agent" => "https://cursor.com/docs/cli/overview",
        "codex" => "https://developers.openai.com/codex/cli",
        "amp" => "https://ampcode.com/manual",
        "goose" => "https://block.github.io/goose/docs/getting-started/installation",
        "cline" => "https://docs.cline.bot/cline-cli/installation",
        "crush" => "https://github.com/charmbracelet/crush",
        "qwen" => "https://qwenlm.github.io/qwen-code-docs/en/users/quickstart/",
        "kimi" => "https://www.kimi.com/code/docs/en/kimi-code-cli/guides/getting-started.html",
        "plandex" => "https://docs.plandex.ai/install",
        "openhands" => "https://docs.openhands.dev/openhands/usage/cli/installation",
        "interpreter" => "https://docs.openinterpreter.com",
        "continue" => "https://docs.continue.dev/cli/quickstart",
        "kilo" => "https://kilo.ai/docs/code-with-ai/platforms/cli",
        "vibe" => "https://docs.mistral.ai/getting-started/quickstarts/vibe-code/install-cli",
        "kiro" => "https://kiro.dev/docs/cli",
        "forge" => "https://forgecode.dev/docs",
        "gptme" => "https://gptme.org/docs/getting-started.html",
        "codewhale" => "https://github.com/Hmbown/CodeWhale",
        "deepseek" => "https://deepseek-harness.github.io/deepseek-harness/",
        "grok" => "https://docs.x.ai/docs",
        "shell" => {
            if cfg!(target_os = "windows") {
                "https://learn.microsoft.com/powershell/scripting/overview"
            } else if cfg!(target_os = "macos") {
                "https://zsh.sourceforge.io/Doc/"
            } else {
                "https://www.gnu.org/software/bash/manual"
            }
        }
        "cmd" => "https://learn.microsoft.com/windows-server/administration/windows-commands/cmd",
        "bash" => "https://www.gnu.org/software/bash/manual",
        "git-bash" => "https://git-scm.com/downloads/win",
        "zsh" => "https://zsh.sourceforge.io/Doc/",
        "sh" => "https://pubs.opengroup.org/onlinepubs/9699919799/utilities/sh.html",
        "wsl" => "https://learn.microsoft.com/windows/wsl/install",
        _ => return None,
    })
}

/// Extra PATH names to try when the primary binary is a renamed/successor CLI.
fn alternate_binaries(id: &str) -> &'static [&'static str] {
    match id {
        "cursor-agent" => &["agent", "cursor-agent"],
        "kiro" => &["kiro-cli", "q", "kiro"],
        "codewhale" => &["codewhale", "codew", "codewhale-tui"],
        "kimi" => &["kimi"],
        "deepseek" => &["deepseek"],
        _ => &[],
    }
}

fn agent(id: &str, name: &str, binary: &str, description: &str) -> AgentDefinition {
    AgentDefinition {
        id: id.to_string(),
        name: name.to_string(),
        binary: binary.to_string(),
        description: description.to_string(),
        category: "Agent".to_string(),
        default_args: vec![],
    }
}

fn shell(id: &str, name: &str, binary: &str, description: &str, default_args: Vec<String>) -> AgentDefinition {
    AgentDefinition {
        id: id.to_string(),
        name: name.to_string(),
        binary: binary.to_string(),
        description: description.to_string(),
        category: "Shell".to_string(),
        default_args,
    }
}

pub fn is_system_shell(id: &str) -> bool {
    matches!(
        id.to_ascii_lowercase().as_str(),
        "shell" | "cmd" | "git-bash" | "wsl" | "bash" | "zsh" | "sh"
    )
}

/// Returns the predefined list of supported agents and shells.
pub fn get_supported_agents() -> Vec<AgentDefinition> {
    let (default_shell_name, default_shell_bin) = if cfg!(target_os = "windows") {
        ("PowerShell", "powershell.exe")
    } else if cfg!(target_os = "macos") {
        ("Zsh", "zsh")
    } else {
        ("Bash", "bash")
    };

    let mut list = vec![
        AgentDefinition {
            id: "claude".to_string(),
            name: "Claude Code".to_string(),
            binary: "claude".to_string(),
            description: "Anthropic's agentic CLI for coding and reasoning".to_string(),
            category: "Agent".to_string(),
            default_args: vec![
                "--settings".to_string(),
                r#"{"theme":"auto"}"#.to_string(),
            ],
        },
        agent("opencode", "OpenCode", "opencode", "Terminal-native coding agent (anomalyco/opencode)"),
        agent("openclaw", "OpenClaw", "openclaw", "Local personal AI assistant CLI with skills and channels"),
        agent("pi", "Pi", "pi", "Minimal terminal coding harness from pi-mono"),
        agent("hermes", "Hermes Agent", "hermes", "Nous Research self-improving CLI agent"),
        agent("aider", "Aider", "aider", "Git-native AI pair programmer"),
        agent("gemini", "Gemini CLI", "gemini", "Google Gemini command line coding agent"),
        agent("agy", "Antigravity CLI", "agy", "Google Antigravity terminal agent (agy)"),
        agent("cursor-agent", "Cursor Agent", "agent", "Cursor CLI (official binary is agent)"),
        agent("codex", "Codex", "codex", "OpenAI Codex CLI harness"),
        agent("amp", "Amp", "amp", "Sourcegraph Amp terminal coding agent"),
        agent("goose", "Goose", "goose", "On-device extensible agent (Linux Foundation / Block)"),
        agent("cline", "Cline CLI", "cline", "Model-agnostic autonomous coding agent"),
        agent("crush", "Crush", "crush", "Charmbracelet agentic coding TUI"),
        agent("qwen", "Qwen Code", "qwen", "Alibaba Qwen official CLI coding agent"),
        agent("kimi", "Kimi Code", "kimi", "Moonshot AI Kimi Code CLI"),
        agent("plandex", "Plandex", "plandex", "Plan-first multi-file CLI coding agent"),
        agent("openhands", "OpenHands", "openhands", "OpenHands agentic developer CLI"),
        agent("interpreter", "Open Interpreter", "interpreter", "Local code-executing terminal agent"),
        agent("continue", "Continue CLI", "cn", "Continue.dev terminal coding CLI (cn)"),
        agent("kilo", "Kilo Code", "kilo", "Kilo Code agentic engineering CLI"),
        agent("vibe", "Mistral Vibe", "vibe", "Mistral CLI coding assistant"),
        agent("kiro", "Kiro", "kiro-cli", "AWS Kiro coding CLI (formerly Amazon Q)"),
        agent("forge", "ForgeCode", "forge", "Multi-model AI pair programmer CLI"),
        agent("gptme", "gptme", "gptme", "Persistent terminal AI agent (gptme)"),
        agent("codewhale", "Codewhale", "codewhale", "Rust TUI coding agent (BYO model)"),
        AgentDefinition {
            id: "deepseek".to_string(),
            name: "DeepSeek Harness".to_string(),
            binary: "dsh".to_string(),
            description: "DeepSeek AI open-source agent harness (dsh)".to_string(),
            category: "Agent".to_string(),
            default_args: vec!["web".to_string()],
        },
        agent("grok", "Grok Build", "grok", "xAI Grok official coding agent TUI"),
        shell(
            "shell",
            default_shell_name,
            default_shell_bin,
            "Default system terminal shell",
            vec![],
        ),
    ];

    if cfg!(target_os = "windows") {
        list.push(shell(
            "cmd",
            "Command Prompt",
            "cmd.exe",
            "Windows Command Prompt",
            vec![],
        ));
        list.push(shell(
            "git-bash",
            "Git Bash",
            "bash.exe",
            "Git for Windows bash",
            vec!["--login".to_string(), "-i".to_string()],
        ));
        list.push(shell(
            "wsl",
            "WSL",
            "wsl.exe",
            "Windows Subsystem for Linux",
            vec![],
        ));
    } else {
        if default_shell_bin != "bash" {
            list.push(shell("bash", "Bash", "bash", "Bourne-again shell", vec![]));
        }
        if default_shell_bin != "zsh" {
            list.push(shell("zsh", "Zsh", "zsh", "Z shell", vec![]));
        }
        list.push(shell("sh", "sh", "sh", "POSIX shell", vec![]));
    }

    list.sort_by(|a, b| match (a.category.as_str(), b.category.as_str()) {
        ("Shell", "Agent") => std::cmp::Ordering::Greater,
        ("Agent", "Shell") => std::cmp::Ordering::Less,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    list
}

/// Shared PTY environment so every registry agent sees a truecolor xterm
/// (same Auto / rich syntax behavior as a normal terminal host).
pub fn pty_terminal_env() -> Vec<(&'static str, &'static str)> {
    vec![
        ("TERM", "xterm-256color"),
        ("COLORTERM", "truecolor"),
        ("FORCE_COLOR", "3"),
    ]
}

/// True when this engine shows a first-run theme / style picker that should be
/// confirmed with Enter (option 1 / Auto).
pub fn needs_theme_auto_confirm(engine_id: &str) -> bool {
    matches!(
        engine_id.to_ascii_lowercase().as_str(),
        "claude"
            | "gemini"
            | "agy"
            | "opencode"
            | "openclaw"
            | "pi"
            | "hermes"
            | "codex"
            | "aider"
            | "cursor-agent"
            | "amp"
            | "goose"
            | "cline"
            | "crush"
            | "qwen"
            | "kimi"
            | "plandex"
            | "kilo"
            | "vibe"
            | "deepseek"
            | "grok"
            | "codewhale"
    )
}

/// User-local folders installers add after this process started (stale PATH).
fn extra_bin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        dirs.push(local.join("agy"));
        dirs.push(local.join("agy").join("bin"));
        // Official Cursor Agent Windows installer (`irm …/install?win32=true | iex`)
        // drops agent.cmd / cursor-agent.cmd here and appends this dir to user PATH.
        dirs.push(local.join("cursor-agent"));
        dirs.push(local.join("Programs"));
        // Codex Windows installer (`irm https://chatgpt.com/codex/install.ps1 | iex`)
        dirs.push(local.join("Programs").join("OpenAI").join("Codex").join("bin"));
        // CodeWhale NSIS installer → %LOCALAPPDATA%\Programs\CodeWhale\bin
        dirs.push(local.join("Programs").join("CodeWhale").join("bin"));
        // ForgeCode Windows curl|sh installer → %LOCALAPPDATA%\Programs\Forge
        dirs.push(local.join("Programs").join("Forge"));
        // WinGet shims + portable package bins (e.g. charmbracelet.crush_…\crush_*\crush.exe)
        let winget = local.join("Microsoft").join("WinGet");
        dirs.push(winget.join("Links"));
        let packages = winget.join("Packages");
        if let Ok(pkgs) = std::fs::read_dir(&packages) {
            for pkg in pkgs.flatten() {
                let Ok(entries) = std::fs::read_dir(pkg.path()) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        dirs.push(p);
                    }
                }
            }
        }
    }
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".local").join("share").join("cursor-agent"));
        // Amp Windows installer (`irm https://ampcode.com/install.ps1 | iex`)
        dirs.push(home.join(".amp").join("bin"));
        dirs.push(home.join(".bun").join("bin"));
        // ForgeCode curl installer → ~/.forge/bin
        dirs.push(home.join(".forge").join("bin"));
        dirs.push(home.join("go").join("bin"));
        dirs.push(home.join("scoop").join("shims"));
        // CodeWhale Windows zip `install.bat` copies to %USERPROFILE%\bin
        dirs.push(home.join("bin"));
    }
    if let Ok(roaming) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(roaming).join("npm"));
    }
    if cfg!(target_os = "windows") {
        if let Ok(pf) = std::env::var("ProgramFiles") {
            let pf = PathBuf::from(pf);
            dirs.push(pf.join("Git").join("bin"));
            dirs.push(pf.join("Git").join("usr").join("bin"));
            dirs.push(pf.join("PowerShell").join("7"));
        }
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            let pf86 = PathBuf::from(pf86);
            dirs.push(pf86.join("Git").join("bin"));
            dirs.push(pf86.join("Git").join("usr").join("bin"));
        }
    } else {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/opt/local/bin"));
    }
    dirs.into_iter().filter(|d| d.is_dir()).collect()
}

/// On Windows, npm global CLIs ship a Unix `#!` shim (`pi`) next to `pi.cmd`.
/// CreateProcess cannot run the shim (os error 193) — prefer a spawnable sibling.
fn prefer_createprocess_path(path: PathBuf) -> Option<String> {
    #[cfg(not(windows))]
    {
        return Some(path.to_string_lossy().into_owned());
    }
    #[cfg(windows)]
    {
        let lower = path.to_string_lossy().to_lowercase();
        if lower.ends_with(".exe")
            || lower.ends_with(".cmd")
            || lower.ends_with(".bat")
            || lower.ends_with(".com")
            || lower.ends_with(".ps1")
        {
            return Some(path.to_string_lossy().into_owned());
        }
        for ext in ["cmd", "exe", "bat"] {
            let sibling = path.with_extension(ext);
            if sibling.is_file() {
                return Some(sibling.to_string_lossy().into_owned());
            }
        }
        // Reject extensionless #! scripts (npm shims) — they are not Win32 apps.
        if path.is_file() {
            if let Ok(bytes) = std::fs::read(&path) {
                if bytes.starts_with(b"#!") {
                    return None;
                }
            }
            return Some(path.to_string_lossy().into_owned());
        }
        None
    }
}

fn resolve_binary_in_dirs(binary: &str, dirs: &[PathBuf]) -> Option<String> {
    for dir in dirs {
        // Windows: prefer .cmd/.exe before a bare npm shim of the same name.
        if cfg!(target_os = "windows") && !binary.contains('.') {
            for ext in [".exe", ".cmd", ".bat", ".ps1"] {
                let candidate = dir.join(format!("{binary}{ext}"));
                if candidate.is_file() {
                    return Some(candidate.to_string_lossy().to_string());
                }
            }
        }
        let direct = dir.join(binary);
        if direct.is_file() {
            if let Some(p) = prefer_createprocess_path(direct) {
                return Some(p);
            }
        }
    }
    None
}

/// Resolves the absolute path to a binary in PATH.
/// Tries `which::which` first, then falls back to `where.exe` on Windows,
/// then user-local install dirs that may not be on this process PATH yet.
pub fn resolve_binary_path(binary: &str) -> Option<String> {
    if let Ok(path) = which::which(binary) {
        if let Some(p) = prefer_createprocess_path(path) {
            return Some(p);
        }
    }

    // Windows fallback using where.exe
    if cfg!(target_os = "windows") {
        if let Ok(output) = Command::new("where.exe").arg(binary).output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Some(p) = prefer_createprocess_path(PathBuf::from(trimmed)) {
                        return Some(p);
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

    resolve_binary_in_dirs(binary, &extra_bin_dirs())
}

fn is_git_for_windows_bash(path: &str) -> bool {
    let lower = path.replace('/', "\\").to_lowercase();
    lower.contains("\\git\\") || lower.contains("\\portablegit\\")
}

/// Resolves the first existing binary for an agent (primary name, then aliases).
pub fn resolve_agent_binary(id: &str, primary: &str) -> Option<String> {
    if id == "git-bash" {
        if let Some(path) = resolve_binary_in_dirs("bash.exe", &extra_bin_dirs()) {
            if is_git_for_windows_bash(&path) {
                return Some(path);
            }
        }
        if let Some(path) = resolve_binary_path(primary) {
            if is_git_for_windows_bash(&path) {
                return Some(path);
            }
        }
        return None;
    }
    if let Some(path) = resolve_binary_path(primary) {
        return Some(path);
    }
    for alt in alternate_binaries(id) {
        if *alt == primary {
            continue;
        }
        if let Some(path) = resolve_binary_path(alt) {
            return Some(path);
        }
    }
    None
}

static AGENT_CACHE: std::sync::Mutex<Option<(std::time::Instant, Vec<DetectedAgent>)>> =
    std::sync::Mutex::new(None);
const AGENT_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(2);

/// Detects all supported agents on the host system.
pub fn detect_agents() -> Vec<DetectedAgent> {
    detect_agents_cached(false)
}

pub fn detect_agents_fresh() -> Vec<DetectedAgent> {
    detect_agents_cached(true)
}

fn detect_agents_cached(force: bool) -> Vec<DetectedAgent> {
    if !force {
        if let Ok(cache) = AGENT_CACHE.lock() {
            if let Some((at, agents)) = cache.as_ref() {
                if at.elapsed() < AGENT_CACHE_TTL {
                    return agents.clone();
                }
            }
        }
    }
    let detected = detect_agents_scan();
    if let Ok(mut cache) = AGENT_CACHE.lock() {
        *cache = Some((std::time::Instant::now(), detected.clone()));
    }
    detected
}

fn detect_agents_scan() -> Vec<DetectedAgent> {
    let supported = get_supported_agents();
    let mut detected = Vec::with_capacity(supported.len());

    for agent in supported {
        let binary_path = resolve_agent_binary(&agent.id, &agent.binary);
        let is_installed = binary_path.is_some();

        info!(
            agent_id = %agent.id,
            name = %agent.name,
            installed = is_installed,
            path = ?binary_path,
            "Detected agent status"
        );

        let (install_cmd, update_cmd, uninstall_cmd, manage_hint) = agent_lifecycle(&agent.id);
        let docs_url = agent_docs_url(&agent.id).map(str::to_string);

        detected.push(DetectedAgent {
            id: agent.id,
            name: agent.name,
            binary: agent.binary,
            description: agent.description,
            category: agent.category,
            is_installed,
            binary_path,
            install_cmd,
            update_cmd,
            uninstall_cmd,
            manage_hint,
            docs_url,
        });
    }

    detected
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    #[cfg(windows)]
    fn resolve_binary_in_dirs_finds_windows_cmd_shim() {
        let dir = std::env::temp_dir().join(format!(
            "termcrew-cmd-shim-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("agent.cmd");
        fs::write(&file, b"@echo off\r\n").expect("cmd shim");
        let found = resolve_binary_in_dirs("agent", &[dir.clone()]);
        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir(&dir);
        assert_eq!(found.as_deref(), Some(file.to_string_lossy().as_ref()));
    }

    #[test]
    #[cfg(windows)]
    fn resolve_binary_prefers_cmd_over_npm_unix_shim() {
        let dir = std::env::temp_dir().join(format!(
            "termcrew-npm-shim-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let unix = dir.join("pi");
        let cmd = dir.join("pi.cmd");
        fs::write(&unix, b"#!/bin/sh\nbasedir=$(dirname \"$0\")\n").expect("unix shim");
        fs::write(&cmd, b"@echo off\r\n").expect("cmd shim");
        let found = resolve_binary_in_dirs("pi", &[dir.clone()]);
        let via_prefer = prefer_createprocess_path(unix.clone());
        let _ = fs::remove_file(&unix);
        let _ = fs::remove_file(&cmd);
        let _ = fs::remove_dir(&dir);
        assert_eq!(found.as_deref(), Some(cmd.to_string_lossy().as_ref()));
        assert_eq!(via_prefer.as_deref(), Some(cmd.to_string_lossy().as_ref()));
    }

    #[test]
    fn resolve_binary_in_dirs_finds_file_not_on_path() {
        let dir = std::env::temp_dir().join(format!(
            "termcrew-extra-bin-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp extra bin dir");
        let name = "agy-extra-lookup";
        let file = if cfg!(windows) {
            dir.join(format!("{name}.exe"))
        } else {
            dir.join(name)
        };
        fs::write(&file, b"").expect("placeholder binary");
        let found = resolve_binary_in_dirs(name, &[dir.clone()]);
        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir(&dir);
        assert_eq!(found.as_deref(), Some(file.to_string_lossy().as_ref()));
    }

    #[test]
    fn aider_uninstall_uses_uv_tool_uninstall() {
        let (_install, _update, uninstall, hint) = agent_lifecycle("aider");
        assert!(hint.is_none());
        let cmd = uninstall.expect("aider must expose uninstall");
        assert!(
            cmd.contains("uv tool uninstall aider-chat"),
            "expected uv tool uninstall aider-chat, got {cmd}"
        );
        assert!(
            cmd.contains("pip uninstall") && cmd.contains("aider-install"),
            "expected pip uninstall of aider-install helper, got {cmd}"
        );
    }

    #[test]
    fn openhands_uninstall_uses_uv_tool_uninstall() {
        let (_i, _u, uninstall, hint) = agent_lifecycle("openhands");
        assert!(hint.is_none());
        let cmd = uninstall.expect("openhands must expose uninstall");
        assert!(
            cmd.contains("uv tool uninstall openhands"),
            "expected uv tool uninstall openhands, got {cmd}"
        );
    }

    #[test]
    fn cursor_agent_uninstall_removes_install_dirs_only() {
        let (_i, _u, uninstall, hint) = agent_lifecycle("cursor-agent");
        assert!(hint.is_none());
        let cmd = uninstall.expect("cursor-agent must expose uninstall");
        assert!(
            !cmd.contains(".cursor\"" ) && !cmd.contains("~/.cursor"),
            "must not wipe IDE ~/.cursor config: {cmd}"
        );
        if cfg!(windows) {
            assert!(
                cmd.contains("cursor-agent"),
                "Windows uninstall should remove LOCALAPPDATA\\cursor-agent: {cmd}"
            );
        } else {
            assert!(
                cmd.contains(".local/share/cursor-agent") || cmd.contains(".local/bin/agent"),
                "Unix uninstall should remove cursor-agent share/bin: {cmd}"
            );
        }
    }

    #[test]
    fn interpreter_exposes_documented_uninstall() {
        let (_i, _u, uninstall, hint) = agent_lifecycle("interpreter");
        assert!(hint.is_none());
        let cmd = uninstall.expect("interpreter must expose uninstall");
        assert!(
            cmd.contains("openinterpreter") || cmd.contains("Open Interpreter"),
            "expected Open Interpreter paths in uninstall, got {cmd}"
        );
    }

    #[test]
    fn amp_exposes_uninstall_of_install_tree() {
        let (_i, _u, uninstall, hint) = agent_lifecycle("amp");
        let cmd = uninstall.expect("amp must expose uninstall");
        assert!(
            hint.is_some(),
            "Amp docs omit uninstall — keep a manage hint"
        );
        assert!(
            cmd.contains(".amp"),
            "must remove ~/.amp install tree: {cmd}"
        );
        assert!(
            cmd.contains("@ampcode/cli") || cmd.contains("@sourcegraph/amp"),
            "must also try npm global remove: {cmd}"
        );
    }

    #[test]
    fn forge_lifecycle_win_and_unix() {
        let (install, update, uninstall, hint) = agent_lifecycle("forge");
        let install = install.expect("forge must expose install");
        let update = update.expect("forge must expose update");
        let uninstall = uninstall.expect("forge must expose uninstall");
        assert!(hint.is_some());
        assert!(
            install.contains("forgecode.dev/cli"),
            "expected official curl installer, got {install}"
        );
        assert!(
            update.contains("forge update"),
            "expected forge update, got {update}"
        );
        assert!(
            uninstall.contains(".forge"),
            "must remove ~/.forge install tree: {uninstall}"
        );
        if cfg!(windows) {
            assert!(
                install.contains("bash.exe") || install.contains("Git"),
                "Windows install should run under Git Bash: {install}"
            );
            assert!(
                uninstall.contains(r"Programs\Forge"),
                "Windows uninstall must remove Programs\\Forge: {uninstall}"
            );
        }
    }

    #[test]
    fn codewhale_probes_nsis_and_aliases() {
        assert_eq!(
            alternate_binaries("codewhale"),
            &["codewhale", "codew", "codewhale-tui"]
        );
        let (_i, update, uninstall, _hint) = agent_lifecycle("codewhale");
        assert!(update.as_deref().is_some_and(|c| c.contains("npm")));
        let cmd = uninstall.expect("codewhale uninstall");
        assert!(cmd.contains("npm uninstall"));
        if cfg!(windows) {
            assert!(
                cmd.contains(r"Programs\CodeWhale"),
                "Windows uninstall should cover NSIS tree: {cmd}"
            );
        } else {
            assert!(
                cmd.contains(".local/bin/codewhale"),
                "Unix uninstall should remove ~/.local/bin shims: {cmd}"
            );
        }
    }

    #[test]
    fn crush_lifecycle_has_install_update_remove() {
        let (install, update, uninstall, _hint) = agent_lifecycle("crush");
        let install = install.expect("crush install");
        let update = update.expect("crush update");
        let uninstall = uninstall.expect("crush uninstall");
        if cfg!(windows) {
            assert!(install.contains("winget") && install.contains("charmbracelet.crush"));
            assert!(update.contains("winget") && update.contains("upgrade"));
            assert!(uninstall.contains("winget") && uninstall.contains("uninstall"));
        } else {
            assert!(install.contains("brew") && install.contains("crush"));
            assert!(update.contains("brew"));
            assert!(uninstall.contains("brew") && uninstall.contains("uninstall"));
        }
    }

    #[test]
    fn codex_uninstall_removes_cli_only_not_dot_codex() {
        let (_i, update, uninstall, hint) = agent_lifecycle("codex");
        let cmd = uninstall.expect("codex must expose uninstall");
        let update = update.expect("codex must expose update");
        assert!(hint.is_some());
        assert!(
            update.contains("CODEX_NON_INTERACTIVE"),
            "update should use non-interactive installer: {update}"
        );
        assert!(
            !cmd.contains("Remove-Item -Path \"$env:USERPROFILE\\.codex\"")
                && !cmd.contains("rm -rf ~/.codex"),
            "must not wipe shared ~/.codex state: {cmd}"
        );
        assert!(
            !cmd.contains("brew uninstall"),
            "must not remove desktop cask: {cmd}"
        );
        if cfg!(windows) {
            assert!(
                cmd.contains(r"Programs\OpenAI\Codex"),
                "Windows uninstall should remove CLI Programs\\OpenAI\\Codex: {cmd}"
            );
        } else {
            assert!(
                cmd.contains(".local/bin/codex"),
                "Unix uninstall should remove ~/.local/bin/codex: {cmd}"
            );
        }
    }

    #[test]
    fn kiro_rebrand_lifecycle_and_aliases() {
        let agent = get_supported_agents()
            .into_iter()
            .find(|a| a.id == "kiro")
            .expect("kiro catalog entry");
        assert_eq!(agent.name, "Kiro");
        assert_eq!(agent.binary, "kiro-cli");
        assert!(get_supported_agents().iter().all(|a| a.id != "q"));

        let (install, update, uninstall, _hint) = agent_lifecycle("kiro");
        let install = install.expect("kiro install");
        assert!(
            install.contains("cli.kiro.dev"),
            "expected official installer host, got {install}"
        );
        assert_eq!(
            update.as_deref(),
            Some("kiro-cli update --non-interactive")
        );
        assert_eq!(uninstall.as_deref(), Some("kiro-cli uninstall"));
        assert_eq!(
            alternate_binaries("kiro"),
            &["kiro-cli", "q", "kiro"]
        );
        assert_eq!(agent_docs_url("kiro"), Some("https://kiro.dev/docs/cli"));
    }

    #[test]
    fn deepseek_is_official_harness() {
        let agent = get_supported_agents()
            .into_iter()
            .find(|a| a.id == "deepseek")
            .expect("deepseek catalog entry");
        assert_eq!(agent.binary, "dsh");
        assert_eq!(agent.default_args, vec!["web".to_string()]);
        assert_eq!(
            agent_docs_url("deepseek"),
            Some("https://deepseek-harness.github.io/deepseek-harness/")
        );
        let (install, update, uninstall, hint) = agent_lifecycle("deepseek");
        assert!(
            install.as_deref().is_some_and(|c| c.contains("@deepseek-ai/dsh")),
            "install must use official npm package, got {install:?}"
        );
        assert!(
            update.as_deref().is_some_and(|c| c.contains("@deepseek-ai/dsh")),
            "update must use official npm package, got {update:?}"
        );
        assert!(
            uninstall
                .as_deref()
                .is_some_and(|c| c.contains("@deepseek-ai/dsh")),
            "uninstall must use official npm package, got {uninstall:?}"
        );
        assert!(hint.is_none());
    }

    #[test]
    fn zoo_and_roo_are_not_in_catalog() {
        assert!(get_supported_agents().iter().all(|a| a.id != "zoo" && a.id != "roo"));
    }

    #[test]
    fn every_agent_has_https_docs_url() {
        for agent in detect_agents() {
            let url = agent
                .docs_url
                .as_deref()
                .unwrap_or_else(|| panic!("{} is missing docs_url", agent.id));
            assert!(
                url.starts_with("https://"),
                "{} docs_url must be https: {url}",
                agent.id
            );
        }
    }

    #[test]
    fn git_bash_path_accepts_git_and_portablegit() {
        assert!(is_git_for_windows_bash(r"C:\Program Files\Git\bin\bash.exe"));
        assert!(is_git_for_windows_bash(r"C:\PortableGit\bin\bash.exe"));
        assert!(is_git_for_windows_bash("C:/Program Files/Git/usr/bin/bash.exe"));
        assert!(!is_git_for_windows_bash(r"C:\msys64\usr\bin\bash.exe"));
        assert!(!is_git_for_windows_bash(r"C:\Windows\System32\bash.exe"));
    }

    #[test]
    fn system_shells_cannot_be_setup() {
        for id in ["shell", "cmd", "git-bash", "wsl", "bash", "zsh", "sh"] {
            assert!(is_system_shell(id), "{id} should be a system shell");
        }
        assert!(!is_system_shell("claude"));
        assert!(!is_system_shell("pwsh"));
    }
}
