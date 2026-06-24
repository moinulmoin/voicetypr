# Plan 027: Formatting & recording-pill UI/UX pass (captured backlog)

## Status

- **Priority**: P2
- **Effort**: M
- **Category**: ui/ux
- **Captured**: 2026-06-19 (during 2.0.0 manual smoke by the maintainer)
- **Status**: TODO — deferred to the post-2.0.0-smoke **UAUX phase**. This is a
  captured backlog / notes file, NOT yet a runnable plan. Do NOT start before
  smoke completes.
- **Depends on**: 016 (AI polish runtime), 017 (catalog/breadth UI)

## Why this matters

During 2.0.0 smoke the maintainer found the formatting / AI-enhancement
experience confusing, and the recording pill no longer clearly signals the
AI-formatting stage. The underlying features WORK (verified end-to-end); the
gaps are **wording, discoverability, and visible state**. This note keeps the
UX redesign separate from the functional bug-fixes that already shipped.

## Already shipped this session (do NOT re-do here)

- Parakeet duration badge `0:00` fixed (WAV-header fallback when the sidecar's
  FluidAudio `duration` is `0.0`).
- Sidecar stderr `ERROR`/`WARN` log-spam silenced (human-readable banners now
  log once at `debug`; genuine stdout protocol parse failures still `error`).
- Recent Recordings **"Show original / Show formatted"** toggle — the pre-AI
  transcript is persisted in **local history only** (never logged) and the
  toggle appears only when AI actually changed the text.

See `CHANGELOG.md` `[Unreleased]`.

## Items (UAUX phase)

### 1. Formatting-mode naming is confusing
- Symptom: "Dictation (no AI)" vs "Clean Dictation" (and the higher modes) read
  unclearly; the maintainer was unsure what each does or that enabling AI
  auto-selects Clean Dictation.
- Evidence: labels `src/types/ai.ts:19-34`; mode list
  `src/components/EnhancementSettings.tsx:84-90`; per-mode copy
  `EnhancementSettings.tsx:1039-1050`; AI-on auto-switch Personal→Clean
  `src/components/sections/EnhancementsSection.tsx:459-464`;
  backend presets/defaults `src-tauri/src/ai/prompts.rs:102-161`.
- Direction: clearer labels + a one-line "what it does / when to use" under
  each; make the default and the auto-switch legible. (Naming changes belong in
  this phase per the maintainer.)

### 2. No in-UI guidance on how to apply / test formatting
- Symptom: "no guidance on how to apply, test, or perform these actions."
- Direction: inline help; a "test formatting" affordance (dictate a sample, see
  before/after); explicit mode→effect explanation.

### 3. Recording pill doesn't signal the AI-formatting stage
- Symptom: the pill looks identical during transcription and AI formatting; the
  old clear "recording → transcribing → formatting" progression is gone; no
  provider/model or "AI applied" signal anywhere always-on-top.
- Evidence: a `formatting` pill state EXISTS
  (`src/components/pill/usePillController.ts:7-12,29-36`) and the backend emits
  `enhancing-started`/`enhancing-completed`/`enhancing-failed`
  (`src-tauri/src/commands/audio.rs:4730-4743,4788-4789`), BUT `transcribing`
  and `formatting` share the same visual
  (`src/components/pill/PillShell.tsx:19-22`, `src/components/AudioDots.tsx:49`)
  and the pill renders only dots, no label (`PillShell.tsx:37-38`). Provider/
  model never shown (only a generic toast, `src-tauri/src/commands/shortcuts.rs:484,493`).
- Direction: give `formatting` a distinct visual (+ optional label); consider
  surfacing provider/model or an "AI formatting" indicator; consider a
  `during_processing` option for `pill_indicator_mode` (today only
  never/always/when_recording). Backend wiring already exists — primarily a
  visual change, not new events.

### 4. "Shaded / locked" formatting controls are unclear
- Symptom: the maintainer was confused why a section/modes appear shaded.
- Evidence: AI modes lock (greyed + lock icon) when AI is off or no model is
  selected (`src/components/EnhancementSettings.tsx:1007-1012,1031-1032`); the
  AI toggle is gated on a configured key + selected model
  (`EnhancementsSection.tsx:674-680`); writing controls dim until settings load
  (`EnhancementsSection.tsx:948`). It is a capability gate, NOT a license/paywall.
- Direction: clearer locked-state affordance and reason; better empty/loading
  states.

### 5. App-category grouping has no customization UI
- Symptom: the maintainer expected to group apps by category / choose which
  apps are included.
- Evidence: the backend classifies app categories (email/chat/editor/notes)
  from app-name substrings (`src-tauri/src/writing.rs:1227-1257`) and can pass
  the category as a privacy-safe context hint ("Context-aware cleanup",
  `EnhancementSettings.tsx:1102-1117`); App Rules match freeform app-name
  substrings only (`EnhancementSettings.tsx:128-239`). No category-membership UI.
- Direction: decide whether to expose category management or document that
  categories are automatic hints; if exposing, add UI + a persisted model.

### 6. Active provider / endpoint visibility
- Symptom: hard to tell which provider/model/endpoint formatting uses,
  especially a local custom endpoint.
- Evidence: the section header shows "Active model:"
  (`EnhancementsSection.tsx:699-704`); the custom base URL is visible only
  inside the Configure modal.
- Direction: surface the active endpoint (e.g. the custom base URL) on the
  section so users can tell they're on the local custom endpoint.

### 7. History badge shows source, not formatting mode (naming collision)
- Symptom: the maintainer saw a "Dictation" badge on history rows and expected
  it to show the chosen formatting mode; it does not.
- Evidence: that badge is the SOURCE label — `sourceLabel('desktop_recording')`
  returns "Dictation" (`src/components/sections/RecentRecordings.tsx:56-63`),
  alongside "Upload"/"Remote". The metadata badge row renders source, duration,
  diarized, and app only (`RecentRecordings.tsx:805-806`); the formatting MODE
  (`writing.mode`, already persisted by `build_writing_history_metadata`) is NOT
  rendered. "Dictation" as a source label collides with the mode names
  "Personal Dictation" / "Clean Dictation" — that collision is the confusion.
- Direction (maintainer-preferred): relabel sources away from the colliding
  "Dictation". The sources span two axes: `desktop_recording` (live mic,
  transcribed on this device) and `remote_server` (transcribed on a remote
  Voicetypr) are a device axis, while `audio_file`/`audio_bytes` is an input
  axis (uploaded file) — so a pure `device:*` scheme doesn't cleanly cover
  uploads. Proposed: "Local device" / "Remote device" / "File" (device frame,
  with "File" carrying the upload axis); minimal alternative is to rename only
  `desktop_recording` to "Local"/"Mic". Pair with a formatting-mode badge so a
  row reads e.g. "Local device · Clean Dictation" (mode already in metadata).

## Related (functional — track separately if pursued, not part of this UI pass)

- **Clean Dictation perceived to change meaning** with a weak local model. The
  prompt is intent-preserving (`src-tauri/src/ai/prompts.rs:13-18`). If a strong
  model still drifts, tighten the prompt to be more conservative. Pending the
  maintainer's strong-model test; this is prompt-tuning, not redesign.
- **Custom-provider Test "model not found in endpoint model list"** — being
  fixed separately (verify via `/chat/completions` when the model is absent from
  a curated `/models` list), outside this UAUX note.

## Open questions (unresolved — confirm before starting)

- **"Misrendered word" in the formatting UI.** The maintainer's first 2.0.0
  smoke message asked to "fix a misrendered word" but never identified the
  exact string (that message was itself AI-formatted and may have garbled it).
  Re-confirm the specific screen/string before the naming pass so it isn't lost.
