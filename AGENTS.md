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

## Multi-Agent Coordination Protocol

When multiple Claude Code agents work in parallel, follow this protocol strictly.

### Automatic Agent Registration (MANDATORY FIRST STEP)

**At the START of every conversation**, before ANY other work:

1. **Read** the file `.agent-counter` in the project root
   - If it doesn't exist, create it with content "0"
2. **Increment** the number by 1
3. **Write** the new number back to `.agent-counter`
4. **Your Agent ID** for this session is `Agent-<number>` (e.g., `Agent-7`)

Then create your worktree from the remote branch, creating a local working branch that tracks it:
```bash
git worktree add .worktrees/agent-<number> -b agent-<number>-work origin/<branch-from-issue>
cd .worktrees/agent-<number>
```
This avoids the "already checked out" error because the worktree uses its own local branch while tracking the remote branch.

**IMPORTANT**: Use your Agent ID consistently in ALL issue claims during this conversation.

Note: `.agent-counter` is gitignored - stays local to this machine.

### Before Claiming ANY Issue

**CRITICAL**: Check the issue first:

```bash
gh issue view <number> --repo tomchapin/voicetypr --comments
```

**DO NOT START** if you see:
- Label `in progress` on the issue
- A recent "🤖 AGENT WORKING" comment without matching "✅ AGENT COMPLETE"

### Claiming an Issue

When you begin work, **immediately** do both:

1. **Add the label**:
```bash
gh issue edit <number> --repo tomchapin/voicetypr --add-label "in progress"
```

2. **Add claim comment** (use the template from agent-start script):
```
## 🤖 AGENT WORKING

**Agent ID**: Agent-42  (your assigned ID)
**Started**: 2026-01-15T20:30:00Z  (current UTC time)
**Worktree**: .worktrees/agent-42

Working on this issue now. Other agents please select a different issue.
```

### Completing an Issue

When you finish work:

1. **Add completion comment**:
```bash
gh issue comment <number> --repo tomchapin/voicetypr --body "$(cat <<'EOF'
## ✅ AGENT COMPLETE

**Agent ID**: [Same ID as claim]
**Completed**: [ISO 8601 timestamp]
**Duration**: [How long it took]

### Summary
[Brief description of what was done]

### Tests Added/Modified
- [List test files]

### Verification
- [ ] All tests pass
- [ ] Code compiles without errors
EOF
)"
```

2. **Remove the label**:
```bash
gh issue edit <number> --repo tomchapin/voicetypr --remove-label "in progress"
```

### Working in Your Worktree

After creating your worktree, all work happens there:

```bash
cd .worktrees/agent-<your-number>

# All work happens in this directory
# Commits go to the shared branch automatically
# Each agent has isolated working directory
```

### Conflict Resolution

If two agents accidentally work on the same issue:
1. The agent who commented first has priority
2. The second agent should stop and pick a different issue
3. If work was already done, coordinate via issue comments to merge or discard

### Issue Labels Reference

- `in progress` - An agent is actively working on this (DO NOT CLAIM)
- `tests` - Test writing task
- `task` - General task
- `blocked` - Cannot proceed, waiting on something

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
