# Skills Registry (Hybrid) — Design

## Goal
Add a Skills console beside Agents: discover global + per-harness + project skills, enable/disable (TermCrew prefs), copy/sync, delete, install (local/git), and marketplace browse/install via skills.sh + `npx skills`.

## Non-goals
- Do not mutate Cursor reserved `~/.cursor/skills-cursor`
- Do not change existing `/api/agents` or PTY behavior
- Do not invent undocumented harness skill paths

## Architecture
- New `backend/src/skills.rs`: roots catalog, scan, prefs, copy, delete, install, marketplace proxy
- Additive REST under `/api/skills*`
- Frontend `SkillsRegistryModal.svelte` matching Agents modal chrome
- Marketplace install reuses SetupTerminal via a shell setup session

## Safety
- All writes confined to allowlisted skill roots
- `.system` and `skills-cursor` are read-only
- Delete requires confirm token
- Preview size-capped
- Path canonicalization + containment checks
