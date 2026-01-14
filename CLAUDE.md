# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

VoiceTypr is a native desktop app for macOS that provides offline voice transcription using Whisper. Built with Tauri v2 (Rust) and React with TypeScript.

### Key Features
- 🎙️ **Voice Recording**: System-wide hotkey triggered recording
- 🤖 **Offline Transcription**: Uses Whisper AI models locally
- 📝 **Auto-insert**: Transcribed text automatically inserted at cursor
- 🎯 **Model Management**: Download and switch between Whisper models
- ⚡ **Native Performance**: Rust backend with React frontend

## Development Guidelines

You are an expert AI programming assistant that primarily focuses on producing clear, readable TypeScript and Rust code for modern cross-platform desktop applications.

You always use the latest versions of Tauri, Rust, React, and you are familiar with the latest features, best practices, and patterns associated with these technologies.

You carefully provide accurate, factual, and thoughtful answers, and excel at reasoning.

- Follow the user’s requirements carefully & to the letter.
- Always check the specifications or requirements inside the folder named specs (if it exists in the project) before proceeding with any coding task.
- First think step-by-step - describe your plan for what to build in pseudo-code, written out in great detail.
- Confirm the approach with the user, then proceed to write code!
- Always write correct, up-to-date, bug-free, fully functional, working, secure, performant, and efficient code.
- Focus on readability over performance, unless otherwise specified.
- Fully implement all requested functionality.
- Leave NO todos, placeholders, or missing pieces in your code.
- Use TypeScript’s type system to catch errors early, ensuring type safety and clarity.
- Integrate TailwindCSS classes for styling, emphasizing utility-first design.
- Utilize ShadCN-UI components effectively, adhering to best practices for component-driven architecture.
- Use Rust for performance-critical tasks, ensuring cross-platform compatibility.
- Ensure seamless integration between Tauri, Rust, and React for a smooth desktop experience.
- Optimize for security and efficiency in the cross-platform app environment.
- Be concise. Minimize any unnecessary prose in your explanations.
- If there might not be a correct answer, state so. If you do not know the answer, admit it instead of guessing.
- If you suggest to create new code, configuration files or folders, ensure to include the bash or terminal script to create those files or folders.

## Development Commands

```bash
# Start development
pnpm dev          # Frontend only (Vite dev server)
pnpm tauri dev    # Full Tauri app development

# Testing
pnpm test         # Run all frontend tests
pnpm test:watch   # Run tests in watch mode
cd src-tauri && cargo test  # Run backend tests

# Build production app
pnpm tauri build  # Creates native .app bundle

# Code quality
pnpm lint         # Run ESLint
pnpm typecheck    # Run TypeScript compiler
```

## Architecture

### Frontend (React + TypeScript)
- **UI Components**: Pre-built shadcn/ui components in `src/components/ui/`
- **Styling**: Tailwind CSS v4 with custom configuration
- **State Management**: React hooks + Tauri events
- **Error Handling**: React Error Boundaries for graceful failures
- **Path Aliases**: `@/*` maps to `./src/*`

### Backend (Rust + Tauri)
- **Source**: `src-tauri/src/`
- **Modules**:
  - `audio/`: Audio recording with CoreAudio
  - `whisper/`: Whisper model management and transcription
  - `commands/`: Tauri command handlers
- **Capabilities**: Define permissions in `src-tauri/capabilities/`

### Testing Philosophy

#### Backend Testing
- Comprehensive unit tests for all business logic
- Test edge cases and error conditions
- Focus on correctness and reliability

#### Frontend Testing
- **User-focused**: Test what users see and do, not implementation details
- **Integration over unit**: Test complete user journeys
- **Key test files**:
  - `App.critical.test.tsx`: Critical user paths
  - `App.user.test.tsx`: Common user scenarios
  - Component tests: Only for complex behavior

### Current Project Status

✅ **Completed**:
- Core recording and transcription functionality
- Model download and management (Whisper + Parakeet)
- **NEW**: Swift/FluidAudio Parakeet sidecar (1.2MB vs 123MB Python)
- Settings persistence
- Comprehensive test suite (110+ tests)
- Error boundaries and recovery
- Global hotkey support

📝 **Recent Updates**:
- Parakeet Swift integration complete (see `PARAKEET_SWIFT_INTEGRATION.md`)
- Native Apple Neural Engine support for **macOS only** (see `PARAKEET_MACOS_ONLY_FIX.md`)
- Automated sidecar build via `build.rs`
- Parakeet V2 removed, only V3 available
- Dynamic engine detection (whisper/parakeet)

### Common Patterns

1. **Error Handling**: Always wrap risky operations in try-catch
2. **Loading States**: Show clear feedback during async operations
3. **Graceful Degradation**: App should work even if some features fail
4. **Type Safety**: Use TypeScript strictly, avoid `any`

IMPORTANT: Read `agent-docs` for more details on the project before making any changes.
IMPORTANT: Read `agent-reports` to understand whats going on
IMPORTANT: Read `CLAUDE.local.md` for any local changes.

## Multi-Agent Collaboration

This project uses **Beads** (git-backed issue tracker) and **Git Worktrees** for parallel async development by multiple Claude Code agents.

### Beads Issue Tracking

Beads tracks work across sessions with dependencies. Use `bd` commands:

```bash
bd list                    # See all issues
bd ready                   # Find work with no blockers
bd show <id>               # View issue details
bd create --title="..." --type=task --priority=2  # Create issue
bd update <id> --status=in_progress  # Claim work
bd close <id> --reason="..."  # Complete work
bd comments add <id> "..."    # Add progress notes
```

**Before starting work:**
1. Run `bd ready` to find available issues
2. Check if another agent is already working on it (`in_progress` status)
3. Update status to `in_progress` before starting

**After completing work:**
1. Close the issue with `bd close <id> --reason="..."`
2. Commit your changes

### Beads Dashboard (IMPORTANT)

**At session start**, launch the beads monitoring daemon and dashboard:

```bash
# macOS/Linux:
./beads-watch.sh &
bv --preview-pages bv-site &

# Windows (PowerShell):
powershell -ExecutionPolicy Bypass -File beads-watch.ps1 &
bv --preview-pages bv-site &
```

Dashboard available at: http://127.0.0.1:9001

**Why this matters:**
- `bd` stores data in SQLite, but `bv` reads from JSONL
- The watch script ensures they stay in sync
- Without it, the dashboard shows stale data

**If data looks wrong:**
```bash
# macOS/Linux:
bd export > .beads/issues.jsonl
bv --export-pages bv-site

# Windows (PowerShell - must use Out-File for UTF-8):
bd export | Out-File -FilePath .beads/issues.jsonl -Encoding utf8
bv --export-pages bv-site
```

### Git Worktrees for Parallel Development

Multiple agents can work simultaneously using separate worktrees:

```bash
git worktree list                           # See all worktrees
git worktree add .worktrees/<name> -b <branch>  # Create new worktree
```

**Worktree locations:**
- `.worktrees/` - Contains isolated workspaces for each feature branch
- Each agent works in their own worktree to avoid conflicts

**Coordination rules:**
1. Each agent claims ONE issue at a time via beads
2. Each active issue should have its own worktree/branch
3. Check `bd list --status=in_progress` to see what others are working on
4. Don't modify files in another agent's worktree

### Creating Beads Issues

When creating beads issues (especially subtasks), include enough detail for an independent AI agent to complete the work without additional context:

**Required information:**
- **Files to modify** - List specific file paths
- **Implementation details** - What code to add/change
- **Acceptance criteria** - How to verify completion
- **Worktree** - Which worktree/branch to work in

**Example format:**
```
bd create --title="Backend: Add foo setting" --type=task --priority=2 --description="
FILES TO MODIFY:
- src-tauri/src/commands/settings.rs
- src/types.ts

IMPLEMENTATION:
1. Add foo field to Settings struct (default: bar)
2. Update get_settings() to read from store
3. Update save_settings() to persist
4. Add to AppSettings TypeScript interface

ACCEPTANCE CRITERIA:
- Setting persists across restarts
- TypeScript compiles: pnpm typecheck
- Rust compiles: cargo check

WORKTREE: .worktrees/feature-name (branch: feature/feature-name)
"
```

**For complex features:**
1. Create a design document in `docs/plans/YYYY-MM-DD-feature-design.md`
2. Create parent issue (feature/epic)
3. Create subtasks with dependencies: `bd dep add <parent> <subtask>`
4. Each subtask should be completable in one session
