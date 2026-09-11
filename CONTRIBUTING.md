# Contributing to TermCrew

PRs are welcome. For the code map and invariants, read [AGENTS.md](AGENTS.md) before you edit.

## Install

**Requirements:** [Rust](https://rustup.rs) (stable), [Bun](https://bun.com) 1.4+, [Git](https://git-scm.com). **Windows 10/11** or **macOS**. A browser with WebGL is preferred (Canvas fallback is automatic).

```bash
git clone https://github.com/SSujitX/TermCrew.git
cd TermCrew
```

Then start both processes. First `cargo run` compiles the backend and can take a few minutes.

### Windows

Double-click `start.bat`, or from PowerShell at the repo root:

```powershell
.\run-dev.ps1
```

`start.bat` opens two windows (backend + Vite). `run-dev.ps1` waits until `http://127.0.0.1:3001` answers, then starts the UI in that terminal.

### macOS (and manual start on any OS)

```bash
# Terminal 1 — API + PTY bridge
cd backend
cargo run
# wait for: Server listening on http://127.0.0.1:3001

# Terminal 2 — UI
cd frontend
bun install
bun run dev
# → http://localhost:5173
```

Open **http://localhost:5173**. Hard-refresh if a backend protocol change landed while the tab was open.

The backend is loopback-only. Do not expose `:3001` or widen CORS without adding authentication.

## Checks

```bash
# backend/
cargo test --lib

# frontend/
bun run check
```

`bun run icons:check` if you touch `frontend/src/lib/fileIcons.ts`.

Do not run `run-dev.ps1` from an automation agent (it opens extra windows). After backend edits on Windows, stop `termcrew.exe` before `cargo run` — the old binary holds the port.

## Rules that matter

- **Official install scripts only** for agent install/update/uninstall. Search the vendor docs; do not invent them.
- **bun** for the frontend. Do not add pnpm or switch lockfiles.
- **Opcode parity:** `backend/src/pty_wire.rs` and `frontend/src/lib/ptySocket.ts` must match.
- **Loopback only.** Do not widen the bind address or CORS without authentication.
- **Surgical diffs.** Touch what the change needs. Match existing style.
- Update [ARCHITECTURE.md](ARCHITECTURE.md), [PRD.md](PRD.md), and [README.md](README.md) when you add an endpoint, agent, or user-facing flow.

## Commits and PRs

Use conventional commits (`feat`, `fix`, `docs`, `test`, `chore`, …). Cite PRD ids when behaviour changes (`fix(session): S-11 keep worktree on restart`).

One concern per PR. Say how you tested (`cargo test --lib`, `bun run check`, launch → type → kill).
