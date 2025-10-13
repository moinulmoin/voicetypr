# AGENTS.md — AI Agent Guide for VoiceTypr

## Purpose & Scope
Guidance for AI coding agents and contributors working in this repository. Keep changes minimal, correct, and aligned with existing patterns. For deeper context, read:
- `agent-docs/ARCHITECTURE.md`
- `agent-docs/README.md`
- `agent-reports/` (analysis reports)
- `CLAUDE.md` (coding assistant ground rules)

## Repository Overview
- Frontend (React + TypeScript): `src/`
  - `components/`, `components/ui/`, `components/tabs/`, `components/sections/`
  - `contexts/`, `hooks/`, `lib/`, `utils/`
  - `assets/`, `globals.css`, `test/`
- Backend (Rust + Tauri v2): `src-tauri/src/`
  - `ai/`, `audio/`, `commands/`, `state/`, `utils/`, `whisper/`, `tests/`
- Shared/other: `public/`, `scripts/`, `agent-docs/`, `agent-reports/`
- Path alias: `@/*` → `./src/*` (see `tsconfig.json`)

## Toolchain & Commands
- Dev: `pnpm dev` (frontend), `pnpm tauri dev` (full app)
- Build: `pnpm build`
- Quality: `pnpm lint`, `pnpm typecheck`, `pnpm format`, `pnpm quality-gate`
- Tests: `pnpm test`, `pnpm test:watch`, `pnpm test:backend` (Cargo)

## Coding Conventions
- Frontend
  - React 19 with function components + hooks; strict TypeScript (see `tsconfig.json`)
  - Tailwind CSS v4 utilities; shadcn/ui components in `src/components/ui/`
  - Keep logic in hooks/lib; small, focused components; no unnecessary comments
- Backend
  - Rust 2021+, modules under `src-tauri/src/*`; run `cargo fmt`/`clippy` locally
  - Tauri v2 commands in `src-tauri/src/commands`; audio/whisper modules encapsulate native work

## Testing Strategy
- Frontend: Vitest + React Testing Library; component tests near components (e.g. `__tests__`) and integration in `src/test/`
- Backend: Rust unit/integration tests in `src-tauri/src/tests`; run with `pnpm test:backend`

## Agent Workflow & Guardrails
1. Understand first: prefer `functions.Read`, `Grep`, `Glob`, `LS` for exploration; use absolute paths.
2. Spec-first when asked “how to approach”: propose a concise plan before edits; await approval.
3. Follow existing patterns/libraries; do not introduce new deps without necessity.
4. Before completion: run `pnpm lint`, `pnpm typecheck`, `pnpm test`, and `cd src-tauri && cargo test` unless explicitly waived.
5. Git safety: `git status` → review diffs → commit; never include secrets; don’t push unless asked.

## Commit & PR Guidelines
- Conventional Commits (e.g., `feat:`, `fix:`, `docs:`); keep scopes tight and messages concise.
- Run `pnpm quality-gate` before opening PRs; document capability changes (Tauri) in the PR.

## References
- `agent-docs/ARCHITECTURE.md`, `agent-docs/EVENT-FLOW-ANALYSIS.md`, security docs in `agent-docs/` and `agent-reports/`
- `README.md` for product overview and repo structure
- `CLAUDE.md` for assistant rules and commands
