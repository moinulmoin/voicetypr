# Plan 046: Polish workflow alignment

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: Plans 016, 027, 040
- **Claimed**: Main, 2026-08-09
- **State**: IN PROGRESS

## Goal

Align the shipped Polish pipeline with the simplified product contract before the next Beta: users speak naturally, deterministic personalization teaches exact terms and saved text, the active-app rule selects one AI mode before Polish runs, and no hidden spoken-punctuation feature rewrites ordinary dictation.

## Product contract

- Polish owns automatic grammar, punctuation, spacing, false-start cleanup, and restrained paragraph breaks at clear topic or intent changes.
- App Rules select the effective reshaping preset before the single logical Polish stage. A generic app category remains only a lightweight hint when no explicit rule matches.
- Deterministic personalization remains available with or without Polish: Automatic Corrections, Words & Names, and Saved Text.
- Saved Text matches a spoken trigger as the whole utterance; literal entries bypass Polish.
- Spoken formatting commands are not a shipped feature in this release. Remove their hidden defaults and runtime stage rather than hiding them behind an unadvertised setting.
- Legacy global reshaping presets migrate in the backend settings path, without requiring the user to open the Polish screen.

## Deliverables

1. Remove the hidden voice-command schema, defaults, runtime substitutions, and obsolete tests/callers.
2. Move legacy global reshape migration from the React screen mount into the backend settings-loading path used by recordings.
3. Rename Text Shortcuts to Saved Text and clarify trigger/body/literal behavior in the UI.
4. Add restrained paragraph guidance to default AI Polish without changing the stronger Writing, Notes, Message, or Code contracts.
5. Update focused backend/frontend behavior tests and add the affected manual runtime checks to `plans/SMOKE.md`.

## Verification

- Focused Rust tests cover default Polish paragraph guidance, backend preset migration, deterministic rules, and the absence of spoken-command substitution.
- Focused frontend tests cover Saved Text naming and guidance.
- `pnpm typecheck`, `pnpm lint`, `pnpm test`, backend Rust tests, formatting, and Clippy pass.
- Manual Beta smoke remains required for natural punctuation/paragraphing, Saved Text, app-rule precedence, and upgraded-settings migration.
