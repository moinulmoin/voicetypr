# Plan 050: Navigation ownership reorganization

**Status:** NEEDS-SMOKE — code complete; frontend gates passed 2026-08-17
**Priority:** P1
**Effort:** M
**Baseline:** `894ecffd`
**Depends on:** 047

## Goal

Make each sidebar destination answer one clear user question without collapsing independent capabilities merely to reduce navigation count.

## Product decisions

- Keep Sources and Network sharing separate. Sources chooses where this device transcribes; Network sharing makes this device available to others. Both can be active simultaneously.
- Keep Polish tabs together and retain the established `Dictionary` label.
- Create a dedicated Recording destination that owns the full before/during/after recording workflow.
- Move Settings to the utility footer and keep only global app preferences there.
- Keep Shortcuts, Network sharing, Agent & CLI, Diagnostics, and Report a problem as coherent destinations.
- Rename Account to License after moving reset controls out.

## Scope

1. Add a `recording` screen and route with a dedicated Recording page.
2. Move capture controls, transcript handling, audio feedback, transcription performance, recording indicator, storage, and cleanup from General Settings to Recording without duplicating editors.
3. Keep Appearance and App behavior in Settings; add privacy controls and reset/start-over controls there.
4. Remove privacy controls from Diagnostics and reset controls from License.
5. Move Settings from the main navigation list to the utility footer and rename Account to License.
6. Clarify that Shortcuts owns additional actions while Recording owns the primary recording trigger.
7. Update affected tests and preserve existing state/update behavior.

## Out of scope

- Combining Sources with Network sharing.
- Renaming Dictionary.
- Splitting Polish into additional sidebar destinations.
- Visual redesign beyond layout required by the ownership changes.
- Backend settings or persistence changes.

## Verification

- Focused component tests cover navigation placement, Recording content, Settings content, License content, Diagnostics content, and shortcut guidance.
- `pnpm typecheck`
- `pnpm lint`
- `pnpm vitest run`
- Native Tauri smoke confirms Recording and Settings render, navigation placement is correct, and moved controls remain operable.

## Evidence

- Focused ownership/navigation suite: 11 files, 93 tests passed.
- Frontend gates: ESLint passed, TypeScript passed, 62 files / 662 tests passed.
- Native development app launched and initialized, but its dashboard window remained hidden with an unresolved accessibility surface; CUA correctly refused background input. Interactive navigation smoke remains in `plans/SMOKE.md`.
