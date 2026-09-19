# AGENTS.md — working in the TermCrew repository

Instructions for AI coding agents (and humans) making changes here. Read this before touching code. The project-level Cursor rule `.cursor/rules/rebuild-rerun.mdc` also applies (summarised in §3). Behavioural guidelines (§0) are maintained as a user-level Cursor rule (local plugin `%USERPROFILE%\.cursor\plugins\local\sujit-global\rules\karpathy-guidelines.mdc`) on the owner's machine; they are restated here so agents running outside Cursor follow the same standard.

## 0. How to behave (Karpathy guidelines, condensed)

1. **Think before coding.** State assumptions; if several interpretations exist, present them instead of picking silently; if a simpler approach exists, say so; if something is unclear, stop and ask.
2. **Simplicity first.** Minimum code that solves the problem — no speculative features, no abstractions for single-use code, no configurability nobody asked for, no error handling for impossible cases. If a senior engineer would call it over-complicated, simplify.
3. **Surgical changes.** Touch only what the request needs; do not "improve" adjacent code, comments or formatting; match existing style; remove only the imports/variables your change orphaned; mention unrelated dead code, do not delete it.
4. **Goal-driven execution.** Turn the task into a verifiable goal (a failing test that then passes, a check that goes green, a log line that appears), state the plan as steps with a verification per step, and loop until verified. For this repo the concrete checks are in §6–§7.

## 1. What this project is

TermCrew is a local web console that runs many AI coding CLIs in real PTYs side by side. Two processes:

| Part | Path | Stack | Port |
| --- | --- | --- | --- |
| Backend | `backend/` (crate `termcrew`) | Rust edition 2024, Axum 0.7, tokio, portable-pty | `127.0.0.1:3001` |
| Frontend | `frontend/` | Svelte 5 (runes), Vite 8, Tailwind 4, TypeScript strict, xterm.js 6, Monaco | `localhost:5173` |

Reference documents — keep them in sync with code changes:

- [ARCHITECTURE.md](ARCHITECTURE.md) — modules, REST/WS protocol, persistence, constants. Update when you add/rename an endpoint, module, opcode, file format or constant.
- [PRD.md](PRD.md) — requirements with stable ids (`S-1`, `T-3`, …). Update when behaviour changes; cite ids in commits.
- [DESIGN.md](DESIGN.md) — UX and technical design decisions. Update when you change a flow, a token, or a protocol decision.
- [README.md](README.md) — user-facing quick start. Keep the feature table and agent list truthful.

## 2. Commands

```powershell
# Backend (from backend/)
cargo run                  # dev server; wait for "Server listening on http://127.0.0.1:3001"
cargo test                 # 45 unit + 6 integration tests (see §6 for two env-sensitive ones)
cargo test --lib           # unit tests only — always green

# Frontend (from frontend/)
bun install
bun run dev                # Vite on :5173, proxies /api to :3001 (WS goes direct to :3001)
bun run check              # svelte-check + tsc
bun run icons:check        # verifies vscode-icons names used in fileIcons.ts exist
bun run build              # → frontend/dist (gitignored; not served by the backend)
```

Windows only: `start.bat` / `run-dev.ps1` open the two servers in separate windows. **Do not run `run-dev.ps1` from an agent** — it opens windows and blocks. Start servers in the background yourself.

## 3. Rebuild and rerun discipline (summary of `.cursor/rules/rebuild-rerun.mdc`)

- Backend edits require a rebuild: Windows locks `termcrew.exe`. `Stop-Process -Name termcrew -Force` (ignore if missing), then `cargo run` from `backend/`, then confirm the listen log or `GET http://127.0.0.1:3001/api/sessions` returns 200.
- Frontend edits: Vite HMR is enough, except for `vite.config.ts`, `svelte.config.js`, or `package.json` (restart `bun run dev`).
- Never start a second `cargo run` while an old backend holds :3001. Never claim the app is running without a fresh check.
- Stopping the backend **parks** live sessions (they are relaunchable) — acceptable during development, but tell the user if they had agents running.

## 4. Code map (where to change what)

| Task | Touch |
| --- | --- |
| Add a REST endpoint | `backend/src/main.rs` (route + handler) → module function → `frontend/src/lib/api.ts` (typed client) → `types.ts` if a new shape → ARCHITECTURE §4 |
| Change session behaviour, presets, broadcast, handoff, restart, park, add pane | `backend/src/session.rs` (+ `LauncherModal.svelte` / `TerminalGrid.svelte` if user-facing) |
| Goals playbook (role contracts, review sends, test detect) | `frontend/src/lib/playbook.ts`, `SupervisorCockpit.svelte` |
| PTY spawn / read loop / scrollback size | `backend/src/pty_manager.rs` |
| Process-tree kill | `backend/src/proc_guard.rs` (Windows Job Object / Unix pgid) |
| WebSocket framing or flow control | `backend/src/pty_wire.rs` + `ws_handler.rs` **and** `frontend/src/lib/ptySocket.ts` — the opcode table must match on both sides |
| Persistence paths / formats | `backend/src/persist.rs`, `worktree.rs::app_data_dir`, `workdirs.rs` |
| Add or fix an agent CLI | `backend/src/registry.rs`: `get_supported_agents` (definition), `agent_lifecycle` (per-OS install/update/uninstall), `agent_docs_url` (HTTPS required — a test enforces it), `alternate_binaries`, `extra_bin_dirs`, `needs_theme_auto_confirm`; add `frontend/public/agents/<id>.svg|png`; update the README agent list and ARCHITECTURE §3.8 table |
| Skills roots / operations | `backend/src/skills.rs` (`build_roots`, guards) and `SkillsRegistryModal.svelte` |
| Editor / file tree | `backend/src/file_editor.rs`, `workdirs.rs::list_dir_with_files`, `FileEditor.svelte`, `FileTree.svelte`, `fileIcons.ts` (run `bun run icons:check`); create via `POST /api/fs/create` |
| Open a folder in Explorer/Finder | `persist.rs::open_dir` + `POST /api/fs/open` → `api.ts::openFolder` |
| Visual tokens | `frontend/src/app.css` (`@theme`), `frontend/src/lib/ui.ts` (`btn`, `badge`, `statusColor`, xterm theme), `monaco.ts` theme |
| App shell state, polling, focus, modals | `frontend/src/App.svelte` |

## 5. Conventions

### Rust

- Edition 2024. Handlers are thin: parse → call module fn → map `Result<T, String>` to `(StatusCode, Json)`. Blocking work (`git`, filesystem scans, `which`) goes through `tokio::task::spawn_blocking`.
- Errors are human sentences intended for a toast ("Working directory does not exist: …"). Keep that style.
- Logging via `tracing` (`info!`, `warn!`, `error!`) with structured fields (`session_id = %id`).
- Unit tests live in a `#[cfg(test)] mod tests` at the bottom of each module; cross-module behaviour goes in `backend/tests/integration_tests.rs`. Tests that touch the filesystem must use `std::env::temp_dir()` + a uuid and clean up.
- Do not add `unwrap()` on I/O in request paths; propagate a `String` error.
- Keep `Cargo.toml` features minimal; note that `tower-http`'s `fs` feature is enabled but unused today.

### Svelte / TypeScript

- Svelte 5 runes only (`$state`, `$derived`, `$effect`, `$props`); no stores, no legacy `export let`. Callback props (`onClose`, `onLaunch`) instead of `createEventDispatcher`.
- All backend calls go through `frontend/src/lib/api.ts`; components never call `fetch` directly. Add types to `types.ts` or export them from `api.ts` as the existing code does.
- Tailwind utility classes with the project tokens (`bg-ink-900`, `text-fog`, `border-line`, `text-phosphor`). Do not introduce raw hex colours in components; add a token in `app.css` instead. Use `cn()` and the `btn`/`badge` variants from `ui.ts`.
- Icons: lucide. Prefer `@lucide/svelte` for new code (both packages are present; do not add a third).
- `TerminalGrid` uses one `{#each}` keyed by session id. Hidden panes stay mounted with `invisible` so One ↔ Split and maximize do not drop sockets.
- Colour rule baseline: dirty gold (`bg-[#e2b93d]`) is the only raw hex — file tabs and the editor save bar. Do not add more.
- Keep `frontend/dist` out of git (already ignored).

### Cross-cutting invariants (breaking these causes real bugs)

1. **Opcode protocol parity** — `pty_wire.rs` and `ptySocket.ts` must agree byte-for-byte. Add opcodes to both and to ARCHITECTURE §3.5.
2. **Resize before replay** — the pane must send `OP_RESIZE` right after the socket opens; the server waits ≤ 600 ms for it before replaying scrollback. Do not remove either side.
3. **CORS never wraps `/ws`** — the CORS layer is applied to the `api` router only; merging `/ws` under it breaks the upgrade.
4. **Hidden sessions** (`hidden: true`, preset `Setup`) must never be listed, persisted, or counted toward group ordinals.
5. **Whole-tree kill** — spawn child processes only via `PtyManager` so `ProcessGuard` covers them.
6. **Filesystem safety** — session ids pass `persist::safe_id`; skill paths pass containment checks in `skills.rs`. Mirror those guards for any new file-writing endpoint.
7. **Setup sentinels** `__MA_SETUP__:ok` / `__MA_SETUP__:fail` are the contract between setup scripts and the UI. If you change them, update `pty_wire.rs`, `session.rs` script builders, and their tests.
8. **Windows setup scripts run from a temp `.ps1`** (`-File`), never inline `-Command` — this is a Defender workaround, enforced by `setup_shell_uses_temp_ps1_file_not_inline_command`.
9. **Restart preserves the session id and worktree**; kill deletes both. Do not swap those semantics.
10. **Loopback only** — do not change the bind address or widen CORS without adding authentication (see PRD §9).

## 6. Test status you should know about (1.0.0)

- `cargo test --lib`: 45 tests, all pass.
- `cargo test --test integration_tests`: 4 pass; **`test_preset_workbench_session_lifecycle` and `test_agent_setup_console_is_hidden` fail on any machine that has parked sessions on disk**, because `session::create_app_state()` loads `%LOCALAPPDATA%\termcrew\sessions\*.json` into the test state. They are not product regressions. To get a clean run, temporarily move that folder aside, or (better) implement a data-root override (`TERMCREW_DATA_DIR`) and use it in tests — a tracked improvement.
- `bun run check`: 0 errors, 0 warnings.
- Integration tests spawn real shells (PowerShell on Windows) and run `git`; they need those on PATH.

When you change behaviour, add or update a test at the same level (unit for pure logic, integration for spawn/kill/persist paths).

## 7. Definition of done for a change

1. `cargo test --lib` green; integration tests green or unchanged-failing for the documented reason only.
2. `bun run check` produces no errors or warnings. `bun run icons:check` green if you touched `fileIcons.ts`.
3. Backend restarted on the new build, listen log confirmed; Vite on :5173.
4. Manually exercised the affected flow in the browser (launch → type → kill for anything touching sessions or the PTY path).
5. ARCHITECTURE / PRD / DESIGN / README updated if any endpoint, constant, flow, or user-facing copy changed.
6. No new hex colours, no new `fetch` outside `api.ts`, no new global state outside `App.svelte`.
7. Commit message references PRD ids where applicable (e.g. `fix(session): S-11 keep worktree on restart`).

## 8. Things not to do

- Do not "fix" the README's `Swarm (4–6)` by changing backend clamps silently; the discrepancy is a product decision (PRD §10).
- Do not add a `/ws` proxy to `vite.config.ts` — it resets on Windows loopback; the direct URL is intentional.
- Do not delete `docs/superpowers/specs/*`; they are the design history.
- Do not commit agent-tool artefacts. `backend/.crush/` is self-ignored; `backend/.codewhale/` currently is **not** ignored and shows as untracked — add it to `.gitignore` rather than committing it.
- Do not commit `frontend/dist` or `backend/target`. Note that the root `.gitignore` currently ignores `Cargo.lock`; for a binary crate the lockfile should normally be committed for reproducible builds — raise it with the owner rather than flipping it silently.
- Do not reformat files you did not otherwise change; match existing style.

## 9. Glossary

| Term | Meaning |
| --- | --- |
| Session | One PTY running one engine in one folder; has id, role, group |
| Group | All sessions from one launch; labelled `{Preset} n` (e.g. `Pair 1`) |
| Engine | Agent id (`claude`, `codex`, …) or shell id (`shell`, `cmd`, `git-bash`, `wsl`, `bash`, `zsh`, `sh`) |
| Preset | `Solo`, `Pair`, `Workbench`, `Swarm` (public) and `Setup` (internal, hidden) |
| Parked | A session whose backend restarted; metadata + scrollback on disk, `pty: None`, relaunchable |
| Setup console | Hidden PTY running install/update/uninstall or `npx skills add`, shown inline in a modal |
| Worktree | Git worktree under `{app-data}/worktrees/{hash}/agent-{id}` used by Pair reviewers and Swarm workers |
| Sentinel | `__MA_SETUP__:ok\|fail` printed by setup scripts, converted to `OP_SETUP` |
| Opcode frame | Binary WebSocket message whose first byte selects DATA/RESIZE/PAUSE/RESUME/ACK/EXIT/SETUP |

## Learned User Preferences

- Agent install/update/uninstall commands must match each CLI's official docs — search the real docs; do not invent scripts.
- Agent icons must be the official favicon/logo from the product website or GitHub; never placeholders or lookalikes.
- PTY panes should feel like a real shell: keep each CLI's native colors, stream live, and do not inject wrapper or status sentences into the terminal.
- Abbreviate home/username in displayed paths; a click copies the full path. Do not show the backend cwd as a header chip — show the session workdir only after a session exists. Open-folder lives on the Files tab only, not on session rows or the terminal title bar.
- Agent lifecycle and shell paths must work on macOS as well as Windows — do not ship Windows-only installers without a Mac equivalent.
- While one agent install/update/uninstall is running, other lifecycle actions stay disabled until it finishes.
- Frontend package manager is bun — do not introduce pnpm or switch lockfiles.
- New session launcher starts with Folder (workdir), then layout and agents.
- Send review (handoff) sends a compact git packet (role, folder, optional task, status/diff) to the next same-group companion — no chat transcript; keep the packet small to limit the receiving agent's token/API cost.
- Launch Task waits until the agent prompt is ready, then types into the PTY — do not inject at spawn.
- Opening files uses editor tabs over the terminal grid; do not remount live PTY panes.
- Goals is a role playbook: different contracts per Lead/Review/Shell — not one broadcast to every pane, and not a TermCrew-owned planner LLM.

## Learned Workspace Facts

- App data lives under `%LOCALAPPDATA%\termcrew` (not `multiagent`); Swarm worktrees belong there, never as `.worktrees` inside the user's project. Solo / Pair / Workbench share the launch folder.
- Amazon Q was removed and replaced with Kiro after virus detections on the old installer.
- PowerShell 7 is not a registered Windows agent.
- Codex uninstall/remove targets the CLI only, not the ChatGPT desktop app.
- Launcher folder starts empty; the user must pick a recent, type a path, or Browse. Empty path does not fall back to the backend cwd.
- Roo Code and Zoo Code are not registered agents (Roo's GitHub is archived; Zoo was added then fully removed).
- Marketplace Global install is one shared `npx skills add … -g -a universal` into `~/.config/agents/skills`, not a copy into every agent. Per-agent `-a` flags are repeatable; comma-separated agent lists fail on the current CLI.
- Node (pane) display `label` is editable separately from machine `role` (Lead/Review/…).
