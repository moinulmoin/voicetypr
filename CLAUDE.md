# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

VoiceTyper is a native desktop app for macOS that provides offline voice transcription using Whisper. Built with Tauri v2 (Rust) and React with TypeScript.

## Development Commands

```bash
# Start development
pnpm dev          # Frontend only (Vite dev server)
pnpm tauri dev    # Full Tauri app development

# Build production app
pnpm tauri build  # Creates native .app bundle

# Preview production build
pnpm preview
```

## Architecture

### Frontend (React + TypeScript)
- **UI Components**: Pre-built shadcn/ui components in `src/components/ui/`
- **Styling**: Tailwind CSS v4 with custom configuration
- **State**: Use react-hook-form for forms, next-themes for theming
- **Path Aliases**: `@/*` maps to `./src/*`

### Backend (Rust + Tauri)
- **Source**: `src-tauri/src/`
- **Capabilities**: Define permissions in `src-tauri/capabilities/`

IMPORTANT: Read `agent-docs` for more details on the project before making any changes.