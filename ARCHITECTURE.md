# TermCrew — Architecture

> Describes the system **as implemented** at version 1.0.0 (`VERSION`, `backend/Cargo.toml`, `frontend/package.json`).
> Every path, constant and endpoint below is taken from the source. When code and this document disagree, the code wins — fix the document.

Companion documents: [PRD.md](PRD.md) (what and why), [DESIGN.md](DESIGN.md) (UX and protocol design), [AGENTS.md](AGENTS.md) (working in this repo).

---

## 1. System overview

TermCrew is two local processes and a browser:

```
┌───────────────────────────────┐        ┌───────────────────────────────────────────┐
│ Browser  http://localhost:5173│        │ Vite dev server  :5173                    │
│  Svelte 5 SPA                 │◀──────▶│  serves SPA, proxies /api → 127.0.0.1:3001│
│  xterm.js panes, Monaco       │  HTTP  │  (keep-alive http.Agent)                  │
└──────────────┬────────────────┘        └───────────────────────────────────────────┘
               │ WebSocket (direct, NOT proxied in dev)
               │ ws://127.0.0.1:3001/ws/:session_id
               ▼
┌──────────────────────────────────────────────────────────────────────────────────┐
│ termcrew backend (Rust, Axum 0.7, tokio)  127.0.0.1:3001                          │
│                                                                                  │
│  REST /api/*  ──▶ session.rs ──▶ pty_manager.rs ──▶ portable-pty ──▶ OS PTY       │
│                     │  │            │  ▲                 (ConPTY / Unix pty)      │
│                     │  │            │  └─ proc_guard.rs (Job Object / pgid)       │
│                     │  └─ worktree.rs (git worktrees in app-data)                 │
│                     └──── persist.rs (sessions/*.json, scrollback/*.bin)          │
│  /ws/:id ────────▶ ws_handler.rs ◀── pty_wire.rs (opcode frames)                  │
│  registry.rs (agent catalog + PATH detection)   skills.rs   workdirs.rs           │
│  file_editor.rs                                                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
               │ spawns
               ▼
   claude / codex / gemini / … / powershell.exe / zsh   (one child process tree per pane)
```

Key properties:

- **Local-first.** The backend binds `127.0.0.1:3001` only. There is no auth layer; the trust boundary is "whoever can reach loopback on this machine".
- **No static file serving.** The backend does not serve the SPA (the `fs` feature of `tower-http` is enabled in `Cargo.toml` but unused). In dev the SPA comes from Vite; there is no production hosting path wired up yet.
- **Two transports.** JSON over REST for control; a binary opcode protocol over WebSocket for terminal bytes.
- **Sessions outlive sockets.** Closing a browser tab does not kill a PTY. Killing the backend *parks* sessions (metadata + scrollback persisted) so they can be relaunched.

---

## 2. Repository layout

```
multiagent/                      (git root; product name is TermCrew)
├── backend/                     Rust crate `termcrew`, edition 2024
│   ├── Cargo.toml               axum 0.7 (ws), tokio, portable-pty 0.8, tower-http 0.5,
│   │                            serde, uuid v4, tracing, which 7, chrono, rfd 0.15,
│   │                            windows 0.62 (JobObjects/Threading), libc (unix)
│   ├── src/
│   │   ├── main.rs              binary: tracing, CORS, router, bind, shutdown → park
│   │   ├── lib.rs               declares the 11 modules below
│   │   ├── session.rs           presets, launch/kill/restart/park, broadcast, handoff, setup consoles
│   │   ├── pty_manager.rs       PtyManager: spawn, reader thread, ring buffer, broadcast channel
│   │   ├── pty_wire.rs          opcode constants, frame encode/decode, setup sentinels
│   │   ├── proc_guard.rs        whole-process-tree kill (Windows Job Object / Unix pgid)
│   │   ├── ws_handler.rs        WebSocket bridge, flow control, scrollback replay, parked replay
│   │   ├── persist.rs           sessions/*.json, scrollback/*.bin, storage info, open folder
│   │   ├── registry.rs          28 agent definitions + shells, PATH detection, lifecycle commands
│   │   ├── skills.rs            skills roots scan, prefs, copy/delete/install, skills.sh proxy
│   │   ├── workdirs.rs          recent folders, directory browsing, native folder picker
│   │   ├── worktree.rs          app-data root, git worktree create/remove/sweep, diff
│   │   └── file_editor.rs       guarded text file read/write for Monaco
│   └── tests/integration_tests.rs
├── frontend/                    Vite 8 + Svelte 5 (runes) + Tailwind 4 + TypeScript (strict)
│   ├── vite.config.ts           port 5173, /api proxy, monaco css alias
│   ├── index.html               Google Fonts: Chivo Mono, IBM Plex Mono, Share Tech Mono
│   ├── public/agents/*.png|svg  agent logos, looked up by agent id
│   ├── scripts/verify-file-icons.mjs
│   └── src/
│       ├── main.ts, App.svelte, app.css
│       └── lib/
│           ├── api.ts           typed REST client (relative URLs)
│           ├── types.ts         shared TS types
│           ├── ptySocket.ts     opcode WebSocket client with ACK flow control + reconnect
│           ├── termClipboard.ts xterm copy/paste key + right-click handling
│           ├── monaco.ts        Monaco workers + `termcrew-dark` theme
│           ├── fileIcons.ts     vscode-icons mapping + language detection
│           ├── ui.ts            cn(), btn/badge variants, xterm theme, path redaction
│           └── components/      13 Svelte components (see §9)
├── docs/superpowers/specs/      design specs (e.g. 2026-09-06 skills registry)
├── run-dev.ps1 / start.bat      Windows launchers (backend window + Vite)
├── scripts/sync_version.sh      empty placeholder
├── VERSION                      1.0.0
└── README.md
```

---

## 3. Backend runtime model

### 3.1 Startup (`main.rs`)

1. `tracing_subscriber` with `EnvFilter` (default `termcrew=debug,tower_http=debug`) + fmt layer.
2. `worktree::sweep_stale_worktrees()` on a blocking thread — deletes `agent-*` worktree directories whose `.git` file points at a `gitdir:` that no longer exists, then removes empty per-repo hash directories.
3. `session::create_app_state()` — loads every `sessions/<id>.json` into memory as a **parked** session (`pty: None`, `is_alive: false`) and starts the 10-second scrollback flusher.
4. Router built; CORS layer wraps **REST only** (wrapping the WebSocket route breaks the 101 upgrade). CORS allows origins `http://localhost:5173` and `http://127.0.0.1:5173`, methods GET/POST/PUT/DELETE/OPTIONS, headers `content-type`, `authorization`, `accept`, no credentials.
5. `TraceLayer` on the merged app; bind `127.0.0.1:3001`; log `Server listening on http://127.0.0.1:3001`.
6. A shutdown task waits for Ctrl-C (and SIGTERM on Unix), calls `park_all_sessions`, then `std::process::exit(0)`. The explicit exit matters: PTY children are not tied to the backend process, so a plain return would orphan every agent.

### 3.2 State

```rust
pub type AppState = Arc<RwLock<HashMap<String, ActiveSession>>>;

pub struct ActiveSession {
    pub info: SessionInfo,
    pub pty: Option<Arc<PtyManager>>,   // None ⇒ parked (backend restarted)
}

pub struct SessionInfo {
    pub id: String,            // uuid v4
    pub name: String,          // e.g. "Lead · claude", "Worker 2 · codex", "Shell"
    pub engine: String,        // agent id or shell id ("claude", "shell", "git-bash", …)
    pub preset: String,        // "Solo" | "Pair" | "Workbench" | "Swarm" | "Setup"
    pub role: Option<String>,  // "Lead", "Review 2", "Agent", "Shell", "Worker 3", …
    #[serde(default)] pub label: Option<String>, // sidebar/strip display; UI falls back to role
    pub working_dir: String,
    pub worktree_path: Option<String>,
    pub created_at: String,    // RFC 3339 (chrono Utc)
    pub is_alive: bool,
    pub group_id: String,      // one per launch
    pub group_label: String,   // "Pair 1", "Swarm 3", …
    #[serde(default)] pub hidden: bool, // true for install/update/uninstall consoles
    #[serde(default)] pub task: Option<String>, // launch task; omitted for Setup
}
```

There is no status enum on the backend. The frontend derives `'running' | 'exited'` from `is_alive` (`api.ts` `mapSession`); the `'paused' | 'error'` values in `types.ts` are never produced by the HTTP mapping.

Global side state: a process-wide `HashSet<String>` of session ids currently inside `restart_session` (guarded by `RestartGuard`), so a concurrent kill cannot delete a worktree mid-restart.

### 3.3 Session lifecycle (`session.rs`)

**Launch** (`launch_preset`):

1. Resolve `base_dir` (required; empty is rejected). Must exist and be a directory.
2. Ordered engine roster = `engines` (trimmed, non-empty) or `[engine]`.
3. `group_id = uuid`, `ordinal = (#distinct non-hidden groups) + 1`, `group_label = "{Preset} {ordinal}"` (e.g. `Pair 1`).
4. Per preset (all panes spawned at 24 rows × 80 cols; the browser resizes immediately):

| Preset | Panes | Working dir | Roles / task |
| --- | --- | --- | --- |
| `solo` | roster if >1 engine, else `count.unwrap_or(1).clamp(1,10)` copies | `base_dir` | `Lead` or `Lead {i}`; task passed through |
| `pair` | 1 lead + reviewers = `engines[1..]`, else `reviewer_engine`, else same engine | **all** `base_dir` (no worktree) | lead gets task; review gets `"Review code changes in this repository. Target task: {task \| Autonomous code review}"` |
| `workbench` | one pane per roster engine + one `shell` pane | `base_dir` | `Agent` / `Agent {i}`, then `Shell` (role `Shell`, no task) |
| `swarm` | roster if >1 engine, else `count.unwrap_or(3).clamp(1,10)` copies | each worker: new **worktree** `agent-{6 hex}-w{n}` | `Worker {n}`; task becomes `"{task} [Worker n]"` |

   Any other preset string → error `Unknown preset`. If a later pane fails the whole launch returns an error and nothing is persisted (persistence happens only after the match). In `swarm` (and `pair` on a later-pane failure), `abandon_launched` kills already-spawned panes and removes any worktrees; `solo` / `workbench` / `pair` create no worktrees, so rollback is PTY drop only.
5. Each `SessionInfo` is persisted, inserted into state, and `workdirs::record_workdir(base_dir)` updates the recents list.

**Spawn** (`spawn_single_session` → `resolve_engine_cmd`): the engine id is looked up in `registry::get_supported_agents()`; the binary is resolved via `registry::resolve_agent_binary` (fails with an actionable "not installed" error). Environment gets `TERM=xterm-256color`, `COLORTERM=truecolor`, `FORCE_COLOR=3`, and `NO_COLOR` is removed. The optional launch **Task** is typed only after the PTY has printed, stayed quiet ≥700 ms (min 1.5 s boot, cap 20 s), and the scrollback looks like a chat/shell prompt (`? for shortcuts`, a trailing `>` / `❯`, …). A theme-picker Enter is sent only when that menu is actually on screen. The task is submitted with `\r`. The `Setup` preset skips both.

**List** (`list_sessions`): skips `hidden`, refreshes `is_alive` from the PTY, and on the alive→dead transition saves scrollback and metadata. Sorted newest first.

**Kill** (`kill_session`): refused while the id is restarting. Kills the PTY (whole tree). For non-hidden sessions, deletes `sessions/<id>.json`, `scrollback/<id>.bin`, and removes the worktree (if any). Removes from state.

**Restart** (`restart_session`): keeps the **same id**, worktree, group and role. Seeds the new PTY's history with the old history (live) or disk scrollback (parked) plus `"--- relaunched ---"`, spawns, then kills the old PTY.

**Park** (`park_all_sessions`, on shutdown): saves scrollback, kills every PTY, persists metadata, and deletes hidden setup sessions. On next start they load as parked; the UI shows history plus a banner and offers Restart.

**Broadcast** (`broadcast_input`): appends `\r` if the input does not already end in `\n`/`\r`; writes to the listed ids (or all live sessions when `session_ids` is `None`); returns delivered count.

**Handoff** (`handoff_review`): compact review packet (role, engine, folder, optional launch task ≤200 chars, `git diff HEAD` + short status). Cap `MAX_REVIEW_BYTES = 6 KiB`. No chat transcript. Writes into the **target** PTY (must be live). UI picks the next pane in the same group.

**Add pane** (`add_to_group`): one new PTY in an existing group (cap 6). Same folder as the group (Swarm: new worktree). Pair → `Review n`, Solo → `Lead n`, Workbench → `Agent n` / `Shell`, Swarm → `Worker n`. Other sessions are not restarted. Setup groups refused.

**Rename** (`rename_group`): trims label, max 80 chars, applies to every session with that `group_id`, persists.

**Rename node** (`rename_session`): trims display `label`, max 80 chars, one session. Does not change `role`. Hidden setup consoles refused. Persists.

**Setup consoles** (`launch_agent_setup`, `launch_command_setup`): spawn a hidden `Setup` preset shell (24 × 100). `launch_agent_setup` builds an OS-specific script from `registry::agent_lifecycle` (install/update/uninstall) that refreshes PATH, verifies the binary, and prints a sentinel `__MA_SETUP__:ok` / `__MA_SETUP__:fail`. Uninstall also kills live panes of that engine, then after the vendor command force-removes leftover `~/.local/share|cache|config|state/<binary>` trees and known shims (Windows `Stop-Process` + `rd /s /q` on EBUSY; Unix `pkill -x` + `rm -rf`). On Windows the script is written to a temp `.ps1` and run with `powershell -NoLogo -NoProfile -NoExit -ExecutionPolicy Bypass -File …` (avoids Defender flagging inline `-Command`). `launch_command_setup` runs a caller-provided command (≤ 4000 chars) the same way; it is used by the skills marketplace installer and git skill install.

### 3.4 PTY layer (`pty_manager.rs`, `proc_guard.rs`)

```
PtyManager::spawn(cmd, args, cwd, env, rows, cols, seed_history)
  ├─ portable_pty::native_pty_system().openpty(size)
  ├─ CommandBuilder → slave.spawn_command(...)        (ConPTY on Windows, forkpty on Unix — inside portable-pty)
  ├─ ProcessGuard::attach(child.process_id())
  └─ std::thread "pty-reader":
        loop { if paused { sleep 8ms; continue }
               n = master.read(buf[32 KiB]) ; on EOF/err → is_alive=false, break
               history.append(bytes) truncated to last 128 KiB
               broadcast_tx.send(bytes)  (tokio::sync::broadcast, capacity 8192) }
```

| Constant | Value | Where |
| --- | --- | --- |
| `MAX_HISTORY_BYTES` | 128 KiB | ring-style scrollback kept per session |
| `PTY_READ_BUF` | 32 KiB | fixed read size |
| `BROADCAST_CAPACITY` | 8192 chunks | slow subscribers get `Lagged` and skip |

`ProcessGuard`:
- **Windows:** creates a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, assigns the child; `kill()` → `TerminateJobObject`; `Drop` closes the handle, so the kernel reaps the whole tree even if the backend crashes.
- **Unix:** records the pgid (portable-pty `setsid`s the child); `kill()` sends `SIGTERM` to `-pgid` then `SIGKILL` after ~1 s if still alive; `Drop` sends `SIGKILL`.

`test_kill_terminates_whole_process_tree` verifies that a grandchild dies when the session is killed.

### 3.5 WebSocket bridge (`ws_handler.rs`, `pty_wire.rs`)

Route: `GET /ws/:session_id` (no CORS layer). One socket per pane; a session may have several sockets (each gets its own broadcast receiver).

**Wire format** — binary frames, first byte is the opcode (ttyd-style):

| Opcode | Byte | Direction | Payload |
| --- | --- | --- | --- |
| `OP_DATA` | `0x00` | both | raw terminal bytes (server→client) / keyboard input (client→server) |
| `OP_RESIZE` | `0x01` | client→server | `cols: u16 LE`, `rows: u16 LE` |
| `OP_PAUSE` | `0x02` | client→server | — (stops the PTY reader) |
| `OP_RESUME` | `0x03` | client→server | — |
| `OP_ACK` | `0x04` | client→server | `bytes_consumed: u32 LE` |
| `OP_EXIT` | `0x05` | server→client | `exit_code: i32 LE` (currently always 0) |
| `OP_SETUP` | `0x06` | server→client | `0x01` ok / `0x00` fail (setup console result) |

Legacy JSON text frames are still accepted: `{"type":"input","data"}`, `{"type":"resize","cols","rows"}`, `{"type":"ping"}`. The server sends two JSON text frames: `{"type":"connected","session_id"}` on attach and `{"type":"error","message"}` when the session id is unknown (followed by Close).

**Attach sequence (live session):**

1. Send `connected`.
2. Wait up to `RESIZE_WAIT = 600 ms` for the first resize (input received meanwhile is queued). If a resize arrived, apply it and sleep `RESIZE_SETTLE = 120 ms`. Rationale: TUI agents (Ink, Kilo, …) redraw on SIGWINCH; replaying 80×24 frames into a 200×50 terminal produces garbage.
3. Send the whole history as one `OP_DATA` frame (setup sentinels stripped; if a sentinel was present, also send `OP_SETUP`).
4. Flush queued input, then split into a send task and a receive task.

**Send task:** pulls chunks from the broadcast receiver, coalesces up to `MAX_COALESCE_BYTES = 64 KiB`, strips sentinels, frames as `OP_DATA`, and adds the byte count to an `unacked` counter. When `unacked > FLOW_HIGH = 256 KiB` the PTY is paused. A 500 ms tick checks `is_alive`; on death sends `OP_EXIT(0)` and ends.

**Receive task:** input → `write_input`; resize → `resize`; pause/resume → `set_paused`; `ACK(n)` → `unacked -= n`, and if `unacked < FLOW_LOW = 32 KiB` the PTY resumes. Unknown binary frames whose first byte is not `OP_DATA` are written raw to the PTY (compat path).

When either task ends the other is aborted, the PTY is un-paused, and the session **keeps running**.

**Parked session** (`pty == None`): send `connected`, drain an optional resize (600 ms), replay `scrollback/<id>.bin`, print `[TermCrew] Session is parked (backend restarted). Press Restart to relaunch this agent.`, send `OP_EXIT(0)`, then hold the socket open until the client closes.

**Client side** (`ptySocket.ts`): DEV connects to `ws://127.0.0.1:3001/ws/{id}` directly (Vite's WS proxy resets on Windows loopback); production uses `window.location.host`. ACKs every `ACK_EVERY = 32 KiB` after xterm has written the bytes. Reconnects with backoff `min(2000, 200·2^min(attempt,4))` ms until disposed. `pause()`/`resume()` exist but are not called by the UI.

### 3.6 Persistence (`persist.rs`, `worktree.rs`, `workdirs.rs`, `skills.rs`)

App-data root (`worktree::app_data_dir`):

| OS | Root |
| --- | --- |
| Windows | `%LOCALAPPDATA%\termcrew` (fallback `%USERPROFILE%\AppData\Local\termcrew`) |
| Unix/macOS | `$XDG_DATA_HOME/termcrew` or `~/.local/share/termcrew` |

If `termcrew` is absent and a legacy `multiagent` directory exists, it is renamed once. The same one-shot migration applies to `~/.multiagent` → `~/.termcrew`.

| Data | Location | Format / notes |
| --- | --- | --- |
| Session metadata | `{root}/sessions/<id>.json` | pretty JSON `PersistedSession`; hidden sessions never written |
| Scrollback | `{root}/scrollback/<id>.bin` | raw bytes, ≤ 128 KiB; flushed every 10 s while alive, on exit, on park |
| Worktrees | `{root}/worktrees/<12 hex>/agent-<id>/` | `<12 hex>` = FNV-1a 64 of the canonical repo path (lower-cased on Windows), first 12 hex digits |
| Skills prefs | `{root}/skills-prefs.json` | `{ "disabled": [absolute skill paths] }`, atomic via `.tmp` rename |
| Skill git installs (temp) | `{root}/skill-install/<uuid>/` | shallow clone, deleted after copy |
| Recent workdirs | `~/.termcrew/recent_workdirs.json` (`%USERPROFILE%` on Windows) | `[{ path, last_used_at, use_count }]`, max 12, pruned of missing folders on load |

Session ids are validated (`safe_id`: 1–80 chars of `[A-Za-z0-9_-]`) before being used as filenames, and a file is only loaded if its stem equals the id it contains.

`GET /api/storage` returns these paths; `POST /api/storage/open` opens the root in Explorer/Finder/xdg-open.

### 3.7 Worktrees (`worktree.rs`)

- `ensure_git_repo(base)`: if `base` is not inside a work tree, `git init`; if `HEAD` has no commit, creates an empty initial commit so `worktree add` can branch.
- `create_worktree(base, agent_id)`: `git worktree prune`, `git branch -D branch-{agent_id}` (ignore failure), `git worktree add -b branch-{agent_id} {root}/worktrees/{hash}/agent-{agent_id}`.
- `remove_worktree(base, agent_id)`: `git worktree remove --force …`, `git branch -D …`.
- `get_diff(dir)`: `git diff HEAD`, then appends `git status --short` under a `--- Untracked / Staged Status ---` header when that output is non-empty.
- Worktrees live **outside** the user's repository, so no `.gitignore` edits are needed (`worktree_lifecycle_in_app_data_dir_without_gitignore` test).

### 3.8 Agent registry (`registry.rs`)

`get_supported_agents()` returns 28 agent definitions plus shells, sorted agents-first then by name:

| id | name | binary | notes |
| --- | --- | --- | --- |
| `claude` | Claude Code | `claude` | default args `--settings {"theme":"auto"}` |
| `codex` | Codex | `codex` | |
| `gemini` | Gemini CLI | `gemini` | |
| `agy` | Antigravity CLI | `agy` | |
| `cursor-agent` | Cursor Agent | `agent` | also probes `cursor-agent` |
| `amp` | Amp | `amp` | |
| `grok` | Grok Build | `grok` | no uninstall command |
| `kiro` | Kiro | `kiro-cli` | also probes `q`, `kiro` |
| `opencode` | OpenCode | `opencode` | |
| `openclaw` | OpenClaw | `openclaw` | |
| `pi` | Pi | `pi` | |
| `hermes` | Hermes Agent | `hermes` | |
| `aider` | Aider | `aider` | |
| `goose` | Goose | `goose` | |
| `cline` | Cline CLI | `cline` | |
| `crush` | Crush | `crush` | |
| `qwen` | Qwen Code | `qwen` | |
| `kimi` | Kimi Code | `kimi` | |
| `plandex` | Plandex | `plandex` | Windows: no installer (WSL hint) |
| `openhands` | OpenHands | `openhands` | |
| `interpreter` | Open Interpreter | `interpreter` | |
| `continue` | Continue CLI | `cn` | |
| `kilo` | Kilo Code | `kilo` | |
| `vibe` | Mistral Vibe | `vibe` | |
| `forge` | ForgeCode | `forge` | |
| `gptme` | gptme | `gptme` | |
| `codewhale` | Codewhale | `codewhale` | also probes `codew`, `codewhale-tui` |
| `deepseek` | DeepSeek Harness | `dsh` | default args `web`; also probes `deepseek` |

Shells: `shell` (PowerShell on Windows, zsh on macOS, bash on Linux) and, on Windows, `cmd`, `git-bash` (`bash.exe --login -i`), `wsl`; on Unix, the non-default of `bash`/`zsh` plus `sh`.

Detection (`detect_agents`): `which::which`, then `where.exe` on Windows, then a probe of `extra_bin_dirs()` — a curated list of vendor install locations (`%LOCALAPPDATA%\agy`, `\cursor-agent`, `\Programs\{OpenAI\Codex\bin, CodeWhale\bin, Forge}`, WinGet `Links` and `Packages\*\*`, `%APPDATA%\npm`, `~/.local/bin`, `~/.amp/bin`, `~/.bun/bin`, `~/.forge/bin`, `~/go/bin`, `~/scoop/shims`, `~/bin`, …). On Windows a real `.exe/.cmd/.bat/.ps1` is preferred over an npm `#!` shim. Results are cached for 2 s; `?fresh=true` bypasses the cache. Each `DetectedAgent` carries `install_cmd/update_cmd/uninstall_cmd`, `manage_hint`, and an HTTPS `docs_url` (asserted by the `every_agent_has_https_docs_url` test).

Icons are not served by the backend; `AgentMark.svelte` loads `/agents/{id}.svg|.png` from `frontend/public/agents/` and falls back to a lucide `Bot` glyph.

### 3.9 Skills (`skills.rs`)

Roots scanned (`build_roots`):

| Scope | Path | Writable |
| --- | --- | --- |
| user | `~/.agents/skills`, `~/.config/agents/skills` | yes |
| user | `~/.claude/skills`, `~/.codex/skills`, `~/.cursor/skills`, `~/.config/opencode/skills`, `~/.openclaw/skills`, `~/.hermes/skills`, `~/.gemini/skills`, `~/.kiro/skills`, `~/.config/goose/skills` | yes |
| system | `~/.codex/skills/.system`, `~/.agents/skills/.system` | **read-only** |
| project | `{git root or workdir}/.claude/skills`, `.agents/skills`, `.cursor/skills`, `.codex/skills` | yes |

Folder names `skills-cursor`, `.system`, `node_modules`, `.git` are reserved: skipped or read-only. A skill is a directory containing `SKILL.md`; name/description come from its YAML frontmatter. Scan results are cached for `SCAN_CACHE_TTL = 8 s`.

Operations and guards:

| Operation | Guard |
| --- | --- |
| enable/disable | prefs file only; never touches harness folders |
| preview | `MAX_PREVIEW_BYTES = 256 KiB`, `truncated` flag |
| copy | target root must be writable; skill name ≤ 80 chars, safe charset; symlinks not followed |
| delete | body `confirm` must equal `"DELETE"`; path must be inside a managed root; `..` rejected |
| install `local` | copy directories containing `SKILL.md` from a local path |
| install `git` | `git clone --depth 1` with hooks disabled into `{root}/skill-install/<uuid>`, copy, delete temp; URL charset validated |
| marketplace search | `curl` to `https://skills.sh/api/search?q=…` (empty query browses with `q=skill`). Browse catalog is prefetched at startup and cached 10 min (multi-query, browse slot kept). Later searches filter that list when it matches, else hit skills.sh. Then sorted (`hot` = API order, `trending`/`all-time` = installs) and paged 9 |
| marketplace install | builds `npx -y skills add <source> -y [-g] -a <agent> [-s <skill>] --copy`. Global is `-a universal` (one shared `~/.config/agents/skills`); a named harness is that CLI only. |

### 3.10 File system endpoints (`workdirs.rs`, `file_editor.rs`)

- `GET /api/fs/dirs?path=` — subdirectories only (launcher folder browser). Empty path → filesystem root (first drive on Windows).
- `GET /api/fs/list?path=` — directories first, then files (sidebar tree). No ignore list; `node_modules` and `.git` are listed.
- `GET /api/fs/file?path=` — refuses directories, files > `MAX_FILE_BYTES = 2 MiB`, and binary content (NUL byte or invalid UTF-8 in the first 8 KiB). Returns `{ path, content, size, modified_ms }`.
- `PUT /api/fs/file` `{ path, content, expected_modified_ms? }` — only writes files that already exist (no create); rejects oversized or binary-looking content; if `expected_modified_ms` is set and the file's mtime differs, refuses with the message "This file changed on disk since you opened it…". The handler maps every error to **409 Conflict**. Returns `{ path, modified_ms }`.
- `POST /api/fs/create` `{ root, rel_path, kind: file\|dir }` — creates under `root` (session tree). `rel_path` may nest (`src/lib/util.ts`); parents are created; `..`, absolute paths, and names with `<>:"|?*` are rejected. Existing paths fail. Returns `{ path, kind }`.
- `POST /api/fs/delete` `{ root, path }` — deletes a file or folder strictly inside `root`. Refuses the root itself and paths outside it.
- `POST /api/fs/pick-folder` — native OS folder dialog via `rfd` (Explorer / Finder). Linux returns an error (type the path). `{ path: null }` on cancel. The backend raises that dialog above the browser (Windows: TOPMOST + foreground steal; macOS: System Events `frontmost`).
- `POST /api/fs/open` `{ path }` — opens that folder in Explorer (Windows) / Finder (macOS) / xdg-open (Linux).

None of these endpoints constrain paths to the workspace; they trust the local operator (see §6).

---

## 4. REST API reference

All routes are under the CORS layer; bodies and responses are JSON. Errors are `{ "error": "…" }`, except kill / restart / handoff / broadcast / rename / diff which return `{ "success": false, "error": "…" }` (launch returns plain `{ "error" }`).

| Method | Path | Body / query | 200 response | Error |
| --- | --- | --- | --- | --- |
| GET | `/api/agents` | `?fresh=bool` | `DetectedAgent[]` | — |
| POST | `/api/agents/setup` | `{ agent_id, action: install\|update\|uninstall }` | `{ session }` (hidden) | 400 |
| GET | `/api/skills` | `?fresh=&workdir=` | `SkillsCatalog { roots, skills, workdir }` | — |
| GET | `/api/skills/content` | `?path=&workdir=` | `{ path, content, truncated }` | 400/500 |
| POST | `/api/skills/prefs` | `{ path, enabled }` | prefs | 400/500 |
| POST | `/api/skills/copy` | `?workdir=` + `{ source_path, target_root_id, name? }` | `{ skill }` | 400/500 |
| POST | `/api/skills/delete` | `?workdir=` + `{ path, confirm: "DELETE" }` | `{ ok: true }` | 400/500 |
| POST | `/api/skills/install` | `?workdir=` + `{ source: local\|git, path_or_url, target_root_id, name? }` | local: `{ skills }`; git: `{ session, command }` (hidden setup PTY) | 400/500 |
| POST | `/api/skills/open` | `{ path, workdir? }` | `{ ok: true }` | 400/500 |
| GET | `/api/skills/marketplace` | `?q=&view=hot\|trending\|all-time&page=&per_page=` | `{ query, view, page, per_page, total, has_more, skills[] }` | 400/500 |
| POST | `/api/skills/marketplace/install` | `{ source, skill?, harness, global? }` | `{ session, command }` | 400 |
| GET | `/api/workspace` | — | `{ path }` (server cwd) | 500 |
| GET | `/api/sessions` | — | `SessionInfo[]` (non-hidden, newest first) | — |
| POST | `/api/sessions/launch` | `LaunchRequest { preset, engine, engines?, base_dir, task?, count?, reviewer_engine? }` | `{ sessions }` | 400 |
| POST | `/api/sessions/add` | `{ group_id, engine }` | `{ session }` | 400 |
| POST | `/api/sessions/:id/kill` | — | `{ success, session_id }` | 404 |
| POST | `/api/sessions/:id/restart` | — | `{ success, session }` | 404 |
| POST | `/api/sessions/:id/rename` | `{ label }` | `{ success, label }` | 400/404 |
| POST | `/api/sessions/handoff` | `{ source_session_id, target_session_id }` | `{ success }` | 400 |
| POST | `/api/sessions/broadcast` | `{ input, session_ids? }` | `{ success, delivered_count }` | 500 |
| POST | `/api/sessions/rename` | `{ group_id, label }` | `{ success, label }` | 400 |
| GET | `/api/worktrees/diff` | `?dir=` (default `.`) | `{ success, diff }` | 400 |
| GET | `/api/workdirs` | — | `RecentWorkdir[]` | — |
| GET | `/api/fs/dirs` | `?path=` | `{ path, parent: string\|null, entries: [{ name, path, kind: "dir" }] }` | 400 |
| GET | `/api/fs/list` | `?path=` | same shape; `kind` is `"dir"` or `"file"`, dirs first | 400 |
| GET | `/api/fs/file` | `?path=` | `{ path, content, size, modified_ms }` | 400 |
| PUT | `/api/fs/file` | `{ path, content, expected_modified_ms? }` | `{ path, modified_ms }` | 409 (all write errors) |
| POST | `/api/fs/create` | `{ root, rel_path, kind: file\|dir }` | `{ path, kind }` | 400 |
| POST | `/api/fs/delete` | `{ root, path }` | `{ ok: true }` | 400 |
| POST | `/api/fs/pick-folder` | `{ title? }` | `{ path \| null }` | 409 |
| POST | `/api/fs/open` | `{ path }` | `{ ok: true }` | 400 |
| GET | `/api/storage` | — | `{ root, sessions, scrollback, worktrees, recents }` | — |
| POST | `/api/storage/open` | — | `{ ok: true }` | 409 |
| GET | `/ws/:session_id` | WebSocket upgrade | see §3.5 | JSON error + Close |

The frontend client (`api.ts`) uses relative URLs; `getSessions` throws on failure while most other calls return safe defaults (e.g. `DEFAULT_AGENTS` when the backend is down).

---

## 5. Frontend architecture

### 5.1 Shell and state (`App.svelte`)

Single root component using Svelte 5 runes; no store library and no router.

- `$state`: `agents`, `sessions`, `workspacePath`, modal flags (`isLauncherOpen`, `isRegistryOpen`, `isSkillsOpen`, `isSupervisorOpen`), `launcherAgentId`, `openFiles` / `activeFile` / `dirtyFiles`, `focusedSessionId`, `focusedGroupId`, `clearEpoch`/`clearSessionId`, `notification`, `pendingDelete`, `restartingIds`.
- `$derived`: `activeGroupSessions` (focused group, launch order = `created_at` ascending), `focusedSession`, `fileTreeRoot` (focused session's `worktree_path ?? working_dir`, else workspace), `runningCount`.
- Data: `onMount` → `getAgents`, `getSessions`, `getWorkspacePath`; then `setInterval(5000)` re-fetches sessions (skipped while any restart is in flight).
- Restart UX: mark id restarting → `restartSession` → remove and re-add the session after `tick()` so the pane remounts and reconnects → clear flag.

Layout: 56 px header (brand, running/total badge, Agents / Skills / Goals) · 240 px `Sidebar` · center `TerminalGrid` (session chips + file tabs; Monaco overlays panes) + `BroadcastBar` · `SupervisorCockpit` overlay · modals (`LauncherModal`, `AgentRegistryModal`, `SkillsRegistryModal`, kill-confirm overlay) · toast.

### 5.2 Components

| Component | Responsibility | Backend calls |
| --- | --- | --- |
| `TerminalGrid` | empty state, session + file strip, One/Split, CSS grid, maximize; file overlay | — |
| `TerminalPane` | one xterm (WebGL → Canvas → DOM), fit/resize, status, agent mark + short name + role (not `Builder · agy [Builder]`), abbreviated path (click copies), copy, send review, clear, restart, kill | WS |
| `SetupTerminal` | embedded xterm for hidden setup consoles; `OP_SETUP` → rescan | WS only |
| `Sidebar` | Sessions tab (groups, agent marks on nodes, group + node rename, kill) / Files tab (abbreviated workdir + copy + open, `FileTree`), storage footer | `getStorageInfo`, `openStorageFolder`, `openFolder` |
| `FileTree` | lazy directory tree | `listDir` |
| `FileEditor` | Monaco overlay, per-tab models, dirty, Ctrl+S mtime conflict, Esc | `readFile`, `writeFile` |
| `LauncherModal` | ~720 px; folder, then 4-across presets, count stepper (1–6, above agents on Solo/Swarm), ordered agent picker, task | `getRecentWorkdirs`, `browseDirs`, `pickFolderNative` |
| `AgentRegistryModal` | search/filter, install/update/remove via `SetupTerminal`, launch | `startAgentSetup`, `killSession` |
| `SkillsRegistryModal` | installed catalog (show/hide, GFM preview + raw + copy, copy/delete, open), local/git install, marketplace browse (Hot/Trending/Most viewed, 9 per page) + search + install | all `/api/skills*` |
| `MarkdownPreview` | Preview/Raw toggle, copy raw, YAML frontmatter table + GFM (`marked`) | — |
| `BroadcastBar` | This/All target, quick commands, clear, close group | via App → `broadcastMessage` |
| `SupervisorCockpit` | role playbook (build/review/shell), Send review, detected tests, real send log | `broadcastMessage`, `handoffReview`, `listDir`/`readFile` |
| `AgentMark` | agent logo with fallback | — |

Unused: `lib/Counter.svelte`, `src/assets/*` (Vite scaffold), `bits-ui` dependency, `api.getDiff`, `api.createSessionWs`.

### 5.3 Terminal rendering

`@xterm/xterm` 6 with `FitAddon`; tries `WebglAddon` (disposed on context loss), falls back to `CanvasAddon`, then DOM. Options: `scrollback: 3000`, bar cursor, blink, 13 px IBM Plex Mono / Chivo Mono, line-height 1.35, `phosphorXtermTheme`. The pane waits for a real size (≥ 40 cols) before opening the socket so the backend's resize-before-replay logic works; a `ResizeObserver` (16 ms debounce) refits and sends `OP_RESIZE`. Clipboard: right-click pastes; Ctrl/Cmd+C copies when there is a selection or Shift is held; Ctrl/Cmd+V pastes. No copy-on-select and no link handling.

### 5.4 Build and tooling

- `vite.config.ts`: port 5173; `/api` proxied to `127.0.0.1:3001` with a keep-alive `http.Agent` (avoids ECONNRESET on Windows loopback); no `/ws` proxy by design; alias `monaco-editor-css` → `monaco-editor/min/vs/editor/editor.main.css`.
- Monaco workers imported with `?worker` (editor, json, css, html, ts).
- Tailwind 4 via `@tailwindcss/vite`; design tokens declared in `app.css` `@theme`.
- TypeScript strict (`@tsconfig/svelte`), `verbatimModuleSyntax`.
- Scripts: `dev`, `build`, `preview`, `check` (svelte-check + tsc for node config), `icons:check` (verifies every `vscode-icons:*` name in `fileIcons.ts` exists in the Iconify collection).
- `frontend/dist/` is a local build artifact and is gitignored.

---

## 6. Security model

TermCrew is a **local developer tool**; the threat model assumes the operator owns the machine.

| Control | Present |
| --- | --- |
| Loopback-only bind | yes (`127.0.0.1:3001`) |
| CORS restricted to `localhost:5173` / `127.0.0.1:5173` | yes (REST only) |
| Authentication / CSRF tokens | **no** — any local process or any page that can reach loopback with a permitted Origin (or a non-browser client) can call the API |
| Session id sanitisation before filesystem use | yes (`safe_id`) |
| Skills path containment, `..` rejection, reserved-folder protection, `DELETE` confirm | yes |
| Git clone hooks disabled during skill install | yes |
| Command-injection checks on marketplace source / skill names | yes (charset allowlists, tested) |
| Workspace containment for `/api/fs/*`, `/api/worktrees/diff`, launch `base_dir` | **no** — any path the OS user can access |
| Arbitrary command execution paths | agent install/update/uninstall scripts (vendor-defined), `npx skills add …`, `launch_command_setup` (internal caller only, ≤ 4000 chars) |
| Outbound network | only `curl https://skills.sh/api/search` and whatever installers/agents do themselves |

Consequences: do not expose port 3001 beyond loopback (e.g. via port-forwarding or `0.0.0.0`) without adding authentication.

---

## 7. Cross-cutting invariants

Things that must stay true; several tests encode them.

1. **Killing a session kills the whole process tree** (Job Object / pgid). Never spawn agents outside `PtyManager`.
2. **Resize before replay.** The WS handler waits for a resize before dumping scrollback; the pane must send `OP_RESIZE` on open.
3. **CORS never wraps `/ws`.**
4. **Hidden sessions** (setup consoles) never appear in `/api/sessions`, are never persisted, and are dropped on park.
5. **Worktrees live in app-data**, never inside the user's repo; `kill` removes them, `restart` preserves them.
6. **Persisted ids pass `safe_id`**; nothing user-controlled becomes a filename without it.
7. **Setup sentinels** `__MA_SETUP__:ok|fail` are stripped from everything the browser sees and converted to `OP_SETUP`.
8. **Windows setup scripts run from a temp `.ps1` file**, not inline `-Command` (Defender heuristics).
9. **Restart keeps the session id**, so open sockets reconnect to the same URL.

---

## 8. Build, run, test

```powershell
# backend
cd backend; cargo run            # → Server listening on http://127.0.0.1:3001
cd backend; cargo test           # 45 unit tests + 6 integration tests (see note)

# frontend
cd frontend; bun install; bun run dev     # → http://localhost:5173
cd frontend; bun run check                # svelte-check + tsc
cd frontend; bun run icons:check
cd frontend; bun run build                # → frontend/dist (not served by backend)
```

Windows convenience: `start.bat` (two cmd windows) or `run-dev.ps1` (waits for `GET /api/agents` to return 200 before starting Vite). Windows locks `termcrew.exe` while it runs — stop it before `cargo run`.

Known state of the checks at 1.0.0 (see [AGENTS.md](AGENTS.md) for details):

- `cargo test`: 45/45 unit tests pass; `test_preset_workbench_session_lifecycle` and `test_agent_setup_console_is_hidden` **fail on a machine that has parked sessions on disk**, because `create_app_state()` loads the real `%LOCALAPPDATA%\termcrew\sessions` into the test's state. The other four integration tests pass.
- `bun run check`: 0 errors, 0 warnings.

---

## 9. Tunable constants (single source of truth is the code)

| Constant | Value | File |
| --- | --- | --- |
| Backend bind | `127.0.0.1:3001` | `main.rs` |
| Frontend dev port | `5173` | `vite.config.ts` |
| Scrollback per session | 128 KiB | `pty_manager.rs` |
| PTY read buffer | 32 KiB | `pty_manager.rs` |
| Broadcast channel capacity | 8192 | `pty_manager.rs` |
| WS flow control high / low | 256 KiB / 32 KiB | `ws_handler.rs` |
| WS coalesce | 64 KiB | `ws_handler.rs` |
| Resize wait / settle | 600 ms / 120 ms | `ws_handler.rs` |
| Client ACK granularity | 32 KiB | `ptySocket.ts` |
| Client reconnect backoff | 200 ms·2^n, cap 2 s | `ptySocket.ts` |
| xterm scrollback | 3000 lines (4000 in setup consoles) | `TerminalPane.svelte`, `SetupTerminal.svelte` |
| Session poll interval | 5 s | `App.svelte` |
| Scrollback flush interval | 10 s | `session.rs` |
| Pane count clamp (solo/swarm) | 1–10 backend, 1–6 in UI stepper | `session.rs`, `LauncherModal.svelte` |
| Swarm default count | 3 backend, 4 UI | `session.rs`, `LauncherModal.svelte` |
| Handoff review cap | 6 KiB | `session.rs` |
| Group label max | 80 chars | `session.rs` |
| Setup command max | 4000 chars | `session.rs` |
| Agent detection cache | 2 s | `registry.rs` |
| Skills scan cache | 8 s | `skills.rs` |
| Skill preview cap | 256 KiB | `skills.rs` |
| Skill name max | 80 chars | `skills.rs` |
| Editor file cap | 2 MiB | `file_editor.rs` |
| Recent workdirs | 12 | `workdirs.rs` |
| Launch task inject | wait for PTY quiet 450 ms, cap 12 s; theme `\r` then 2 s settle | `session.rs` |
