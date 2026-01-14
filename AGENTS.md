# VoiceTypr

macOS desktop app for offline voice transcription using Whisper AI. Built with Tauri v2 (Rust backend) and React 19 (TypeScript frontend). Features system-wide hotkey recording, automatic text insertion at cursor, and local model management.

## Core Commands

```bash
# Development
pnpm dev              # Frontend only (Vite)
pnpm tauri dev        # Full Tauri app (frontend + Rust)

# Quality checks (run before commits)
pnpm lint             # ESLint
pnpm typecheck        # TypeScript compiler
pnpm test             # Vitest frontend tests
pnpm test:backend     # Rust tests (cd src-tauri && cargo test)
pnpm quality-gate     # All checks in one script

# Build
pnpm build            # Frontend build
pnpm tauri build      # Native .app bundle
```

## Project Layout

```
src/                          # React frontend
├── components/               # UI components
│   ├── ui/                   # shadcn/ui primitives
│   ├── tabs/                 # Tab panel components
│   └── sections/             # Page sections
├── contexts/                 # React context providers
├── hooks/                    # Custom React hooks
├── lib/                      # Shared utilities
├── utils/                    # Helper functions
├── services/                 # External service integrations
├── state/                    # State management (Zustand)
└── test/                     # Integration tests

src-tauri/src/                # Rust backend
├── commands/                 # Tauri command handlers
├── audio/                    # CoreAudio recording
├── whisper/                  # Transcription engine
├── ai/                       # AI model management
├── parakeet/                 # Parakeet sidecar integration
├── state/                    # Backend state management
├── utils/                    # Rust utilities
└── tests/                    # Rust unit tests
```

## Development Patterns

### Frontend
- **Framework**: React 19 with function components + hooks
- **Styling**: Tailwind CSS v4; use `@/*` path alias for imports
- **Components**: shadcn/ui in `src/components/ui/`; extend, don't modify
- **State**: React hooks + Zustand + Tauri events
- **Types**: Strict TypeScript; avoid `any`
- **Tests**: Vitest + React Testing Library; test user behavior, not implementation

### Backend
- **Language**: Rust 2021 edition
- **Framework**: Tauri v2 with async commands
- **Modules**: Commands in `commands/`; domain logic in dedicated modules
- **Style**: Run `cargo fmt` and `cargo clippy` before commits
- **Tests**: Unit tests in `tests/` directory; use `#[tokio::test]` for async

### Communication
- Frontend calls backend via `invoke()` from `@tauri-apps/api`
- Backend emits events via `app.emit()` or `window.emit()`
- Event coordination handled by `EventCoordinator` class

## Git Workflow

- **Commits**: Conventional Commits (`feat:`, `fix:`, `docs:`, `refactor:`)
- **Pre-commit**: Run `pnpm quality-gate` or individual checks
- **Branches**: Feature branches off `main`
- **Never push** without explicit user instruction

```bash
git status                    # Always check first
git diff                      # Review changes
git add -A && git commit -m "feat: description"
```

## Beads Issue Tracking (Multi-Agent)

This project uses **Beads** for issue tracking across multiple Claude Code agents.

### Session Startup (REQUIRED)

```bash
# macOS/Linux:
./beads-watch.sh &
bv --preview-pages bv-site &

# Windows (PowerShell):
powershell -ExecutionPolicy Bypass -File beads-watch.ps1 &
bv --preview-pages bv-site &
```

Dashboard at: http://127.0.0.1:9001

### Essential Commands

```bash
bd ready                          # Find available work (no blockers)
bd list --status=in_progress      # See what others are working on
bd update <id> --status=in_progress  # Claim work before starting
bd close <id> --reason="..."      # Mark complete when done

# Force sync if stale:
# macOS/Linux: bd export > .beads/issues.jsonl
# Windows:     bd export | Out-File .beads/issues.jsonl -Encoding utf8
```

### Why the Watch Daemon?

- `bd` stores data in SQLite (fast queries)
- `bv` reads from JSONL (git-friendly format)
- Without the daemon, dashboard shows stale data
- The watch script syncs them every 30 seconds

See `CLAUDE.md` → Multi-Agent Collaboration for full details.

## Gotchas

1. **macOS only**: Parakeet models use Apple Neural Engine; Whisper uses Metal GPU
2. **Path alias**: Use `@/` not `./src/` for imports (e.g., `@/components/ui/button`)
3. **NSPanel focus**: Pill window uses NSPanel to avoid focus stealing; test carefully
4. **Clipboard**: Text insertion preserves user clipboard; restored after 500ms
5. **Model preloading**: Models preload on startup; don't assume instant availability
6. **Tauri capabilities**: Permission changes require edits in `src-tauri/capabilities/`
7. **Large lib.rs**: Main Rust entry point at 96KB; navigate via module imports
8. **Sidecar builds**: Parakeet Swift sidecar built via `build.rs` during `tauri build`

## Key Files

- `src-tauri/src/lib.rs` — Main Rust entry, command registration
- `src-tauri/src/commands/` — All Tauri command implementations
- `src/hooks/` — React hooks for Tauri integration
- `src/components/tabs/` — Main UI tab components
- `src-tauri/capabilities/` — Tauri permission definitions

## References

- `agent-docs/ARCHITECTURE.md` — Detailed architecture diagrams
- `agent-docs/EVENT-FLOW-ANALYSIS.md` — Event system documentation
- `CLAUDE.md` — Coding assistant guidelines
- `README.md` — Product overview
