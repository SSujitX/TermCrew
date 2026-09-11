# TermCrew — Product Requirements Document

| | |
| --- | --- |
| Product | TermCrew — local web console for running a crew of AI coding CLIs side by side |
| Version covered | 1.0.0 (`VERSION`) |
| Status | Shipped behaviour is marked **[Implemented]**; everything else is marked **[Proposed]** and is not in the code |
| Related | [ARCHITECTURE.md](ARCHITECTURE.md) · [DESIGN.md](DESIGN.md) · [AGENTS.md](AGENTS.md) · [README.md](README.md) |

---

## 1. Problem

Developers now keep several AI coding CLIs installed (Claude Code, Codex, Gemini CLI, Cursor Agent, OpenCode, Aider, …). Using more than one at a time means juggling terminal windows, remembering which one is in which folder, copy-pasting the same prompt into each, and manually shuttling a diff from a "builder" to a "reviewer". Running agents concurrently in the same checkout also makes them step on each other's files.

There is no lightweight, local, vendor-neutral surface that:

- shows every agent's real terminal at once,
- launches them in a chosen arrangement (one, a builder+reviewer pair, agent+shell, or a swarm),
- isolates concurrent workers without polluting the repository,
- lets the developer manage which CLIs and skills are installed,
- and survives a backend restart without losing the transcript.

## 2. Vision

One browser window that acts as the developer's **crew console**: real PTYs for any CLI, arranged in presets, with shared controls (broadcast, handoff, restart), a registry for the agents and skills on the machine, and an editor for the files the crew is changing. Everything runs on the developer's machine; no cloud control plane.

## 3. Target users

| Persona | Needs |
| --- | --- |
| **Multi-CLI power user** | Compare or combine agents on the same task; keep long-running sessions alive across page reloads |
| **Reviewer-minded developer** | Have one agent lead and another critique the same folder; Swarm workers stay isolated |
| **Tool maintainer / tinkerer** | See which CLIs are on PATH, install/update/remove them from one place, manage Agent Skills across harnesses |
| **Windows-first developer** | Native ConPTY behaviour, PowerShell / cmd / Git Bash / WSL as first-class shells, no WSL requirement |

## 4. Goals and non-goals

### Goals (v1.0.0)

1. Real terminals: every pane is a native PTY with full-fidelity TUI rendering (colours, cursor, resize).
2. Presets that map to how people actually pair agents: Solo, Pair, Workbench, Swarm.
3. Zero repo pollution: worker isolation via git worktrees stored in app-data, not in the project.
4. Crew-level controls: broadcast a prompt, hand a diff to a reviewer, restart, kill a whole group.
5. Local tool management: detect, install, update, uninstall CLIs; browse, enable, copy, delete, install skills.
6. Resilience: sessions and their scrollback survive backend restarts and can be relaunched.
7. Windows and macOS parity for core flows.

### Non-goals (v1.0.0)

- Remote or multi-user access, authentication, or hosting the UI from the backend.
- A server-side orchestrator or planner LLM (Goals is a role playbook over existing broadcast/handoff, see §6.9).
- Parsing or interpreting agent output (TermCrew is deliberately protocol-agnostic; it moves bytes).
- Replacing the agents' own configuration, model selection or billing.
- Linux as a supported target (code paths exist but are untested; README lists Windows and macOS).

## 5. Success criteria

Measured on a developer machine with the backend and Vite running:

| Metric | Target | How it is met today |
| --- | --- | --- |
| Time from "New session" to first prompt visible | < 3 s for a shell, bounded by the agent CLI for agents | PTY spawn is synchronous; the Task is typed after the CLI goes quiet (≤ 12 s), not on a fixed 0.6 s timer |
| TUI fidelity after reload | No garbled frames | Backend waits for the browser's size before replaying scrollback |
| Output under load (e.g. `cat` of a large file) | UI stays responsive, no unbounded memory | 128 KiB server ring buffer, 3000-line xterm scrollback, ACK-based pause/resume at 256 KiB/32 KiB |
| Backend restart | Every previous session visible with its last ~128 KiB of output, one click to relaunch | Parked sessions + `Restart` |
| Kill | No orphaned agent processes | Job Objects / process groups, covered by an integration test |
| Repo cleanliness | `git status` in the user's repo unchanged by Swarm/Pair | Worktrees under `%LOCALAPPDATA%\termcrew\worktrees` |

## 6. Functional requirements

Requirement ids are stable; use them in issues and commit messages.

### 6.1 Sessions and presets — [Implemented]

| Id | Requirement |
| --- | --- |
| S-1 | A **session** is one PTY running one engine (an agent id or a shell id) in one working directory, with a name, role, preset, group, creation time and alive flag. |
| S-2 | A **launch** creates one **group** of sessions from a preset, an ordered list of selected agents, an optional task prompt and a **required** working folder. Launch is disabled until a folder is chosen; the backend rejects an empty path. The task is typed into each agent pane only after that pane’s prompt is ready, then Enter is sent. |
| S-3 | **Solo**: one pane per selected agent, or *N* copies (1–10) of a single agent, all in the working folder. Roles `Lead` / `Lead n`. |
| S-4 | **Pair**: the first selected agent is the `Lead` in the working folder; every further selection (or the same agent if only one was picked) is `Review n` in that **same** folder, seeded with a review prompt that quotes the task. Roles are labels, not locks. |
| S-5 | **Workbench**: one pane per selected agent (`Agent` / `Agent n`) plus one system `Shell` pane, all in the working folder. |
| S-6 | **Swarm**: one `Worker n` per selected agent, or *N* copies (1–10) of a single agent, each in its own worktree; the task is suffixed with `[Worker n]`. |
| S-7 | If any pane of a launch fails to start, the whole launch is rolled back (already-started panes killed, worktrees removed) and an actionable error is shown. |
| S-8 | Launching an agent that is not installed fails with an error naming the missing binary. |
| S-9 | Groups are labelled `{Preset} {n}` (e.g. `Pair 1`) and can be renamed (≤ 80 chars). |
| S-10 | A session can be killed individually or with its whole group (with confirmation). Kill terminates the entire process tree and removes the session's worktree and persisted data. |
| S-11 | A session can be restarted in place: same id, role, folder, worktree; the previous transcript is replayed followed by a `--- relaunched ---` marker. |
| S-12 | Sessions persist across backend restarts as **parked** sessions: they appear in the sidebar, show their last transcript and a banner, and offer Restart. |
| S-13 | The UI polls the session list every 5 s and reflects alive/exited state per pane and per group. |
| S-14 | The working folder used for a launch is recorded in a recents list (max 12, newest first, pruned when a folder disappears). |
| S-15 | A **+** on the group tab strip adds one pane to that group (same folder; Swarm gets a new worktree). Existing PTYs are not restarted. Pair → Review n, Solo → Lead n, Workbench → Agent n / Shell, cap 6. |

### 6.2 Terminal panes — [Implemented]

| Id | Requirement |
| --- | --- |
| T-1 | Each pane renders a live xterm.js terminal over a binary WebSocket, GPU-accelerated when available (WebGL → Canvas → DOM). |
| T-2 | The pane sends its real size on connect and on every resize; TUIs reflow accordingly. |
| T-3 | Reconnecting (reload, socket drop) replays the server-side scrollback (≤ 128 KiB) and continues live. Reconnects retry with backoff up to 2 s. |
| T-4 | Copy: `Ctrl/Cmd+C` with a selection (or with Shift), or the Copy button (selection, else whole buffer). Paste: `Ctrl/Cmd+V` or right-click. |
| T-5 | Per-pane controls: Clear (unsent chat box; same PTY bytes on Windows and macOS), Copy, Handoff (see B-3), Restart, Maximize, Kill. |
| T-6 | Layouts: **One** (tabs) and **Split** (grid); grid adapts to 1, 2, 3, 4, 5–6 and more panes; any pane can be maximized. One `{#each}` keyed by session id: One ↔ Split and maximize only change CSS (`invisible`), so sockets stay up. |
| T-7 | A pane whose process has exited shows an exited state; the session stays listed until killed. |

### 6.3 Broadcast and handoff — [Implemented]

| Id | Requirement |
| --- | --- |
| B-1 | The broadcast bar sends a line of text (newline appended if missing) to the focused pane (**This**) or to every pane of the focused group (**All**). |
| B-2 | Quick commands are available: `git status`, `git diff`, `pnpm test`, `exit`. |
| B-3 | **Send review** (API: handoff) builds a compact packet — source role/engine, folder, launch task (≤200 chars if set), `git diff HEAD` + short status — capped at 6 KiB. No chat/transcript. Target must be running. The UI sends to the **next pane in the same group** (Lead → Review 1 → … → wrap). |
| B-4 | Broadcast reports how many panes received the input. |

### 6.4 Agent registry — [Implemented]

| Id | Requirement |
| --- | --- |
| A-1 | TermCrew ships a catalogue of 28 coding-agent CLIs plus the system shells for the OS, each with name, binary, description, docs URL and OS-specific install / update / uninstall commands where they exist. |
| A-2 | Detection finds binaries on PATH and in known vendor install folders; it prefers real executables over npm shims on Windows; results refresh within 2 s (or immediately with *fresh*). |
| A-3 | From the Agents modal the user can search and filter (All / Ready / Missing), switch grid/list, open docs, copy the resolved path, and **Install / Update / Remove** any agent that defines the corresponding command. |
| A-4 | Install/update/remove run in an embedded setup console (a hidden PTY, never shown in the sidebar). Remove first stops live panes of that agent, then runs the vendor command, then force-deletes leftover XDG dirs and shims if the official uninstaller left them locked. Completion is detected automatically and the catalogue rescans; the console can be closed at any time. |
| A-5 | Agents that cannot be installed on the current OS (e.g. Plandex on Windows) show a hint paragraph and a disabled Install button whose tooltip repeats the hint. |
| A-6 | An installed agent can be launched directly from the registry (opens the launcher with it preselected). |

### 6.5 Skills registry — [Implemented]

| Id | Requirement |
| --- | --- |
| K-1 | TermCrew scans the user's skill roots for the major harnesses (`~/.agents`, `~/.claude`, `~/.codex`, `~/.cursor`, OpenCode, OpenClaw, Hermes, Gemini, Kiro, Goose) and the project's `.claude/.agents/.cursor/.codex` skill folders, listing every directory that contains `SKILL.md`. |
| K-2 | Harness-managed folders (`.system`, `skills-cursor`) are shown read-only and never modified. |
| K-3 | Per skill: **Show/Hide** (TermCrew-only preference), **Preview** (≤ 256 KiB), **Copy** to another writable root, **Open** folder, **Delete** (three-step confirmation; backend requires the literal token `DELETE`). |
| K-4 | Install from a local folder or a git URL (shallow clone, hooks disabled) into a chosen writable root, optionally renamed. |
| K-5 | Marketplace tab browses skills.sh on load (9 per page, Hot / Trending / Most viewed) and searches by keyword; installs via `npx skills add … --copy` for a chosen harness in an embedded setup console. |
| K-6 | All writes are confined to the allow-listed roots; path traversal is rejected. |

### 6.6 Files — [Implemented]

| Id | Requirement |
| --- | --- |
| F-1 | The sidebar Files tab shows a lazy tree rooted at the focused session's worktree or working folder (or the backend's cwd when nothing is focused), with VS Code-style icons. |
| F-2 | Clicking a file opens a tab on the session strip and overlays Monaco on the pane area (language detection, console theme). PTYs stay mounted. A session chip returns to the terminals. |
| F-3 | Save (`Ctrl/Cmd+S`) writes back only if the file has not changed on disk since it was opened; otherwise the save is refused with an explanation. `Esc` closes. |
| F-4 | Files larger than 2 MiB or that look binary are refused with a clear message. |
| F-5 | The Files tab can create a file or folder under the tree root via path (`notes.txt` or `src/lib/util.ts`). Intermediate folders are created; `..` and paths outside the root are rejected. New files open in the editor. |
| F-6 | Each file and folder row has a delete icon. First click shows **Sure?**; second click deletes. Paths outside the session folder and the folder root itself are rejected. An open editor for that path closes. |

### 6.7 Working folder selection — [Implemented]

| Id | Requirement |
| --- | --- |
| W-1 | The launcher offers up to 5 recent folders, a free-text path, a native OS folder picker (Explorer / Finder), and an in-app folder browser fallback. |
| W-2 | The folder field starts empty (no recents or backend cwd preselected). Launch stays disabled until the user picks a recent, types a path, or uses Browse. |

### 6.8 Storage transparency — [Implemented]

| Id | Requirement |
| --- | --- |
| D-1 | The sidebar footer shows where TermCrew stores its data and can open that folder in the OS file manager. Usernames are redacted in the display (`~`, `%LOCALAPPDATA%`). |
| D-2 | Data lives under `%LOCALAPPDATA%\termcrew` (Windows) or `~/.local/share/termcrew` (macOS/Linux): `sessions/`, `scrollback/`, `worktrees/`, `skills-prefs.json`; recents under `~/.termcrew/`. Legacy `multiagent` folders are migrated once. |

### 6.9 Goals panel (role playbook) — [Implemented as a client-side convenience]

The panel exists and works, but it is important to state precisely what it does:

| Id | Behaviour |
| --- | --- |
| G-1 | **Run playbook** types a *role-specific* contract into each running pane of the focused group: builders implement (sliced if several), reviewers wait for a git packet and must not implement, shells are left alone. Same text is never blasted to every role. |
| G-2 | **Send review** hands the compact git packet from each running builder to each running Review pane (same as pane Send review). Disabled when the group has no reviewer. |
| G-3 | **Run tests** detects `cargo test` / `go test` / `pytest` / `npm|pnpm|yarn|bun run test` from the session folder and types that command into the Shell (Workbench) or the first builder. No hardcoded `npm test`. |
| G-4 | The log records only those sends. There is no fake Plan→Build→Review→Verify machine and no backend supervisor. |

A real supervisor (backend-owned goal state, per-agent assignment, completion detection) is **[Proposed]** — see §9.

## 7. Non-functional requirements — [Implemented unless noted]

| Id | Requirement | Status |
| --- | --- | --- |
| N-1 | Backend binds loopback only; CORS restricted to the Vite origins. | Implemented |
| N-2 | Bounded memory per session: 128 KiB server scrollback, 32 KiB reads, backpressure that pauses the PTY when the browser falls > 256 KiB behind. | Implemented |
| N-3 | Killing the backend (Ctrl-C/SIGTERM) parks sessions and terminates every agent process tree; no orphans. | Implemented |
| N-4 | Windows: setup scripts run from temp `.ps1` files to avoid Defender false positives; ConPTY via portable-pty. | Implemented |
| N-5 | The UI degrades gracefully when the backend is down (static agent list, empty sessions, "not found" pane message). | Implemented |
| N-6 | Accessibility: keyboard operation of modals (Esc, Ctrl+Enter), visible focus ring on the active pane, reduced-motion respected. Six launcher form labels are not programmatically associated with their controls (svelte-check reports 8 warnings in total, 6 of them this). | Partial |
| N-7 | Authentication for the local API. | Not implemented (see §9) |
| N-8 | Automated checks: `cargo test` (51 tests), `bun run check`, `bun run icons:check`. Two integration tests are environment-sensitive. | Partial (see AGENTS.md) |

## 8. Platform and dependencies

- **Runtime:** Rust toolchain (edition 2024), Node.js 20.19+ or 22.12+ (required by Vite 8; the README still says 18+), Git on PATH. Windows 10/11 or macOS. Browser with WebGL preferred.
- **Backend:** Axum 0.7, tokio, portable-pty 0.8, tower-http 0.5, rfd 0.15 (native dialogs), `windows` 0.62 (Job Objects) / `libc`.
- **Frontend:** Svelte 5, Vite 8, Tailwind 4, TypeScript 6 (strict), `@xterm/xterm` 6 with fit/webgl/canvas addons, Monaco 0.56, lucide icons, Iconify vscode-icons.
- **External network:** only the skills marketplace search (`https://skills.sh/api/search`) and whatever the agents' own installers and CLIs do.

## 9. Roadmap — [Proposed]

Ordered by expected value; none of this exists in the code today.

1. **Production hosting** — build the SPA and serve it from the backend (the `tower-http` `fs` feature is already enabled), so `cargo run` alone gives a working URL; make the WebSocket URL relative in that mode.
2. **Local API token** — a per-launch random token required on every REST/WS call, injected into the SPA, so a malicious page or another local user cannot drive the console.
3. **Workspace containment option** — restrict `/api/fs/*`, `/api/worktrees/diff` and launch folders to an allow-list (defaults to the recents list) when the token mode is on.
4. **Real supervisor** — backend-owned goals with per-session assignment, completion heuristics (prompt idle detection), and a persisted event log replacing the simulated stage machine.
5. **Session import / export** — save a group definition (preset, agents, folder, task) as a reusable launch template.
6. **Linux support** — test the existing `xdg-open` / bash paths and add Linux to the README badge.
7. **Pane polish** — keep panes mounted when switching Split ↔ maximized (single `{#each}` with layout-only class changes), link detection (`@xterm/addon-web-links`), copy-on-select option, search addon, font-size control.
8. **Diff viewer and handoff target picker** — surface `/api/worktrees/diff` (already implemented, unused by the UI) as a side panel per worker, and let the user choose the handoff target (default: the Reviewer in the same group) instead of the first other session.
9. **Test hardening** — an environment override for the data root so `cargo test` never reads the developer's real sessions; frontend component tests.

## 10. Open questions

- Should Swarm workers get a merge/cherry-pick helper back into the main checkout, or is the diff/handoff flow enough?
- Should hidden setup consoles be listed somewhere (e.g. an "activity" drawer) so a stuck installer is discoverable?
- Should the UI default preset be Pair (current) or Solo?
- README advertises "Swarm (4–6)"; backend default is 3 and the UI default is 4 — pick one and align copy.
