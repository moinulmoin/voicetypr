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

- Follow the user's requirements carefully & to the letter.
- Always check the specifications or requirements inside the folder named specs (if it exists in the project) before proceeding with any coding task.
- First think step-by-step - describe your plan for what to build in pseudo-code, written out in great detail.
- Confirm the approach with the user, then proceed to write code!
- Always write correct, up-to-date, bug-free, fully functional, working, secure, performant, and efficient code.
- Focus on readability over performance, unless otherwise specified.
- Fully implement all requested functionality.
- Leave NO todos, placeholders, or missing pieces in your code.
- Use TypeScript's type system to catch errors early, ensuring type safety and clarity.
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
- **Remote Transcription / Network Sharing**

### Remote Transcription Feature

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

IMPORTANT: Check GitHub Issues before starting work: https://github.com/tomchapin/voicetypr/issues
IMPORTANT: Read `CLAUDE.local.md` for any machine-specific configuration.

## Issue Tracking (GitHub Issues)

All issues are tracked via GitHub Issues: https://github.com/tomchapin/voicetypr/issues

### Essential Commands

```bash
# List open issues
gh issue list --repo tomchapin/voicetypr

# View issue details
gh issue view <number> --repo tomchapin/voicetypr

# Create new issue
gh issue create --repo tomchapin/voicetypr --title "Title" --body "Description" --label "task"

# Add comment to issue
gh issue comment <number> --repo tomchapin/voicetypr --body "Comment text"

# Close issue when complete
gh issue close <number> --repo tomchapin/voicetypr --comment "Completed: <summary>"
```

### Multi-Agent Coordination Protocol

Multiple Claude Code agents can work on issues in parallel. **STRICTLY FOLLOW THIS PROTOCOL** to avoid conflicts.

#### BEFORE Starting ANY Work - MANDATORY CHECK

**CRITICAL**: Always check the issue status before claiming:

```bash
gh issue view <number> --repo tomchapin/voicetypr --comments
```

**DO NOT START WORK** if you see ANY of these:
- ❌ Label `in progress` is present on the issue
- ❌ A comment within the last 2 hours saying "AGENT WORKING"
- ❌ A claim comment without a matching "AGENT COMPLETE" comment

#### Automatic Agent Registration (DO THIS FIRST)

**At the START of every conversation**, before doing any work:

1. **Read** the file `.agent-counter` in the project root (create with "0" if it doesn't exist)
2. **Increment** the number by 1
3. **Write** the new number back to `.agent-counter`
4. **Your Agent ID** for this session is `Agent-<number>` (e.g., `Agent-7`)

Example:
- Read `.agent-counter` → contains "6"
- Your Agent ID is `Agent-7`
- Write "7" to `.agent-counter`

Then create your worktree (branch specified in the issue you're working on):
```bash
git worktree add .worktrees/agent-7 <branch-from-issue>
cd .worktrees/agent-7
```

**Use your Agent ID consistently** for all issue claims in this conversation.

Note: `.agent-counter` is gitignored, so it stays local to this machine.

#### Claiming an Issue

When you decide to work on an issue, **IMMEDIATELY** perform BOTH steps:

**Step 1 - Add the label:**
```bash
gh issue edit <number> --repo tomchapin/voicetypr --add-label "in progress"
```

**Step 2 - Add claim comment (copy and fill in the template):**
```
## 🤖 AGENT WORKING

**Agent ID**: [YOUR_AGENT_NAME]
**Started**: [CURRENT_UTC_TIMESTAMP, e.g., 2026-01-15T20:30:00Z]
**Branch**: feature/network-sharing-remote-transcription
**Worktree**: .worktrees/[YOUR_AGENT_NAME]

Currently working on this issue. Other agents: please select a different issue.
```

#### While Working

- Reference issue in commits: `git commit -m "test: add X tests (refs #123)"`
- For long tasks, add progress comments every 30+ minutes
- If blocked, comment immediately and pick different issue

#### Completing Work

**Step 1 - Add completion comment with timestamp:**
```bash
gh issue comment <number> --repo tomchapin/voicetypr --body "## ✅ AGENT COMPLETE

**Agent ID**: [Same ID as claim comment]
**Completed**: $(date -u +%Y-%m-%dT%H:%M:%SZ)
**Duration**: [X minutes/hours]

### Summary
[What was accomplished]

### Files Changed
- [List files]

### Tests
- [ ] All tests pass locally
- [ ] Verified with: [command used]

### Ready for Review
Waiting for user verification before closing."
```

**Step 2 - Remove the label:**
```bash
gh issue edit <number> --repo tomchapin/voicetypr --remove-label "in progress"
```

**Step 3 - Do NOT close the issue** - wait for user verification

#### Conflict Resolution

If two agents accidentally claim the same issue:
1. Agent with **earliest timestamp** has priority
2. Second agent must STOP immediately and pick different issue
3. Comment explaining the situation
4. If significant work was done, coordinate via comments to merge

### Labels

- `priority: high` - Critical issues
- `priority: medium` - Normal priority
- `priority: low` - Nice to have
- `bug` - Bug reports
- `feature` - New features
- `task` - Tasks/chores
- `in progress` - Currently being worked on

### Creating Good Issues

When creating issues, include enough detail for any agent to complete the work:

```markdown
## Summary
Brief description of what needs to be done

## Branch
`feature/branch-name` (or `main` if working directly on main)

## Files to Modify
- src-tauri/src/commands/foo.rs
- src/components/Bar.tsx

## Implementation Details
1. Step one
2. Step two
3. Step three

## Acceptance Criteria
- [ ] Criterion 1
- [ ] Criterion 2
- [ ] Tests pass: `pnpm test`
- [ ] TypeScript compiles: `pnpm typecheck`
```

**IMPORTANT**: Always specify which branch the issue relates to. This helps agents know where to commit their work.

## Git Worktrees for Parallel Development

Multiple agents can work simultaneously using separate worktrees:

```bash
git worktree list                           # See all worktrees
git worktree add .worktrees/<name> -b <branch>  # Create new worktree
```

**Worktree locations:**
- `.worktrees/` - Contains isolated workspaces for each feature branch
- Each agent works in their own worktree to avoid conflicts

**Coordination rules:**
1. Each agent claims ONE issue at a time via GitHub Issues
2. Each active issue should have its own worktree/branch
3. Check issue comments to see what others are working on
4. Don't modify files in another agent's worktree
