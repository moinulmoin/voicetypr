---
title: feat: Add auto-paste toggle
type: feat
status: active
date: 2026-04-27
origin: https://github.com/moinulmoin/voicetypr/issues/74
---

# feat: Add auto-paste toggle

## Overview

Add a user-facing setting that controls whether Voicetypr automatically inserts completed transcriptions into the focused app. The default remains enabled so existing users keep current behavior. When disabled, Voicetypr should still complete transcription, run formatting, save history, and leave the transcript available in the clipboard for manual paste without sending paste keystrokes to the current focused window.

## Problem Frame

Issue #74 reports that long transcriptions can finish after the user has switched context, causing auto-paste to insert text into the wrong window. The first QoL improvement is a simple opt-out. Capturing the original app/control and retargeting insertion is intentionally out of scope because it requires cross-platform focus/accessibility design.

## Requirements Trace

- R1. Provide an app setting to activate/deactivate auto-paste.
- R2. Preserve current default behavior: auto-paste remains enabled unless the user disables it.
- R3. When auto-paste is disabled, do not paste into the active app after transcription finishes.
- R4. When auto-paste is disabled, preserve the completed transcript for manual use and continue saving transcription history.
- R5. Cover defaults, persistence, and insertion gating with targeted tests.

## Scope Boundaries

- Do not implement original-window/control retargeting in this change.
- Do not change transcription, AI formatting, or history semantics except for the final auto-paste gate.
- Do not change permission requirements beyond avoiding paste when auto-paste is disabled.

## Context & Research

### Relevant Code and Patterns

- `src-tauri/src/commands/settings.rs` owns persisted settings, defaults, `get_settings`, and `save_settings`.
- `src/types.ts` mirrors `AppSettings` for frontend settings updates.
- `src/components/sections/GeneralSettings.tsx` already has switch rows for recording preferences including `keep_transcription_in_clipboard`.
- `src-tauri/src/commands/audio.rs` calls `crate::commands::text::insert_text` after transcription/formatting and before saving history.
- `src-tauri/src/commands/text.rs` already has `copy_text_to_clipboard`, which copies without attempting paste.
- `src-tauri/src/tests/settings_commands.rs` covers settings defaults and serialization.
- `src/components/sections/__tests__/GeneralSettings.autostart.test.tsx` and `GeneralSettings.recording-indicator.test.tsx` show the General Settings testing pattern.

### Institutional Learnings

- `docs/solutions/` is not present in the tracked/current checkout, so no project learning doc was available for this feature.

## Key Technical Decisions

- Add a distinct `auto_paste_transcription` setting instead of reusing `keep_transcription_in_clipboard`; one controls paste side effects, the other controls clipboard restoration after paste.
- Default `auto_paste_transcription` to `true` for backward compatibility.
- Gate in the backend `audio.rs` completion path so all recording entry points share the same behavior.
- When disabled, call `copy_text_to_clipboard(final_text)` instead of `insert_text(final_text)` so the transcript remains manually available without paste keystrokes.
- Keep history saving after the gate unchanged.

## Open Questions

### Resolved During Planning

- Should disabled auto-paste still copy to clipboard? Yes. The existing UI already frames clipboard retention/manual paste as a user workflow, and copying without paste directly addresses issue #74.

### Deferred to Implementation

- Exact toast wording when auto-paste is disabled: choose concise existing pill-toast style while editing `audio.rs`.

## Implementation Units

- [x] **Unit 1: Persist auto-paste preference**

**Goal:** Add `auto_paste_transcription` to backend settings and frontend types.

**Requirements:** R1, R2, R5

**Dependencies:** None

**Files:**
- Modify: `src-tauri/src/commands/settings.rs`
- Modify: `src/types.ts`
- Test: `src-tauri/src/tests/settings_commands.rs`

**Approach:**
- Add the field to `Settings`, default it to `true`, read it from store with default fallback, and write it in `save_settings`.
- Add the optional field to frontend `AppSettings`.
- Update settings tests that construct `Settings` explicitly.

**Patterns to follow:**
- `keep_transcription_in_clipboard` in `src-tauri/src/commands/settings.rs`
- existing default/serialization tests in `src-tauri/src/tests/settings_commands.rs`

**Test scenarios:**
- Happy path: default settings report `auto_paste_transcription == true`.
- Serialization: explicit settings serialize/deserialize the field.

**Verification:**
- Backend settings tests compile and pass.

- [x] **Unit 2: Add General Settings toggle**

**Goal:** Expose the setting in the Recording section of General Settings.

**Requirements:** R1, R2, R5

**Dependencies:** Unit 1

**Files:**
- Modify: `src/components/sections/GeneralSettings.tsx`
- Test: `src/components/sections/__tests__/GeneralSettings.autostart.test.tsx` or a focused General Settings test file

**Approach:**
- Add a switch row near the clipboard-retention setting.
- Use `settings.auto_paste_transcription ?? true` for checked state.
- Persist updates through `updateSettings({ auto_paste_transcription: checked })`.

**Patterns to follow:**
- Switch rows for clipboard retention and recording sounds in `GeneralSettings.tsx`.

**Test scenarios:**
- Happy path: switch renders enabled by default when setting is absent/true.
- Happy path: toggling off calls `save_settings` through the existing settings context update path with `auto_paste_transcription: false`.

**Verification:**
- General Settings targeted tests pass.

- [x] **Unit 3: Gate backend insertion path**

**Goal:** Prevent paste keystrokes when auto-paste is disabled while preserving clipboard/manual-use and history behavior.

**Requirements:** R3, R4, R5

**Dependencies:** Unit 1

**Files:**
- Modify: `src-tauri/src/commands/audio.rs`
- Test: `src-tauri/src/tests/audio_commands.rs` if a focused unit test seam exists; otherwise cover by targeted compile/test and settings tests.

**Approach:**
- In the transcription completion path after formatting and UI stabilization, read settings via `get_settings(app_for_process.clone()).await`.
- If `auto_paste_transcription` is true, keep the existing `insert_text` behavior and error toasts.
- If false, call `copy_text_to_clipboard(final_text.clone()).await`, show a short pill toast indicating text was copied, and continue to history save.
- On copy failure, show a paste/copy failure toast but still continue to history save.

**Patterns to follow:**
- Existing `insert_text` error handling in `src-tauri/src/commands/audio.rs`.
- Existing `copy_text_to_clipboard` command in `src-tauri/src/commands/text.rs`.

**Test scenarios:**
- Integration/behavior: default enabled path still attempts insertion.
- Integration/behavior: disabled path does not call insertion and leaves transcript copied for manual paste.
- Error path: copy failure does not prevent history save.

**Verification:**
- Targeted Rust tests pass or, if the seam is too integration-heavy for a small change, backend tests compile and pass with explicit manual reasoning documented.

## System-Wide Impact

- **Interaction graph:** Recording completion -> optional AI formatting -> pill hide -> auto-paste/copy gate -> history save.
- **Error propagation:** Paste/copy errors surface as pill toasts and must not abort history persistence.
- **State lifecycle risks:** Setting is persisted in the existing Tauri store; default fallback protects older stores.
- **API surface parity:** Frontend `AppSettings` must match backend `Settings` for `save_settings` payloads.
- **Integration coverage:** The backend gate is the important shared seam because hotkey recording and any recording completion path flow through `audio.rs`.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Conflating clipboard retention with auto-paste | Add a separate setting with explicit naming and UI copy. |
| Disabled auto-paste loses transcript | Copy transcript to clipboard and preserve history save. |
| Existing users lose current workflow | Default new setting to `true`. |
| Tests rely on stale settings fixtures | Update explicit settings objects in frontend/backend tests. |

## Documentation / Operational Notes

- Reference issue #74 in commit/PR summary.
- No release workflow or migration changes required.

## Sources & References

- Origin issue: https://github.com/moinulmoin/voicetypr/issues/74
- Related settings code: `src-tauri/src/commands/settings.rs`
- Related insertion code: `src-tauri/src/commands/audio.rs`, `src-tauri/src/commands/text.rs`
- Related UI code: `src/components/sections/GeneralSettings.tsx`
