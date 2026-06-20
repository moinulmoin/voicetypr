# CLAUDE.md

This file provides guidance to Claude Code when working with code in this repository.

## Project Overview

VoiceTypr is a native desktop app for macOS that provides offline voice transcription using Whisper. It is built with Tauri v2, Rust, React, and TypeScript. V2 also includes remote transcription through local network sharing.

## Development Guidelines

- Follow the user's requirements carefully and to the letter.
- Check `specs/` first if it exists for the task.
- Prefer readable, fully implemented TypeScript and Rust over clever shortcuts.
- Use TypeScript's type system; avoid `any`.
- Extend shadcn/ui primitives instead of modifying them directly.
- Keep Tauri, Rust, and React integration explicit and type-safe.
- Do not push without explicit user instruction.

## Development Commands

```bash
pnpm dev          # Frontend only
pnpm tauri:dev    # Full Tauri app
pnpm test         # Frontend tests
pnpm typecheck    # TypeScript compiler
pnpm lint         # ESLint
pnpm test:backend # Rust tests
pnpm tauri build  # Native app bundle
```

On Windows, use `src-tauri/run-tests.ps1` for Rust tests if plain `cargo test` hits the TaskDialog manifest issue.

## Architecture

### Frontend

- UI components: `src/components/ui/`
- Tabs and app views: `src/components/tabs/`
- Settings sections: `src/components/sections/`
- Hooks: `src/hooks/`
- Shared types: `src/types.ts`
- Path alias: `@/*`

### Backend

- Main Tauri entry: `src-tauri/src/lib.rs`
- Commands: `src-tauri/src/commands/`
- Audio recording: `src-tauri/src/audio/`
- Whisper model management: `src-tauri/src/whisper/`
- Parakeet sidecar integration: `src-tauri/src/parakeet/`
- Network sharing: `src-tauri/src/remote/`
- Tray/menu: `src-tauri/src/menu/`
- Capabilities: `src-tauri/capabilities/`

## Testing

- Frontend tests should verify user-visible behavior.
- Backend tests should cover state transitions and error paths.
- Run focused tests while iterating, then run the relevant full gate before committing.

## Current Notes

- Remote transcription is local-network only: strong host, weak client.
- Remote auth secrets belong in secure storage, not plain settings JSON.
- Manual weak-client/strong-host smoke testing is still required before release confidence.
