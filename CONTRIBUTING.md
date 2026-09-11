# Contributing to TermCrew

PRs are welcome. For the code map and invariants, read [AGENTS.md](AGENTS.md) before you edit.

## Run the app

See [Install](README.md#install). Backend: `127.0.0.1:3001`. UI: `http://localhost:5173`.

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
