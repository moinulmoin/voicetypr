# Plan 023: Wave 2 (F2) — cloud speaker diarization for uploads

> **STATUS: IN PROGRESS — claimed Main 2026-06-16.** Wave 2 of `plans/021-v2-feature-track.md`. Approach vetted by oracle (`agent://PlanPressureTest` C2) + a reviewer simplification (skip-polish-on-diarized, see Decision).

## Status
- **Priority**: P2 · **Effort**: M · **Risk**: MED
- **Depends on**: W0 (`TranscriptionResult.words` — DONE), the existing upload path
- **Parent**: plans/021 (Wave 2)

## Scope
When an uploaded file is transcribed by a **diarization-capable cloud provider (Deepgram, Soniox)**, request diarization and produce a **single speaker-attributed transcript** ("Speaker 0: …\n\nSpeaker 1: …") shown in the upload result and saved verbatim by the F3 `.txt`/`.md` export. **Cloud-only**: Parakeet keeps its separate speaker-timeline (unchanged); Whisper/OpenAI/Groq/Cohere produce no speaker labels. **Live dictation is untouched** (no diarization; `transcribe(...)` signature unchanged).

## Decision — diarized uploads skip AI polish (own behavior choice, documented)
Speaker attribution lives at the word level on the **raw** STT output. AI polish rewrites text and tends to merge/misattribute speakers, and would break word↔speaker alignment. So: **when speaker words are present, the speaker-grouped raw transcript IS the result (no AI polish)**; when no speaker data is returned (non-diarizing provider, single speaker, or parse failure), the upload flows through AI polish exactly as before. This removes the dual-representation/alignment problem and keeps labels on the text the user actually sees. (Reversible if the user prefers polished + separate speaker view.)

## Backend↔frontend contract (fixed)
`transcribe_audio_file(...)` returns:
```
{ text: string, words: Array<{ text: string; start_ms?: number; end_ms?: number; speaker_id?: string; confidence?: number }> | null }
```
`text` is **display-ready**: the speaker-grouped transcript when diarized, otherwise the AI-polished (or plain) transcript as today. `words` is the raw speaker-per-word output (present only when a diarization-capable cloud provider returned speaker data; else `null`) — kept for structured/history use and as the "diarized" signal to the UI.

## Approach
**Backend cloud_stt** — add an upload-only structured path; keep `transcribe(...) -> Result<String,String>` unchanged for live callers.
- New `CloudProvider::transcribe_diarized(app, path, language) -> Result<CloudTranscript, String>`, `CloudTranscript { text: String, words: Vec<TranscriptionWord> }`, enum-dispatched.
- Deepgram (`deepgram.rs`): add `diarize=true` query param; at the existing Value parse site also walk `results.channels[0].alternatives[0].words[]` → `TranscriptionWord { text: punctuated_word|word, start_ms: round(start*1000), end_ms: round(end*1000), speaker_id: speaker.map(|n| format!("Speaker {n}")), confidence }`.
- Soniox (`soniox.rs`): add `enable_speaker_diarization: true` in `build_create_payload`; parse `tokens[]` → `TranscriptionWord { text, start_ms, end_ms, speaker_id: speaker.map(|s| format!("Speaker {s}")) }`.
- OpenAI/Groq/Cohere: `transcribe_diarized` falls back to plain transcribe (`words: vec![]`).
- **DERISK = fixture tests**: unit tests parse synthetic Deepgram/Soniox JSON (documented shapes) → assert words + speaker_ids + the no-speaker/empty fallback. (Real-key end-to-end is smoke; no API keys in CI.)

**Backend upload** (`commands/audio.rs`):
- Change `transcribe_audio_file` return type from `String` to `UploadTranscription { text: String, words: Option<Vec<TranscriptionWord>> }` (upload-only command; only `upload.ts` consumes it).
- Cloud branch: call `transcribe_diarized`. If `words` non-empty → build the speaker-grouped transcript via a helper `group_words_into_speaker_text(&[TranscriptionWord]) -> String` (concatenate consecutive same-`speaker_id` words into "Speaker N: …" paragraphs), set that as the result text, set `result.words`, and **return it directly (skip `process_transcription`)**. If `words` empty → existing flow (`process_transcription` → `final_text`), `words: None`.
- Non-cloud branches: `words: None`, existing behavior.

**Frontend** (`src/state/upload.ts`, `src/components/sections/AudioUploadSection.tsx`):
- Consume `{ text, words }`; set `resultText = text` (already speaker-grouped when diarized). Store `words` (or a `diarized` flag = `words != null`).
- Render `resultText` preserving line breaks (e.g. `whitespace-pre-wrap`) so speaker paragraphs display. The existing Parakeet `speakerSegments` timeline path is unchanged. F3 `.txt`/`.md` save already writes `resultText` — now carries speaker blocks for free.
- Add a brief "Speaker labels: Deepgram / Soniox only" hint in the upload UI.

## Tests
- Backend: Deepgram + Soniox fixture-JSON parser tests (words + speakers + fallback); `group_words_into_speaker_text` grouping test (consecutive-same-speaker merge, speaker switch, missing speaker_id).
- Frontend: result renders multi-line speaker text; `words != null` drives the diarized hint.

## Acceptance
- Fixture + grouping tests pass; all gates green.
- A diarization cloud provider on upload yields a speaker-grouped transcript shown + saved (`.txt`/`.md`); non-diarizing sources are unchanged (polished/plain, `words: null`); Parakeet timeline unchanged; live dictation unaffected.

## Smoke (append to SMOKE.md)
- **023-S1**: real-key upload (Deepgram, then Soniox) of a 2-speaker file → speaker-attributed transcript shown + exported; a non-diarizing provider → normal polished transcript.

## STOP
- If a provider's diarized response can't be parsed reliably into words, return plain text for it (no guessed labels) — never fabricate attribution.
