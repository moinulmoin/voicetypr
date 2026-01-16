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
  - `remote/`: Network sharing server and client (HTTP API via warp)
  - `menu/`: System tray menu management
  - `parakeet/`: Parakeet sidecar integration
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
- Swift/FluidAudio Parakeet sidecar (1.2MB vs 123MB Python)
- Settings persistence
- Comprehensive test suite (110+ tests)
- Error boundaries and recovery
- Global hotkey support
- **Remote Transcription / Network Sharing** (see below)

📝 **Recent Updates** (combined-fixes branch):
- **Remote Transcription**: Mac can use Windows PC as transcription server
- Network Sharing UI for hosting/connecting to remote servers
- Tray menu shows remote servers with "ServerName - ModelName" format
- System notifications when remote server unavailable
- Parakeet Swift integration (Apple Neural Engine, macOS only)
- Automated sidecar build via `build.rs`

### Remote Transcription Feature

The `combined-fixes` branch includes full remote transcription support:

**Server Mode (Windows/powerful machine):**
- Settings → Network Sharing → Enable "Share on Network"
- Serves transcription requests from other VoiceTypr instances
- Uses local Whisper models on GPU

**Client Mode (Mac/lightweight machine):**
- Settings → Models → Add Remote Server
- Select remote server from tray menu or dashboard
- Audio recorded locally, sent to server for transcription

**Key files:**
- `src-tauri/src/remote/` - Server and client implementation
- `src-tauri/src/commands/remote.rs` - Tauri commands for remote features
- `src/components/sections/NetworkSharingSection.tsx` - UI for network sharing

### Common Patterns

1. **Error Handling**: Always wrap risky operations in try-catch
2. **Loading States**: Show clear feedback during async operations
3. **Graceful Degradation**: App should work even if some features fail
4. **Type Safety**: Use TypeScript strictly, avoid `any`

IMPORTANT: Run `bd list` to see current issues before starting work.
IMPORTANT: Run `bd ready` to find issues with no blockers that you can pick up.
IMPORTANT: Read `CLAUDE.local.md` for any machine-specific configuration.

## Multi-Agent Collaboration

This project uses **Beads** (git-backed issue tracker) and **Git Worktrees** for parallel async development by multiple Claude Code agents.

### 🔴 CRITICAL: Beads Viewer & Daemon

This project uses two essential tools for multi-agent coordination:

| Tool | What It Does | Command | Source |
|------|--------------|---------|--------|
| **Beads CLI (`bd`)** | Issue tracking commands | `bd list`, `bd ready`, etc. | [steveyegge/beads](https://github.com/steveyegge/beads) |
| **Beads Viewer (`bv`)** | Web dashboard (used by daemon) | (started by daemon) | [Dicklesworthstone/beads_viewer](https://github.com/Dicklesworthstone/beads_viewer) |
| **Beads Daemon** | Syncs SQLite → JSONL + runs preview server | `./beads-watch.sh` (or `.ps1`) | (local script in repo) |

**⚠️ WITHOUT THE DAEMON, THE DASHBOARD SHOWS STALE DATA!**

The `bd` CLI stores data in SQLite. The `bv` viewer reads from `.beads/issues.jsonl`. The daemon bridges them by exporting changes every 5 seconds. If you update an issue with `bd update` but don't run the daemon, other agents won't see your changes in the dashboard.

### Quick Start for New Agents

**Every session, do these steps FIRST:**

1. **Start the beads daemon** (REQUIRED - includes preview server):
   ```bash
   # macOS/Linux
   ./beads-watch.sh &

   # Windows PowerShell
   powershell -ExecutionPolicy Bypass -File beads-watch.ps1
   ```

2. **Check current work status:**
   ```bash
   bd list --status=in_progress   # See what's being worked on
   bd ready                        # Find available issues
   ```

3. **Read the bv-site/README.md** for prioritized issue list

4. **Before starting any issue:**
   ```bash
   bd show <issue-id>              # Read full details and comments
   bd update <id> --status=in_progress  # Claim the work
   ```

5. **While working, add progress comments:**
   ```bash
   bd comments add <id> "Started work on X..."
   bd comments add <id> "Fixed Y, testing Z..."
   ```

6. **After completing work:**
   ```bash
   bd comments add <id> "STATUS: READY FOR VERIFICATION - <summary>"
   # DO NOT close - wait for user to verify
   ```

### First-Time Setup (Bootstrap)

**Prerequisites:** Go 1.21+ (for building from source) or use package managers.

#### Install Beads CLI (`bd`)

**macOS/Linux (Homebrew - recommended):**
```bash
brew install steveyegge/beads/bd
```

**macOS/Linux (curl script):**
```bash
curl -fsSL https://raw.githubusercontent.com/steveyegge/beads/main/scripts/install.sh | bash
```

**npm (any platform):**
```bash
npm install -g @beads/bd
```

**Go (any platform):**
```bash
go install github.com/steveyegge/beads/cmd/bd@latest
```

#### Install Beads Viewer (`bv`)

**macOS/Linux (curl script - recommended):**
```bash
curl -fsSL "https://raw.githubusercontent.com/Dicklesworthstone/beads_viewer/main/install.sh" | bash
```

**Windows (PowerShell):**
```powershell
irm "https://raw.githubusercontent.com/Dicklesworthstone/beads_viewer/main/install.ps1" | iex
```

**Go (any platform):**
```bash
git clone https://github.com/Dicklesworthstone/beads_viewer.git
cd beads_viewer
go install ./cmd/bv
```

#### Initialize Beads (if not already done)

```bash
bd init
```

This creates the `.beads/` directory. The project should already have this initialized.

#### Verify Installation

```bash
bd --version    # Should show version
bv --version    # Should show version
bd list         # Should show project issues
```

### Beads Issue Tracking

**Source repositories:**
- **beads** (`bd`): https://github.com/steveyegge/beads - Git-backed issue tracker
- **beads_viewer** (`bv`): https://github.com/Dicklesworthstone/beads_viewer - Dashboard UI

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
1. **DO NOT close the issue yourself** - wait for user verification
2. Commit your changes
3. Inform the user the work is ready for testing
4. Only close the issue with `bd close <id> --reason="..."` **after the user confirms** it's functionally complete

**IMPORTANT:** Never close issues until a human has verified the feature works correctly. Tests passing is not sufficient - the user must confirm the actual functionality.

### Beads Watch Daemon (CRITICAL)

This project includes custom watch scripts that keep the beads dashboard in sync with the database. **You MUST run this daemon at the start of every session.**

**Watch script files (in project root):**
- `beads-watch.ps1` - Windows PowerShell version
- `beads-watch.sh` - macOS/Linux bash version

**What the daemon does:**
1. Starts the `bv` preview server at http://127.0.0.1:9001
2. Every 5 seconds, exports the SQLite database content via `bd export`
3. Compares MD5 hash of DB content vs `.beads/issues.jsonl` file
4. If different, writes the new content to JSONL and regenerates `bv-site/`
5. This ensures the web dashboard always reflects the current database state

**Why this is necessary:**
- `bd` (beads CLI) stores data in SQLite for fast queries
- `bv` (beads viewer) reads from `.beads/issues.jsonl` for git-friendly storage
- Without the daemon, changes to issues (status updates, new issues, etc.) won't appear in the dashboard
- The daemon detects ANY change (not just new issues) including status changes like `open → in_progress`

### Starting the Daemon (REQUIRED AT SESSION START)

The daemon handles everything: SQLite→JSONL sync AND the preview server.

**Detect your platform first**, then run the appropriate command:

#### macOS / Linux / Git Bash / WSL
```bash
./beads-watch.sh &
```

#### Windows (PowerShell)
```powershell
powershell -ExecutionPolicy Bypass -File beads-watch.ps1
```

**Dashboard URL:** http://127.0.0.1:9001

**Verify daemon is working:**
- Check for output like `Preview server running at http://127.0.0.1:9001`
- Check for periodic output like `[HH:MM:SS] DB changed, syncing N issues...`
- Make a change with `bd` and confirm it appears in the dashboard within 5 seconds

### Manual Sync (If Daemon Not Running)

If the dashboard shows stale data and the daemon isn't running:

#### macOS / Linux / Git Bash / WSL
```bash
bd export > .beads/issues.jsonl
bv --export-pages bv-site
```

#### Windows (PowerShell)
```powershell
# PowerShell requires special handling to avoid UTF-16 BOM corruption
$content = bd export | Out-String
[System.IO.File]::WriteAllText(".beads/issues.jsonl", $content.Trim(), [System.Text.UTF8Encoding]::new($false))
bv --export-pages bv-site
```

### Troubleshooting

**Dashboard empty or showing wrong data:**
1. Run `bd doctor` to check for sync issues
2. Run the manual sync commands above (use correct platform commands!)
3. Restart the watch daemon

**Windows-specific: JSONL file shows garbage characters (ÿþ or ��):**
- This is UTF-16 BOM corruption from using `>` redirect in PowerShell
- Fix: Use the `.NET WriteAllText` method shown above, or use Git Bash instead

**"Count mismatch" or "Status mismatch" warnings:**
- Run `bd export > .beads/issues.jsonl` to force sync from DB (source of truth)

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
