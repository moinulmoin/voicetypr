# VoiceTypr

macOS desktop app for offline voice transcription using Whisper AI. Built with Tauri v2 (Rust backend) and React 19 (TypeScript frontend). Features system-wide hotkey recording, automatic text insertion at cursor, local model management, and **remote transcription via network sharing**.

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

## Issue Tracking (GitHub Issues)

All issues are tracked via GitHub Issues: https://github.com/tomchapin/voicetypr/issues

### Essential Commands

```bash
# List open issues
gh issue list --repo tomchapin/voicetypr

# View issue details
gh issue view <number> --repo tomchapin/voicetypr

# Create new issue
gh issue create --repo tomchapin/voicetypr --title "Title" --body "Description"

# Close issue when complete
gh issue close <number> --repo tomchapin/voicetypr --comment "Completed: <summary>"
```

### Workflow

1. **Find work**: `gh issue list --repo tomchapin/voicetypr --label "priority: high"`
2. **Check the branch**: Each issue specifies which branch to work on
3. **Claim issue**: Add a comment with your agent ID and the branch you're working on
4. **Work on it**: Make changes, commit with `Fixes #<number>` in message
5. **Close when done**: Issue auto-closes when PR merges, or user manually closes after verification

### Issue Format

Issues should include:
- **Branch**: Which branch the work should be done on
- **Files to Modify**: Specific file paths
- **Implementation Details**: What to do
- **Acceptance Criteria**: How to verify completion

### Labels

- `priority: high` - Critical issues
- `priority: medium` - Normal priority
- `priority: low` - Nice to have
- `bug` - Bug reports
- `feature` - New features
- `task` - Tasks/chores

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
├── remote/                   # Network sharing (server + client)
│   ├── server.rs             # HTTP server (warp)
│   ├── client.rs             # HTTP client for remote transcription
│   ├── lifecycle.rs          # Server start/stop management
│   └── settings.rs           # Saved connections persistence
├── menu/                     # System tray menu
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
- `src-tauri/src/commands/audio.rs` — Recording and transcription flow
- `src-tauri/src/commands/remote.rs` — Remote server commands
- `src-tauri/src/remote/` — Network sharing implementation
- `src-tauri/src/menu/tray.rs` — System tray menu
- `src/hooks/` — React hooks for Tauri integration
- `src/components/tabs/` — Main UI tab components
- `src/components/sections/` — Section components (ModelsSection, NetworkSharingSection)
- `src-tauri/capabilities/` — Tauri permission definitions

## References

- `CLAUDE.md` — Full coding guidelines
- `README.md` — Product overview
