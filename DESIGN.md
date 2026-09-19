# TermCrew — Design

This document covers two kinds of design: the **product/UX design** (what the user sees and does) and the **technical design decisions** behind the protocol, persistence and process model (why the system is shaped the way it is). It reflects version 1.0.0 as implemented. For module-level detail see [ARCHITECTURE.md](ARCHITECTURE.md); for requirements see [PRD.md](PRD.md).

---

## Part A — Product and interface design

### A1. Design language: "Phosphor Console"

TermCrew looks like a green-on-black CRT workstation. The rationale is functional: the product's content *is* terminals, so the chrome around them should read as terminal hardware, not as a SaaS dashboard. Concretely (`frontend/src/app.css`):

- Warm green on near-black. No neon cyan/violet gradients — the legacy `--color-cyan` / `--color-violet` tokens are remapped to phosphor greens so older class names keep working.
- Sharp corners (`rounded-sm` = 4 px in Tailwind 4; the custom `.bezel` / `.glass-panel` classes use 2 px) and 1 px "bezel" borders; no drop-shadow cards.
- Signature elements: a **scanline veil** over terminal areas (`.scanlines::after`, 45 % opacity, 20 % under reduced motion) and a **phosphor pulse** on live status only (`.pulse-glow`, 2.2 s).
- Monospace everywhere, including body text; uppercase letter-spaced "eyebrow" labels for section headers.

#### Colour tokens (Tailwind 4 `@theme`)

| Token | Value | Use |
| --- | --- | --- |
| `ink-950` | `#050805` | page and terminal background |
| `ink-900` | `#0a0f0a` | header, sidebar, bezels |
| `ink-850` | `#0e150e` | inputs, secondary surfaces |
| `ink-800` | `#141c14` | hover surfaces, badges |
| `ink-700` / `ink-600` | `#1c281c` / `#2a3a2a` | pressed states, exited status, scrollbar thumb |
| `line` / `line-strong` | `#1a2a1a` / `#2d4a2d` | borders |
| `bone` | `#c8e6c9` | primary text |
| `fog` | `#6b8f6e` | secondary text |
| `dim` | `#3d5c40` | tertiary text, paths |
| `phosphor` | `#39d353` | accent, live status, focus ring, cursor |
| `phosphor-bright` | `#7cff8a` | hover accent |
| `phosphor-dim` | `#1a6b2a` | subtle accent borders |
| `alert` | `#e85d5d` | destructive actions, errors |
| `warning` | `#d4a017` | paused / caution |

The xterm palette (`phosphorXtermTheme` in `ui.ts`) mirrors these: background `#050805`, foreground `#a8d4aa`, cursor `#39d353`, ANSI green/cyan both phosphor, blue shifted to `#4a8f5a` so blue-heavy TUIs stay legible on the green theme. Monaco uses a matching `termcrew-dark` theme (`monaco.ts`).

#### Typography

| Role | Stack |
| --- | --- |
| UI (`--font-sans`) | Chivo Mono, IBM Plex Mono, ui-monospace |
| Terminal / code (`--font-mono`) | IBM Plex Mono, Chivo Mono |
| Eyebrows (`--font-display`) | Share Tech Mono, 11 px, 0.18 em tracking, uppercase |

Fonts load from Google Fonts in `index.html`. Terminal panes use 13 px / line-height 1.35; setup consoles 12 px / 1.3. UI text is dense: 10–12 px labels, 24–40 px control heights.

#### Component primitives (`ui.ts`)

- `btn` variants: `default`, `primary` (solid phosphor), `violet` (outlined phosphor — name is historical), `ghost`, `danger`, `outline`; sizes `icon` (28 px), `xs` (24 px), `sm` (32 px), `md` (36 px).
- `badge` variants: `default`, `success`, `warning`, `error`, `cyan`, `purple`.
- `statusColor`: `running` (phosphor, glowing, pulsing), `exited` (ink-600), `paused` (warning), `error` (alert).
- `displayHomePath()` redacts the user name from displayed paths (`C:\Users\me\AppData\Local\…` → `%LOCALAPPDATA%\…`, `/Users/me/…` → `~/…`); the full path stays in `title` and copy actions.

Icons: lucide for actions (both `lucide-svelte` and `@lucide/svelte` are in use), agent logos from `public/agents/{id}.svg|png` via `AgentMark` (falls back to a `Bot` glyph), VS Code file icons via Iconify for the file tree.

Motion is minimal and purposeful: status pulse, a 180 ms toast slide-up, colour transitions on hover, `active:brightness-90` on buttons. `prefers-reduced-motion` disables the pulse and toast animation.

### A2. Layout

```
┌───────────────────────────────────────────────────────────────────────────────┐
│ HEADER 56px  TermCrew v1.0 │ ● 3/4 running │ Agents  Skills  Goals            │
├──────────┬────────────────────────────────────────────────────────────────────┤
│ SIDEBAR  │ TERMINAL GRID                                                      │
│ 240px    │  strip: [Lead ×] [Review 1 ×] │ [app.py ×]  One|Split               │
│          │  ┌──────────────┐ ┌──────────────┐                                 │
│ Sessions │  │ pane         │ │ pane         │  file tab → Monaco overlays     │
│  ▾ S1    │  │  xterm       │ │  xterm       │  panes (stay mounted)           │
│    node  │  └──────────────┘ └──────────────┘                                 │
│ Files    ├────────────────────────────────────────────────────────────────────┤
│ ──────── │ BROADCAST BAR  [This|All] ▸ command…   quick ▾ 🗑                  │
│ storage  │                                                                    │
└──────────┴────────────────────────────────────────────────────────────────────┘
   modals: Launcher · Agents registry · Skills registry · Kill confirmation · toast (bottom-right)
```

- **Header**: brand and hard-coded `v1.0` label, backend working directory (the default launch folder), live `running/total` badge, and the four primary actions.
- **Sidebar** (240 px): *Sessions* tab groups sessions by launch (`group_id`), sorted by creation; each group shows a pulsing dot if any node is running, node count + preset, a rename affordance (pencil or double-click), the same rename on each node (display label only; `role` stays Lead/Review/…), and kill for node or group. Collapsed state is remembered per group; selecting a new group expands it. *Files* tab is the only place to open the session folder: abbreviated workdir (click copies) and Open folder. The tree root has New (+) before Refresh; hover a folder for the same +. One path field: `notes.txt`, `folder/file.txt`, or `docs/` (trailing slash = folder). Hover a row for trash; first click **Sure?**, second deletes (same as agent Remove). The footer shows the redacted data root with an Open button and a New session button.
- **Terminal grid**: an empty state ("New session") when nothing is focused; otherwise a strip of chips for the focused group's sessions, then any open file tabs, a **+** to add a pane to this group (same folder; existing PTYs stay up; view switches to Split so the new pane is not left behind One-mode stacking), plus a One/Split toggle. Each pane title is AgentMark + short name, plus a folder icon that copies the workdir (no path text; Open folder stays on Files). Role lives on the strip chip and the sidebar row, not again on the pane. Panes and chips follow launch order (Pair: Lead, then Review 1…n). Split uses a responsive CSS grid (1 → 1 col; 2 → 2 cols at `lg`; 3 → 3 cols; 4 → 2×2; 5–6 → 3×2; more → 3 cols). One mode stacks panes absolutely and shows the active one. The session strip stacks above the pane area so the + agent menu is not covered by the xterm canvas. One `{#each}` keyed by session id covers Split and One; hidden panes are `invisible`, so One ↔ Split and maximize do not remount sockets. A file tab overlays Monaco on the pane area without remounting or resizing PTYs; click a session chip to return.
- **Broadcast bar**: target toggle (This/All — All only when the group has > 1 node), input, quick commands, clear (wipes the unsent prompt — readline + word-delete + backspace + Del; same bytes on Win/macOS), and a trash icon that kills the focused group after confirmation.
- **Goals**: role playbook overlay — per-role contracts, git Send review, detected test command. Not a fake pipeline. The editor is a file tab over the grid.
- **Toast**: bottom-right, info/success/error, auto-dismiss after 4 s.

### A3. Key flows

#### Launch a crew

1. *New session* (sidebar footer, empty state, or Agents → Launch) opens the launcher. Default preset is **Pair**.
2. **1. Folder** — pick where agents work first: up to 5 recents, free-text path, *Browse* (native picker brought in front of the browser; falls back to an in-app directory browser). Starts empty; Launch stays off until a folder is chosen.
3. **2. Layout** — four preset cards in one row: Solo `1+` "Same folder. One terminal per agent"; Pair `2+` "Same folder. First lead, next review"; Workbench `2+` "Same folder. Agents plus a shell"; Swarm `2+` "Each worker gets its own copy".
4. **3. How many** — only when a single agent is selected and the preset is Solo or Swarm: buttons 1–6 (defaults: solo 1, swarm 4), shown above the agent grid.
5. **4. Agents** — installed agents are toggleable in click order and get a numbered badge; missing agents are disabled and dimmed with the tooltip "Install this agent first". The last selected agent cannot be deselected. A "Will start" preview lists the roles that will be created (e.g. `Lead`, `Review 1`, `(Shell)`).
6. **Task** — optional prompt typed into each agent after its chat/shell prompt is ready, then Enter.
7. `Launch` (or `Ctrl/Cmd+Enter`). The modal closes, new panes appear, the first is focused. `Esc` cancels.

Error handling: a missing binary or bad folder returns a 400 with a human sentence, shown as a toast; the backend has already rolled back any partially started panes.

#### Watch, steer, review

- Type in a pane as in any terminal; right-click pastes.
- Broadcast a prompt to the whole group with *All*; a toast confirms `Ran on N terminals.` (or `Ran on active TTY.` for *This*), or shows the error.
- *Send review* on a pane types a compact packet (role, folder, optional launch task, git status/diff, “list bugs + missing tests only”) into the **next pane in the same group**. Capped at 6 KiB — no chat transcript. The toast names the target.
- *Restart* on a pane relaunches the same engine in the same folder/worktree; the transcript is kept and a `--- relaunched ---` marker separates the runs. The pane is unmounted and remounted after the backend answers so it reconnects to the same session id; session polling pauses while any restart is in flight to avoid flicker, and a toast confirms.
- *Kill* on a chip, pane or sidebar row asks for confirmation (Enter confirms, Esc cancels), then removes the pane, its persisted files and worktree.

#### Backend restart (parked sessions)

Sessions reappear in the sidebar with their last transcript. A line with a yellow `[TermCrew]` prefix — `Session is parked (backend restarted). Press Restart to relaunch this agent.` — ends the transcript and the pane shows an exited state. *Restart* relaunches; *Kill* discards.

#### Manage agents

The Agents modal lists the catalogue with Ready/Missing state, resolved path (copyable), description and docs link. Filters: All / Ready / Missing; search; grid or list. *Install / Update / Remove* (Remove needs a second click "Sure?") opens an embedded **setup console** — a real hidden PowerShell/zsh/bash PTY under the list that shows the vendor installer's output. Remove stops any live pane of that agent first, then after the vendor command force-deletes leftover data dirs and shims if Windows/macOS still has them locked. When the script prints its sentinel the console reports success/failure, the list rescans (also every 8 s while a console is open), and *Done* closes it. Only one setup console runs at a time.

#### Manage skills

Two tabs. **Installed** lists every skill found across harness roots with harness, scope (user/project) and Shown/Hidden state; filters per harness; grid/list; actions Show/Hide, Preview (read-only overlay, truncation notice), Copy (choose a writable root), Open folder, Delete (Delete → Sure? → Confirm DELETE). An *Install* panel accepts a local folder or git URL plus target root and optional name. **Marketplace** opens on a 9-skill grid (Hot / Trending / Most viewed, next/prev pages). Browse is prefetched; typing filters the warm catalog (skills.sh only on a miss). Choose a harness (Global = one `universal` root; Claude/Codex/… = that CLI only), then `npx skills add …` in a setup console. Preview shows the markdown body (name · license as a one-line meta), not a YAML table.

#### Edit a file

Files tab → click a file → a chip appears on the session strip and Monaco overlays the pane area (PTYs stay mounted). Editing marks the tab dirty; `Ctrl/Cmd+S` saves. Click a session chip to see the terminals again. If an agent changed the file since it was opened, the save is refused with an explanation and the user reopens the file. Binary or > 2 MiB files are refused on open.

### A4. States and feedback

| State | Presentation |
| --- | --- |
| Pane connecting | banner lines (name, engine, folder, preset, "Connecting…") until the first bytes arrive, then the screen is reset and scrollback replayed |
| Pane live | `LIVE` dot (phosphor, glowing); active pane has a `ring-1 ring-phosphor/50` outline in Split view |
| Pane offline / exited | `OFFLINE` dot, status square in ink-600; content remains readable |
| Backend down | agents fall back to a static list, sessions list empties, panes report the session was not found |
| Setup running | embedded console with LIVE indicator; success/failure reflected by the rescan |
| Destructive actions | two-step (agents Remove, skills Delete) or modal confirmation (kill) |

### A5. Keyboard

| Context | Keys |
| --- | --- |
| Launcher | `Esc` close, `Ctrl/Cmd+Enter` launch |
| Kill confirmation | `Enter` confirm, `Esc` cancel |
| Editor | `Ctrl/Cmd+S` save, `Esc` close |
| Goals panel | `Ctrl/Cmd+Enter` run playbook |
| Terminal | `Ctrl/Cmd+C` copy when a selection exists or Shift is held (otherwise passes through as SIGINT), `Ctrl/Cmd+V` paste, right-click paste |

There are no global shortcuts for switching panes or opening modals yet.

### A6. Accessibility status

Focus-visible outlines are phosphor 1 px; the reduced-motion media query is honoured; icon-only buttons carry `title`s and, in some places, `aria-label`s. Known gaps (from `svelte-check`): six launcher `<label>`s are not associated with controls, and the editor container attaches keyboard handlers to a non-interactive `div`. Colour contrast of `dim` (`#3d5c40`) on `ink-950` is below WCAG AA and is used only for tertiary text.

---

## Part B — Technical design decisions

Each decision states the problem, the choice, and the alternative that was rejected.

### B1. Real PTYs over a byte pipe, not agent-specific integrations

*Problem:* dozens of CLIs with different UIs, some full-screen TUIs. *Choice:* spawn each in a native PTY (`portable-pty`: ConPTY on Windows, forkpty elsewhere) and move raw bytes to xterm.js. TermCrew never parses agent output. *Rejected:* per-agent SDK/JSON integrations — brittle, vendor-specific, and would exclude plain shells.

### B2. Binary opcode frames (ttyd-style) instead of JSON

*Problem:* JSON-wrapping terminal bytes costs base64/UTF-8 escaping and CPU on both ends, and there was no way to express flow control. *Choice:* one leading opcode byte (`DATA 0x00`, `RESIZE 0x01`, `PAUSE 0x02`, `RESUME 0x03`, `ACK 0x04`, `EXIT 0x05`, `SETUP 0x06`), little-endian fixed-width fields. Legacy JSON text frames remain accepted for compatibility. *Rejected:* JSON messages (`WsMessage` in `types.ts` is a leftover of that design).

### B3. Credit-based backpressure

*Problem:* an agent that dumps megabytes (or a `cat` of a large file) fills the WebSocket faster than the browser renders, growing memory without bound. *Choice:* the server counts un-ACKed bytes; above 256 KiB it pauses the PTY reader (the child blocks on its stdout, exactly as a slow terminal would). The client ACKs every 32 KiB *after* xterm has written the bytes; below 32 KiB un-ACKed the reader resumes. Output is coalesced into ≤ 64 KiB frames to reduce message overhead. *Rejected:* dropping output (breaks TUIs), unbounded buffering.

### B4. Resize before scrollback replay

*Problem:* replaying an 80×24 transcript into a 220×60 xterm garbles TUIs that use absolute cursor positioning. *Choice:* on attach the server waits up to 600 ms for the browser's `RESIZE`, applies it, waits 120 ms for the child to redraw (SIGWINCH), and only then sends history — which now ends with a frame at the right size. The pane, in turn, refuses to open the socket until it has a real size (≥ 40 cols). *Rejected:* replaying immediately, or storing history per size.

### B5. Bounded server-side scrollback, persisted

*Problem:* reloads and backend restarts should not lose the transcript, but memory must be bounded. *Choice:* a 128 KiB byte history per session in memory (tail-truncated), flushed to `scrollback/<id>.bin` every 10 s, on exit and on park. xterm keeps its own 3000 lines for scrolling. *Rejected:* full transcript logging (disk growth, privacy), no persistence.

### B6. Sessions outlive sockets; the backend restart *parks* rather than kills

*Problem:* closing a tab must not kill an agent mid-task; restarting the backend (rebuild during development, crash) must not destroy work. *Choice:* PTYs are owned by the backend, not by sockets. On shutdown, sessions are parked: scrollback and metadata saved, processes terminated (they cannot survive without their PTY owner), and on next start they load as parked with a Restart affordance. *Rejected:* detached daemon processes surviving the backend (no cross-platform way to re-attach a ConPTY).

### B7. Whole-tree process termination

*Problem:* agents spawn shells, package managers and test runners; killing the top PID orphans them. *Choice:* Windows Job Object with `KILL_ON_JOB_CLOSE` (kernel-enforced even if the backend dies); Unix process groups with SIGTERM then SIGKILL. *Rejected:* walking the process tree by parent PID (racy).

### B8. Worktrees in app-data, not in the repo

*Problem:* parallel **implementers** in one checkout overwrite each other; `.worktrees/` inside the repo needs `.gitignore` edits. *Choice:* Swarm only — `{app-data}/worktrees/{hash(repo)}/agent-{id}` with branch `branch-{id}`; sweep on startup; remove on kill, keep on restart. Solo / Pair / Workbench share the selected folder (roles are labels). If Swarm's folder is not a git repo, TermCrew initialises one so worktrees are possible. *Rejected:* isolating Pair reviewers (they cannot see live dirty files); in-repo `.worktrees/`.

### B9. Hidden setup consoles with sentinels

*Problem:* installers are interactive, slow and noisy; the user wants to watch but the sidebar should not fill with "install" sessions, and the UI needs a machine-readable result. *Choice:* a hidden `Setup` preset session shown inline in the modal; the script prints `__MA_SETUP__:ok|fail`, which the WS layer strips from the stream and turns into an `OP_SETUP` frame. Hidden sessions are never persisted and are dropped on park. On Windows the script is written to a temp `.ps1` and executed with `-ExecutionPolicy Bypass -File` because inline `-Command` strings trip Defender heuristics. *Rejected:* running installers headless and showing only exit codes.

### B10. Registry as code, detection with a short cache

*Problem:* install commands differ per OS and change often; PATH lookups on Windows are slow and npm shims mislead. *Choice:* a Rust table of 28 agents with per-OS lifecycle commands, unit-tested (every docs URL is HTTPS, shims are de-prioritised), `which` + `where.exe` + a curated list of vendor install folders, 2 s cache with `?fresh=true` bypass. *Rejected:* a remote catalogue (adds a network dependency to a local-first tool).

### B11. Skills: allow-listed roots and explicit confirmation

*Problem:* skills live in many harness-specific folders, some managed by the harness itself. *Choice:* a fixed root table; reserved folders (`.system`, `skills-cursor`) are read-only; every write validates the path is inside a managed root and free of `..`; delete needs the literal `DELETE`; enable/disable is a TermCrew-only preference file so harness folders are never edited for visibility. Marketplace installs are delegated to the official `npx skills` CLI rather than re-implementing its resolution. SKILL.md preview renders GFM (GitHub-like) with a Raw tab and copy-source; YAML frontmatter is a table, raw HTML in the file is not executed. *Rejected:* free-form path inputs for writes.

### B12. Optimistic locking for the editor

*Problem:* the user and an agent may edit the same file. *Choice:* the client echoes the `modified_ms` it read; the server refuses the write if the mtime changed. Only existing files can be written (no create), keeping the editor a tool for the crew's output rather than a general IDE. *Rejected:* last-writer-wins, three-way merge.

### B13. Dev transport quirks (Windows loopback)

*Problem:* Vite's http-proxy resets WebSocket upgrades on Windows loopback, and per-request connection closes produce ~5 % `ECONNRESET` on `/api`. *Choice:* REST goes through the Vite proxy with a keep-alive `http.Agent`; the browser connects to `ws://127.0.0.1:3001` directly in dev (`ptyWsUrl`), and to `window.location.host` in production builds. CORS is applied to REST only because wrapping the upgrade route breaks `101 Switching Protocols`.

### B14. Local trust boundary

*Problem:* the API can spawn processes, read and write files, and run installers. *Choice (1.0.0):* bind to loopback, restrict CORS to the Vite origins, and otherwise trust the local operator — the same trust a terminal has. This is documented as a known gap in [PRD.md §9](PRD.md) with a token-based follow-up. *Rejected for now:* password/login UX for a single-user local tool.

---

## Part C — Known design debt (1.0.0)

Kept here so future design work starts from the truth rather than the README:

- Goals is a role playbook over broadcast/handoff, not a server-side supervisor.
- Send review still has no manual target picker (it always takes the next pane in the group).
- Swarm sizing copy disagrees: README "4–6", backend default 3, UI default 4, UI stepper max 6, backend clamp 10.
- Older parked crews may still show `Session n (Pair)` until relaunched.
- `paused` / `error` statuses exist in the type system but the HTTP mapping only emits `running` / `exited`.
- `Counter.svelte`, scaffold SVGs, `bits-ui`, `getDiff`, `createSessionWs`, `pause()/resume()` are unused.
- Header version label is hard-coded (`v1.0`) rather than read from `package.json`/`VERSION`.
- No link detection, copy-on-select, search or font-size control in panes.
- Node requirement drift: Vite 8 needs Node 20.19+/22.12+, README says 18+.
- The file tree lists `node_modules` and `.git`; there is no ignore filter.
