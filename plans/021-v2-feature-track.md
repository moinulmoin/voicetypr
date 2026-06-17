# Plan 021: V2 feature track + carried-over work

> **STATUS: TODO — roadmap index, drafted, not claimed, not executing.** This is
> the sequenced roadmap for the user's V2 feature set (release **option B** — it
> folds into 2.0.0) **plus** the carried-over / unfinished work surfaced by the
> 2026-06-16 roadmap audit. Each **Wave** graduates to its own executable plan
> file (022, 023, …) when claimed, per the `plans/README.md` concurrency
> protocol; this file is the index. Sequencing pressure-tested by oracle
> (`agent://PlanPressureTest`); current-state scout-verified.

## Status

- **Priority**: P2 (feature track; the 2.0.0 recording/AI smoke gates come first)
- **Effort**: XL (multi-wave; per-wave effort below)
- **Risk**: MED — features land on existing seams; the risky executor unification is deferred to Wave 7.
- **Depends on**: 2.0.0 smoke gates (004/008/015/016/017/019/020 + ports — `plans/SMOKE.md`).
- **Category**: product features + architecture carry-over
- **Planned at**: 2026-06-16
- **Release boundary**: **B** — the **Part A feature waves (Wave 0–8)** ship IN 2.0.0 (user decision). Each wave appends its own `SMOKE.md` entries. Part B carry-over is NOT auto-included (see the Part B scope note). 2.0.0 ships after the committed waves + their smoke.

## Decisions baked in (user, 2026-06-16)

- **Release boundary B**: feature track folds into 2.0.0 (not a separate 2.1 track).
- **Diarization = CLOUD-ONLY** (Deepgram / Soniox). Local Whisper has no usable diarization — whisper-rs 0.16.0 exposes experimental `set_tdrz_enable` (tinydiarize, 2-speaker, tiny-model-only) + `next_segment_speaker_turn()`, but VoiceTypr hardcodes `speaker_id: None` (`whisper/transcriber.rs:685-694`, `whisper/gpu_sidecar.rs:584-592`) and it is unwired. macOS Parakeet returns speaker time-ranges only (Swift sidecar `TranscriptionResponse.segments: []`) so it CANNOT yield an attributed transcript without sidecar alignment work. Surface availability in the source/model selector; **transcription quality is unaffected** (Whisper transcribes well on Windows).
- **App-aware history privacy**: persist app metadata ONLY when the user already enabled the existing app-context opt-in (the AI-polish/formatting app-hint preset, `context_policy == AppHintOnly` in `writing.rs`). No new consent surface.
- **Agent surface = CLI polish now; MCP server DEFERRED** (on-demand, when an MCP-native host needs it).

## Architectural ground truth (do NOT re-derive)

- Single transcription seam: `transcribe_with_app(app, TranscriptionRequest)` (`transcription/executor.rs`); `TranscriptionAudio` = `Path | Bytes` (source-agnostic).
- `TranscriptionResult` ALREADY carries `segments: Option<Vec<TranscriptionSegment>>` (with `speaker_id`, `start_ms`/`end_ms`), `timings`, `engine`, `model`, languages, `task` (`transcription.rs:68-93`). Whisper already emits segments → **NOT a DTO greenfield**.
- Cloud parsers (`cloud_stt/*.rs`) return `String` only; do NOT populate `segments`/`speaker_id` — that population is the real diarization cost.
- Upload already ships (`AudioUploadSection.tsx` → `transcribe_audio_file` → `transcribe_audio_file_impl`); ffmpeg extracts video→16 kHz-mono WAV (`ffmpeg/mod.rs`). The upload path does NOT use the shared executor and normalizes cloud/video to WAV before sending, whereas the executor passes cloud input as-is — **do NOT naively unify** (Wave 7 / contract Stage 3).
- Diarization today: `diarize_audio_file` → Parakeet, upload-only, macOS-only, returned as a separate speaker-timeline (not merged into text).

## Part A — Feature waves (oracle-corrected sequence)

### Wave 0 — minimal artifact + history foundation [S]
- Add `source` to `TranscriptionResult` (from `TranscriptionJob`); add an OPTIONAL `words: Option<Vec<TranscriptionWord>>` `{ text, start_ms?, end_ms?, speaker_id?, confidence? }`, populated only where providers supply it (do NOT force every engine to emit words). App-context lives in history/run metadata, NOT the STT result.
- Add a **versioned, metadata-capable** history save path; tolerate old rows, NO eager migration.
- **Acceptance**: types compile, serde back-compat (old history rows still read), no behavior change yet.

### Wave 1 — F3: save uploaded transcript to .txt/.md [S]
- "Save as…" on the upload result via a native save dialog → **both `.txt` (raw) and `.md` (transcript under a minimal `# <name>` heading) ship now**; only the *speaker-block* `.md` enrichment is deferred to Wave 2/F2. Backend save command (frontend fs is appdata-scoped).
- Seam: `src/state/upload.ts` `resultText` + a backend save command (or frontend fs write).
- **Acceptance**: upload → Save as `.txt`/`.md` writes the transcript; plain works before diarization exists.

### Wave 2 — F2: cloud diarization → attributed transcript [M]
- Wire Deepgram `diarize_model=latest` + Soniox `enable_speaker_diarization`; parse speaker-per-word/token into the upload result's `words`/`segments`; group into readable speaker blocks for display + `.md`.
- Source-selector badge: "Speaker labels: Deepgram / Soniox only" (honest copy per ground truth). Parakeet stays timeline-only.
- **DERISK FIRST**: build one tiny provider parser against a recorded fixture JSON (Deepgram or Soniox) — prove speaker grouping, punctuation, Markdown output, and blank/no-speaker fallback BEFORE touching UI broadly.
- **Acceptance**: a 2-speaker file via Deepgram/Soniox → attributed transcript + `.md` with speaker blocks; non-diarizing sources → plain transcript, never guessed labels.

### Wave 3 — F4: app-aware, rich, filterable history [M]
- Persist metadata for NEW rows: `source`, `input_kind` (desktop/upload/remote), engine, model, spoken/transcript language, audio/processing duration, `diarized?`, and `app_context` ONLY when the existing app-hint opt-in is on. Versioned; old rows = metadata-missing (not broken).
- History UI: per-entry detail + filters (by app, source, date).
- **Acceptance**: new entries carry metadata; filters work; old rows still render; app metadata absent when the opt-in is off.

### Wave 4 — F1: CLI agent polish [S] (MCP deferred)
- Make `--json` consistent (currently ignored for `status`/`models`); `transcribe`/`record` emit the typed artifact (text + segments + speaker + metadata) once Wave 0 lands. Calls the local transcription seam, NOT the LAN server.
- **MCP server = DEFERRED** (separate future plan, on demand): if built, prefer stdio MCP; if HTTP, 127.0.0.1-only + mandatory token + no discovery — NOT the existing LAN warp server.
- **Acceptance**: `voicetypr transcribe --file x --json`, `status --json`, `models --json` emit consistent structured JSON.

### Wave 5 — F5: actionable errors + feedback [S]
- Use the unused `ErrorEventPayload.actions/details/suggestion`; extend the pill `toast` payload minimally for remediation; audit copy. Keep roles clear: pill = recording-status/remediation, main-window Sonner = durable/settings actions.
- **Acceptance**: representative failures (mic denied, bad cloud key, paste-permission) show a clear cause + fix; app stays responsive.

### Wave 6 — F6: hotkey model consolidation [M]
- Unify the two models (legacy GeneralSettings single global hotkey + per-action `shortcut_bindings`); preserve migration from `hotkey`/`recording_mode`/`ptt_hotkey`; surface single-key PTT (already works for HoldToRecord behind `allow_risky_combo`).
- **Acceptance**: one coherent shortcuts UI; existing bindings migrate; single-key PTT discoverable; no flap/double-stop regression (PORT-S5).
- **Shipped (2026-06-17, `451d396` + `26bc20e`)**: single-key PTT is now discoverable on Hold-to-record and the General↔Shortcuts relationship is clarified; a bindable "Toggle AI formatting" action was added. The full legacy↔`shortcut_bindings` model merge stays DEFERRED (user kept the dedicated Shortcuts screen; overlaps the native-trigger rewrite).

### Wave 7 — executor unification + adaptive acceleration [L]
- Upload→executor (**contract Stage 3**) BEHIND an explicit input-normalization policy (cloud must still receive real WAV; test with MP4/WebM/WAV fixtures).
- Adaptive GPU↔CPU (F7): first-run micro-benchmark (GPU vs CPU decode) cached per machine, feeding `transcription_acceleration_mode`. Needs real Windows hardware smoke; scope tightly so startup isn't hurt (Apple Silicon Metal essentially always wins).
- **Acceptance**: upload reliability == mic path with no cloud-normalization regression; adaptive choice improves weak-GPU Windows perf without harming Apple Silicon.
- **Shipped (2026-06-17, `6358305`)**: the user-facing acceleration choice — Windows Auto/GPU/CPU picker in Settings + onboarding toggle, persisting the existing `transcription_acceleration` setting (GPU engine + CPU fallback already existed). Still DEFERRED: upload→executor unification (Stage 3) and the adaptive auto-benchmark (needs Windows hardware).

### Wave 8 — release cleanup [S]
- CHANGELOG 2.0.0 entry; focused smoke matrix for the new features; remove only obsolete code from paths actually cut over.

## Part B — Carried-over / unfinished work (2026-06-16 audit)

> **Scope note**: Part B is pre-existing deferred work surfaced for visibility, NOT a
> 2.0.0 commitment. Only **B7** (the existing NEEDS-SMOKE backlog, already part of 2.0.0)
> and **B8** (CHANGELOG) are release gates; **B1 Stage 3** rides Wave 7 (in the track).
> Everything else here — **contract Stages 4–6, SECURITY-04 taxonomy, Plan 011 keychain,
> operational TODOs, deferred-with-intent** — is **NOTED / UNCOMMITTED**: it enters 2.0.0
> only if you explicitly opt an item in. Keeps the release executable "one by one," not a
> 15-item mega-bundle.

### B1. Shared transcription contract — Stages 3–6 (architecture debt)
Design: `docs/plans/2026-06-10-shared-transcription-contract-design.md:290-301`. Stages 1–2 DONE (014/020).
- **Stage 3** — port upload + CLI-local onto `transcribe_with_app` (= Wave 7 unification half). PENDING.
- **Stage 4** — remote server inbound + multipart + `HostDefault`; defines the stable `PublicTranscriptionError` taxonomy. PENDING (`executor.rs:133-137` deferred).
- **Stage 5** — remote client/send-to-peer + CLI-remote onto the executor; retires the inline remote desktop path. PENDING (`executor.rs:256-260` deferred).
- **Stage 6** — delete legacy errors/DTOs/header-context/duplicate routing; normalization-ownership cutover. PENDING.
- Note: Stage 4 supplies the durable error taxonomy that Wave 5 (F5) and SECURITY-04 ultimately want.

### B2. SECURITY-04 — remote raw error strings: MOSTLY RESOLVED
`/api/v1/transcribe` now returns `{ error: "transcription_failed" }` (`remote/http.rs:523-563`), not raw internal strings. Remaining: the durable `PublicTranscriptionError` taxonomy is Stage 4. Downgrade from open finding to "covered; taxonomy pending Stage 4."

### B3. Plan 011 — secrets → OS keychain: REJECTED, redo deferred
Secrets are still `secure_store`-backed (`commands/keyring.rs`, `license/keychain.rs`, `cloud_stt` `stt_api_key_*`, `commands/remote.rs` passwords; `reset.rs` targets `secure.dat`). A redo needs an app-owned secret manifest, lazy per-key migration with read-back verification, a clean all-caller cutover, and macOS/Windows keychain smoke. Not scheduled.

### B4. Plan 013 — RESERVED, no file
Close 004/008 smoke blockers; only materializes if 004/008 smoke surfaces a blocker.

### B5. Operational-polish code markers (low priority)
- `commands/model.rs:276` — download duration logged as 0 (TODO).
- `license/api_client.rs:5` — retry logic TODO; `MAX_RETRIES` dead.
- `remote/lifecycle.rs:440` — `active_connections` reported as 0 (TODO).
- `utils/diagnostics.rs:59` — placeholder 100 GB disk value.
- Bundle as a single low-priority polish pass if/when touched.

### B6. Deferred-with-intent (still open, unscheduled)
`sha1`/`sha256`→`checksum` rename; `useRecording` listener-cleanup race; blocking recorder-stop busy-poll; god-module splits (`audio.rs` ~6k lines); frontend state consolidation; mock-heavy frontend tests; recorder CPAL hot-path allocation (PR #79 review deferral); 4 network-UI UX items (`NetworkSharingCard`/`AddServerModal`/`RemoteServerCard`).

### B7. NEEDS-SMOKE backlog (gates 2.0.0)
004/008/015/016/017/019/020 + PORT-S1..S14 + Failure-Preservation — done-but-unsmoked (`plans/SMOKE.md`). With release boundary B, these PLUS every feature wave's smoke gate 2.0.0.

### B8. Release cleanup
CHANGELOG has no 2.0.0/Unreleased (versions already 2.0.0 in `package.json` + `Cargo.toml`). = Wave 8.

## Post-2.0.0 — Native key-trigger engine (confirmed project, design-doc-first)

Replace Tauri `global_shortcut` with a native event-tap key-trigger engine (macOS CGEventTap watching modifier flags; Windows `WH_KEYBOARD_LL`). Owner-confirmed 2026-06-17; ships AFTER 2.0.0. Build as a standalone, reusable, open-sourceable package.

**Why** — three root-caused limits of `global_shortcut` (exclusive + concrete-key-required + consume-or-nothing):
1. No bare modifier-only / double-tap triggers (hold Right-Option, double-tap Cmd) — the confirmed primary need.
2. Can't bind any key without hijacking it from typing (the 2.0.0 single-key allowlist+cap, `f96743d`, is the stopgap).
3. Exclusive registration fails when a hotkey is already taken; owner wants permissive/non-exclusive — never block, observe + fire, overlaps are the user's to resolve.

**End-state (NOT a permanent side-by-side):** the native engine OWNS ALL triggers (it detects combos from modifier+key state too), `global_shortcut` is RETIRED, and the legacy↔per-action hotkey model-unify (W6) completes as a side effect — one engine, one binding model. Rollout MAY be phased (modifier-only/PTT first, combos after) but single-system replacement is the target.

**Design-doc questions to resolve before any code:** which triggers v1 ships; permission/consume model (Accessibility already granted for paste); the non-exclusive conflict model; cross-platform abstraction; package boundary. Pressure-test with oracle first.

## Execution protocol

- Each Wave graduates to its own executable plan file (022, 023, …) when claimed; add its `plans/README.md` row + set IN PROGRESS per the concurrency protocol. This file is the index/roadmap.
- DERISK items (Wave 2 fixture parser; Wave 7 normalization policy; any future MCP) precede their UI/integration.
- Per-wave smoke is appended to `plans/SMOKE.md` as each lands.

## STOP conditions

- If a wave's "land on the existing seam" assumption breaks (e.g., F2 turns out to need Stage 3 unification), STOP and reassess sequencing — do NOT front-load the risky unification.
- If diarization formatting can't be made honest for a provider (no reliable speaker-per-word), ship a plain transcript for that provider, never guessed labels.
- If app-aware history would capture app identity beyond the existing opt-in's privacy promise, STOP and reconfirm the privacy stance.
