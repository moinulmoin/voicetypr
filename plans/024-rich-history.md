# Plan 024: Wave 3 (F4) — rich, filterable history

> **STATUS: IN PROGRESS — claimed Main 2026-06-16.** Wave 3 of `plans/021-v2-feature-track.md`.

## Status
- **Priority**: P2 · **Effort**: M · **Risk**: LOW-MED (additive metadata + UI)
- **Depends on**: W0 (TranscriptionResult fields — DONE), W2 (UploadTranscription — DONE)
- **Parent**: plans/021 (Wave 3)

## Scope
Persist rich per-entry metadata on NEW history rows and add a per-entry detail view + filters (source, app, date) to the history list. Versioned/back-compat: old rows lack metadata and still render (all fields optional). **Privacy: `app_context` is persisted ONLY via `writing_result.context_hint`, which is already `Some` only when the user's app-hint opt-in (`context_policy == AppHintOnly`) is on.** App context is meaningful for desktop dictation (the app that received the text); uploads have no target app.

## Ground truth (scout-verified)
- Save seam: `save_transcription(app, text, model)` → `save_transcription_with_recording(app, text, model, recording_file?, writing_metadata?)` stores `{text, model, timestamp}` + optional `recording_file` + optional `writing` (the metadata blob), store "transcriptions", RFC3339 key.
- Desktop ALREADY persists `writing` via `build_writing_history_metadata(transcription, writing_result) -> { mode, output_language, transcript_language, spoken_language, ai_applied, applied_operations, warnings, context_hint }`. Upload + remote-retry DROP metadata (frontend `save_transcription` with text/model only).
- DTOs: `TranscriptionResult { source, engine, model, spoken_language, transcript_language, timings{audio_duration_ms, processing_duration_ms}, words }`; `diarized = words.is_some()`.
- Frontend: `useTranscriptionHistory` → `get_transcription_history({limit})` maps `writing`; `RecentRecordings` has `searchQuery`/`filteredHistory` (filter seam), date grouping, card render (detail seam), bottom metadata row (badges); `types.ts` `TranscriptionWritingMeta` only has translation fields.

## Approach
**Backend** (`commands/audio.rs`):
- Change `build_writing_history_metadata(transcription: &TranscriptionResult, writing: Option<&WritingResult>) -> serde_json::Value`. Always add from `transcription`: `source` (snake_case variant), `engine`, `audio_duration_ms`, `processing_duration_ms`, `diarized` (`words.is_some()`). When `writing` is `Some`: also add `mode`, `output_language`, `ai_applied`, `applied_operations`, `warnings`, `context_hint` (as today). Update the 3 desktop call sites to pass `Some(&writing_result)`.
- Add `metadata: Option<serde_json::Value>` to `UploadTranscription`. Populate it on EVERY upload sub-branch (whisper/parakeet/cloud-non-diarized via `Some(&writing_result)`; cloud-DIARIZED early-return via `None` writing — build from the cloud `TranscriptionResult`/job: source `AudioFile`, engine=provider, model, spoken_language, `diarized: true`).
- Add optional `metadata: Option<serde_json::Value>` param to the `save_transcription` command → forward as `writing_metadata` to `save_transcription_with_recording` (back-compat: existing callers omit it → `None`).

**Frontend**:
- `src/types.ts`: expand `TranscriptionWritingMeta` with optional `source`, `engine`, `audio_duration_ms`, `processing_duration_ms`, `diarized`, `context_hint?: { app_name?: string; app_category?: string }` (keep existing fields).
- `src/state/upload.ts`: `UploadTranscriptionResult` gains `metadata?: ... | null`; pass `metadata: res.metadata` to `save_transcription`.
- `src/components/sections/RecentRecordings.tsx`: per-entry detail — show source, engine/model, language, duration, `diarized` badge, and app (from `writing.context_hint.app_name`) in/near the card metadata row. Filters — extend `filteredHistory` and add controls by the search bar: by **source**, by **app** (values from entries' `context_hint.app_name`, shown only when any exist), by **date** (reuse the existing date grouping/range). Keep text search.

## Tests
- Backend: `build_writing_history_metadata` with `Some`/`None` writing → asserts source/engine/duration/diarized always present, context_hint/mode only with `Some`; `save_transcription` with metadata persists it under `writing`.
- Frontend: `filteredHistory` filters by source + app + date; per-entry detail renders metadata; old entries (no `writing`) still render.

## Acceptance
- New desktop + upload entries carry the metadata; filters narrow the list by source/app/date; per-entry detail shows the metadata; old rows render unchanged; app metadata absent when the opt-in is off. All gates green.

## Smoke (append to SMOKE.md)
- **024-S1**: record (desktop) with app-hint opt-in ON → entry shows the app; upload via cloud → entry shows source=upload + duration + diarized; filter by source/app/date narrows the list; an old (pre-metadata) entry still renders.

## STOP
- If persisting metadata would write `app_context` when the opt-in is OFF, STOP — the gate is `writing_result.context_hint` being `Some`; never re-derive app identity at save time.
