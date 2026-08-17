# Plan 050: Navigation ownership reorganization

**Status:** DONE — frontend gates and native macOS CUA smoke passed 2026-08-17
**Priority:** P1
**Effort:** M
**Baseline:** `894ecffd`
**Depends on:** 047

## Goal

Make each sidebar destination answer one clear user question without collapsing independent capabilities merely to reduce navigation count.

## Product decisions

- Keep Sources and Network sharing separate. Sources chooses where this device transcribes; Network sharing makes this device available to others. Both can be active simultaneously.
- Keep Polish capabilities together and retain the established `Dictionary` label. Use a vertical workflow rail so provider, vocabulary, replacements, snippets, and writing modes have explicit purposes without becoming sidebar destinations.
- Create a dedicated Recording destination that owns the full before/during/after recording workflow.
- Order primary navigation by workflow: Overview, History, Upload, Sources, Recording, Polish, Shortcuts, Network sharing, CLI.
- Keep CLI in primary navigation because it is a product capability; label it plainly and provide a reusable prompt for Claude Code, Codex, OpenCode, OpenClaw, and other terminal agents.
- Move Settings to the utility footer and open it as a centered modal with General, License, and Diagnostics navigation.
- Keep license state visible in the app header. License revalidation/expiry also marks Settings for attention and deep-links to License.
- Keep Report a problem outside Settings as a direct utility action.

## Scope

1. Add a `recording` screen and route with a dedicated Recording page.
2. Move capture controls, transcript handling, audio feedback, transcription performance, recording indicator, storage, and cleanup from General Settings to Recording without duplicating editors.
3. Keep Appearance and App behavior in General; add privacy controls and reset/start-over controls there.
4. Remove privacy controls from Diagnostics and reset controls from License.
5. Replace General, License, and Diagnostics sidebar destinations with a centered Settings modal and left-side section navigation.
6. Preserve license discoverability through a visible header badge, attention treatment, and direct License-modal entry.
7. Reorder primary navigation and rename Agent & CLI to CLI.
8. Add agent-oriented CLI onboarding with a copyable, privacy-preserving transcription prompt.
9. Replace Polish's dense horizontal tab strip with an explicit vertical workflow rail and descriptive labels.
10. Clarify that Shortcuts owns additional actions while Recording owns the primary recording trigger.
11. Update affected tests and preserve existing state/update behavior.

## Out of scope

- Combining Sources with Network sharing.
- Renaming Dictionary.
- Splitting Polish into additional sidebar destinations.
- Backend settings or persistence changes.

## Verification

- Focused component tests cover navigation placement, modal section switching, Recording content, Settings content, License content, Diagnostics content, CLI onboarding, Polish workflow navigation, and shortcut guidance.
- `pnpm typecheck`
- `pnpm lint`
- `pnpm vitest run`
- Native Tauri smoke confirms primary navigation order, Recording and Shortcuts ownership, the General/License/Diagnostics modal, license attention treatment, the two-column Polish workflow, and CLI agent onboarding.

## Evidence

- Focused UI suite: 5 files, 64 tests passed.
- Frontend gates: ESLint passed, TypeScript passed, 63 files / 664 tests passed.
- Native macOS CUA smoke: verified the ordered primary destinations; Recording content through the full scroll range; Shortcuts ownership guidance; centered Settings modal with General, License, and Diagnostics; License status badge and Settings attention marker; two-column Polish workflow; and CLI status, commands, supported-agent copy, prompt, and copy control.
