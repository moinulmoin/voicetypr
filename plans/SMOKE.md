# Pending manual smoke — consolidated checklist

All code below is implemented, gate-green, and committed. The ONLY remaining
work is interactive desktop smoke, batched (per product owner) to run once at
the end of the current feature push, before release. Do NOT re-implement
anything here; executors and agents treat these plans as code-frozen.

Run on a real macOS machine via `pnpm tauri dev` (item 16-S8 needs a Windows
build). Check each box with date + result; on failure, file the failure
against the named plan instead of hot-fixing inline.

## Plan 004 — cancel during `Starting` (code at `9868fdc` era, NEEDS-SMOKE)

- [ ] 004-S1 Toggle mode: record hotkey, immediately press Escape repeatedly
      during startup → ends Idle, no stuck pill, next record starts fresh.
- [ ] 004-S2 PTT mode: hold PTT, release during init → recorder stops
      (`PTT_START_ABORTED_AFTER_RELEASE` path).
- [ ] 004-S3 Normal record/stop/transcribe → unchanged.

## Plan 008 — audio callback hot path (NEEDS-SMOKE)

- [ ] 008-S1 Normal dictation: 10 s record → text appears; WAV plays back
      cleanly if `save_recordings` on.
- [ ] 008-S2 Long recording 3+ min → no unbounded memory growth, transcript
      complete.
- [ ] 008-S3 Stop variants: hotkey stop, Escape cancel → return to Idle, no
      leftover temp files. (Silence no longer auto-stops — see PORT-S8/S9.)
- [ ] 008-S4 Device yank: unplug/switch input mid-recording → graceful stop.
      (Safety-critical: misbehavior here is a release blocker.)

## Plan 015 — pipeline feel / never-stuck (code at `b1a66bf`, NEEDS-SMOKE)

- [ ] 015-S1 Sound ON, wired/builtin mic: hotkey → speak immediately → first
      word present in transcript.
- [ ] 015-S2 Sound ON, Bluetooth headset (if available): chime may clip but
      transcription must include first word.
- [ ] 015-S3 Esc-cancel mid-decode of a ~60 s recording on CPU Whisper →
      pill idle within ~1 s (abort callback).
- [ ] 015-S4 Parakeet: cancel during transcription → pill idle, next
      recording works (sidecar respawn + model reload).
- [ ] 015-S5 Force a formatting hard-failure → toast appears, transcript in
      clipboard, entry in history, state returns to idle.

## Plan 016 — AI polish Rust-native cutover (code at `fb09a61`, NEEDS-SMOKE)

Already auto-proven (no manual re-check needed): invalid OpenAI/Gemini/
Anthropic keys rejected against live endpoints; unreachable custom base URL
→ Network error; failure-path delivery/save/notice covered by unit tests.

- [ ] 016-S2 Valid-key end-to-end polish for two real provider families
      (e.g. OpenAI + Gemini): dictate → polished text inserts at cursor.
- [ ] 016-S3 Forced polish failure (cut network or bad custom URL after a
      valid setup): raw/deterministic text still inserts + "polish failed"
      notice; app stays responsive.
- [ ] 016-S4 Custom base URL with bad endpoint does not persist in settings.
- [ ] 016-S5 Provider switch restores per-provider remembered model.
- [ ] 016-S6 Quit app mid-polish: no crash, no half-written history/settings.
- [ ] 016-S7 Fresh build launches + polishes with no formatting sidecar
      present in the bundle.
- [ ] 016-S8 Windows build: one real polish call (TLS/proxy path) +
      migration from a pre-cutover settings file (`google` provider id).

## Plan 019 — cloud STT shortlist (code `2026-06-12`, NEEDS-SMOKE)

Requires a real API key per provider. Each provider is one catalog entry with a
fixed curated model (OpenAI `gpt-4o-transcribe`, Groq `whisper-large-v3-turbo`,
Deepgram `nova-3`, Cohere `cohere-transcribe-03-2026`, Soniox `stt-async-v3`).

- [ ] 019-S1 For each provider: add API key in Models → provider becomes
      selectable (no longer "Add API Key"); select it → record → transcript
      inserts. (Soniox path unchanged; verify it still works post-migration.)
- [ ] 019-S2 Deepgram specifically (raw-body + `Authorization: Token` path):
      record → transcript returned (validates the non-OpenAI-compatible flow).
- [ ] 019-S3 Cohere: a non-supported language clamps to English; supported
      language transcribes; picker shows only the 14 Cohere languages.
- [ ] 019-S4 Invalid key for any provider → clean typed error (no raw provider
      response body leaked in the message); app stays responsive.
- [ ] 019-S5 Transient failure (e.g. kill network mid-request) → one retry then
      a clean Timeout/Network error; recording-path returns to idle.
- [ ] 019-S6 Network sharing tab with a cloud engine selected → "Cloud sources
      cannot be shared" warning; sharing disabled.
- [ ] 019-S7 Upload + re-transcribe history flows label the source as
      "<Provider> (Cloud)".

## Plan 017 — AI provider catalog + breadth UI (code `2026-06-13`, NEEDS-SMOKE)

Catalog-driven provider/model picker. The 4 production providers (OpenAI,
Anthropic, Google Gemini, Custom) must behave exactly as in plan 016.

- [ ] 017-S1 Production unchanged: existing OpenAI/Anthropic/Gemini/Custom keys
      still validate, select, and polish exactly as before the catalog change.
- [ ] 017-S2 Search filters across provider names AND model ids; clearing
      restores the grouped Recommended/All view.
- [ ] 017-S3 Only OpenAI/Anthropic/Gemini/Custom are listed (no experimental or
      hidden tier); the Experimental badge and Advanced toggle stay dormant
      unless such a provider is added later.
- [ ] 017-S4 Per-provider model memory persists across provider switches.

## Plan 020 — transcription contract Stage 2: desktop executor (code `6ac9b00`, NEEDS-SMOKE)

The desktop record→transcribe→insert hot path now runs through the shared
executor for local + cloud engines (remote stays inline). This integrates plan
015's watchdog/retry/cancel at the executor seam, so **020-S3/S4 supersede
015-S3/S4** — run these against the integrated path.

- [ ] 020-S1 Whisper: hotkey → speak → transcript inserts at cursor; first word
      present (initial_prompt/custom vocab still applied).
- [ ] 020-S2 Parakeet: hotkey → speak → transcript inserts; next recording works.
- [ ] 020-S3 Cloud provider (one real key): hotkey → speak → transcript inserts.
- [ ] 020-S4 Esc-cancel mid-decode of a ~60 s CPU Whisper recording → pill idle
      within ~1 s, no text pasted, no history row (shared cancel flag).
- [ ] 020-S5 Long decode hits the watchdog (or simulate a tiny budget): control
      returns with a timeout, UI not wedged, no speech silently lost.
- [ ] 020-S6 Too-short recording (<0.5 s) rejected cleanly pre-dispatch; no
      history row written.
- [ ] 020-S7 Non-speech/silence → "No speech detected"; no history row.
- [ ] 020-S8 Forced translation failure (output language ≠ spoken, AI key bad):
      raw transcript saved to history with a "translation failed" badge, NOT
      pasted (Fix #2 + marker through the integrated path).
- [ ] 020-S9 Active remote desktop server selected → record → transcript via the
      UNCHANGED inline remote path; kill the server mid-request → failed remote
      history row + preserved recording (Stage 5 untouched).
- [ ] 020-S10 `save_recordings` on: local success saves a recording before temp
      cleanup; re-transcribe that row from History succeeds.

## Failure preservation — recording + retryable row on failure (code `b93a739`, NEEDS-SMOKE)

Genuine transcription failures now preserve the recording + write a retryable
History row, but ONLY when `save_recordings` is ON (privacy-respecting, uniform
across local/cloud/remote). Cancellation/too-short never preserved.

- [ ] FP-S1 `save_recordings` ON + force a cloud failure (bad key / kill network
      mid-request): a "Transcription failed" row appears in History with the
      recording; "Re-transcribe with current source" on it succeeds after fixing
      the key/network. Pill guides to History.
- [ ] FP-S2 `save_recordings` OFF + same cloud failure: NO history row, NO
      orphaned recording kept (nothing pasted either) — respects the setting.
- [ ] FP-S3 Remote failure with `save_recordings` OFF: recording is now NOT kept
      (behavior change — previously always kept); with ON, failed remote row +
      recording as before.
- [ ] FP-S4 Cancel mid-transcription and a too-short clip never create a failed
      row or a kept recording, regardless of `save_recordings`.

## Main hotfix-line ports (2026-06-15, NEEDS-SMOKE)

Behavioral re-application of good fixes from the V1 hotfix line (origin/main)
onto V2; each is reviewer-clean + gate-green and committed. Several are
Windows-runtime only (marked **W**) and can be verified solely on a real
Windows build; the rest run via `pnpm tauri dev` on macOS.

- [ ] PORT-S1 (bug report) Submit a bug report → body includes a System table
      (OS, CPU, RAM, GPU); a spec-collection failure never blocks the report.
      **W**: GPU shows the real adapter name(s), not "Vulkan0" / "Microsoft
      Basic Render Driver".
- [ ] PORT-S2 (parakeet) Parakeet transcription over a long/streaming session →
      no line-protocol corruption from native sidecar stdout noise.
- [ ] PORT-S3 (media pause, ON) A player that under-reports pausability (some
      browsers) is now paused during recording and resumed after; only the
      player WE paused is resumed (none stranded, none wrongly resumed).
- [ ] PORT-S4 (media default) Fresh/unset config: pause-media-during-recording
      defaults to OFF (General settings toggle + actual behavior).
- [ ] PORT-S5 (**W**, hotkey) Hold the toggle hotkey / OS key-repeat → exactly
      one stop (no flap/double-stop); a normal press-release still toggles.
- [ ] PORT-S6 (**W**, recorder) Force an autonomous recorder stop (device yank /
      size cap) mid-recording → app recovers, no stuck "Recording" lockout, and
      a NEW recording starts cleanly afterward (RecorderWatchdog).
- [ ] PORT-S7 (recorder, never-lose-speech) Speak, then yank the input device
      mid-recording → audio captured BEFORE the fault is transcribed (not
      discarded). A genuine stop-timeout/hang instead surfaces "Recording error"
      with no transcribe and full media/ESC cleanup.
- [ ] PORT-S8 (silence, supersedes 008-S3) Stay silent at start → a
      non-destructive "No audio detected — check your microphone" warning
      (NOT an auto-stop); resume speaking → the warning clears and recording
      continues.
- [ ] PORT-S9 (silence timeout) Speak then go silent past the 60s timeout →
      the captured speech is transcribed (never-lose-speech). NO speech for the
      full timeout → discard/cancel with "No audio captured".
- [ ] PORT-S10 (silence gating) Brief taps/blips under ~300ms are not counted as
      speech (no spurious warning-clear); sustained speech clears the warning.
- [ ] PORT-S11 (models responsive) Start a model download; while downloading,
      open Models / get status → UI stays responsive (no freeze, no lock).
- [ ] PORT-S12 (models guards) Click download twice → second rejected ("already
      in progress") without breaking the first's cancel; delete during a
      download → rejected and the selected model is unchanged; a failed download
      shows exactly ONE error (no duplicate toast) and clears the verifying
      state.
- [ ] PORT-S13 (**W**, sidecar warm) Manually preload a Whisper model with GPU/
      auto acceleration → the first transcription after preload is fast (Vulkan
      sidecar warmed during preload).
- [ ] PORT-S14 (**W**, CPU perf) Windows CPU Whisper is faster (openmp + Greedy
      profile) with acceptable accuracy; confirm Apple-Silicon Metal quality is
      UNCHANGED (still BeamSearch) and the Windows build links OpenMP.

## Automated logic coverage for the ports (what no longer needs a human)

Each port's *decision logic* is now locked by unit tests, so the manual run
below only confirms the irreducible physical edges (real hardware I/O, ML
compute, OS event delivery, wall-clock timing) that no headless gate can
exercise. "Locked" = a regression in that logic fails `cargo test` / `vitest`.

- PORT-S1 — LOCKED: `system_info::tests::{get_system_specs_is_infallible_and_populated,
  gpu_detection_is_empty_off_windows}`; `crashReport.test.ts` System-table
  present/absent/GPU-Unknown. RESIDUE: **W** real DXGI adapter names.
- PORT-S2 — LOCKED: `parakeet::sidecar::tests::parse_response_line_*` (5, incl.
  noisy-prefix recovery). RESIDUE: Swift stdout fd-redirect over a real session.
- PORT-S3 — cfg(windows), inspection-only on macOS. RESIDUE: full (GSMTC).
- PORT-S4 — LOCKED: `settings_commands::tests::test_pause_media_during_recording_{default_off,roundtrip}`.
- PORT-S5 — LOCKED: `hotkeys::tests::claim_toggle_press_blocks_repeats_until_release`.
  RESIDUE: **W** real OS key-repeat delivery + the 300 ms wall-clock throttle.
- PORT-S6 — LOCKED: `recorder_watchdog::tests::*` (wait / dispatch-once /
  no-double-dispatch / re-arm) + `recorder::tests::recording_thread_finished_*`.
  RESIDUE: **W** real autonomous stop (device-yank/size-cap) + 250 ms poll timing.
- PORT-S7 — LOCKED: `recorder::tests::stop_error_is_unfinalized_distinguishes_finalized_from_unfinalized`.
  RESIDUE: real device-yank producing a finalized WAV.
- PORT-S8/S9/S10 — LOCKED: `silence_detector::tests::*` (12: tiers, sustained-voice
  300 ms gating, terminal latching, both paths) + `commands::audio::tests::silence_timeout_with_speech_transcribes_and_no_speech_discards`
  (never-lose-speech routing). RESIDUE: real mic capture + 60 s capture/stop integration.
- PORT-S11 — NOT logic-lockable (UI-responsiveness/lock-release property).
  RESIDUE: real download + observe no freeze.
- PORT-S12 — LOCKED: `model_commands::tests` dedup + delete-guard;
  `useModelManagement` double-toast regression test. RESIDUE: real download interactions.
- PORT-S13 — NOT logic-lockable. RESIDUE: **W** real Vulkan warm + first-transcription latency.
- PORT-S14 — PARTIAL: the `cpu_profile = !is_apple_silicon` decision is trivial; the
  Greedy-vs-BeamSearch param *values* are not assertable (whisper-rs `FullParams`
  exposes no getters). RESIDUE: **W** real CPU speed/accuracy + OpenMP link; Metal
  path untouched (inspection-confirmed unchanged).
- Catalog (SHA256/exact-size) — LOCKED: `model_commands::tests` pinned-URL +
  exact-size accept/reject + 64-hex SHA256 + mismatch-removal + dropped-models.

Irreducible floor (no test on any machine-less gate can cover): real GPU/ANE/Vulkan
compute, real microphone capture, OS hotkey event delivery, Windows GSMTC media
sessions, the Swift sidecar fd-redirect, UI-responsiveness, and wall-clock timing
(300 ms throttle, 250 ms poll, 60 s silence timeout).

## Plan 022 — save uploaded transcript to .txt/.md (NEEDS-SMOKE)

- **022-S1**: upload a file → transcribe → click **Save** → choose `.txt`; then repeat and choose `.md`. Both files contain the transcript (the `.md` has a `# <name>` heading); cancelling the dialog writes nothing. (Backend write + command registration are gate-covered; the native save-dialog round-trip is the irreducible UI residue.)

## Plan 023 — cloud speaker diarization for uploads (NEEDS-SMOKE)

- **023-S1**: with a real **Deepgram** key, upload a 2-speaker file → the result is a speaker-attributed transcript ("Speaker 0: … / Speaker 1: …") shown with line breaks, and Save `.txt`/`.md` contains the speaker blocks. Repeat with a real **Soniox** key. A non-diarizing provider (OpenAI/Groq/Cohere) or single-speaker audio → plain transcript (no labels). Live dictation is unaffected (no diarization). (Provider word-parsing + speaker grouping are unit-covered with fixtures; the real-key round-trip + attribution quality are the irreducible residue.)

## Plan 024 — rich, filterable history (NEEDS-SMOKE)

- **024-S1**: with the app-hint opt-in **ON**, dictate into an app → the history entry shows that app; upload a file via a cloud provider → its entry shows source + duration (+ a "Speakers" badge if diarized). Filter the list by **source / app / date** and confirm it narrows correctly; an old (pre-metadata) entry still renders and appears only under "All sources". With the opt-in **OFF**, new entries carry no app name.

## Plan 025 — CLI agent polish (NEEDS-SMOKE)

- **025-S1**: `voicetypr transcribe --file x.wav --json` emits `{ text, words, metadata, model, engine }`; without `--json` prints just the text. `voicetypr status` / `voicetypr models` print human-readable output by default and JSON with `--json`. (Flag parsing + availability formatting are unit-covered; the real transcription round-trip is the residue.)

## Plan 026 — actionable errors + feedback (NEEDS-SMOKE)

- **026-S1**: deny microphone access → the pill feedback overlay shows the failure + a "how to fix" line (System Settings ▸ Privacy & Security). Trigger auto-paste without Accessibility permission → overlay shows "Text copied" + the Accessibility remediation. A normal success toast shows no remediation line.

## GPU/CPU acceleration choice (NEEDS-SMOKE)

- **ACCEL-S1** (**W**, Windows): Settings ▸ General shows a "Transcription performance" Auto/GPU/CPU picker, and onboarding (readiness step) shows a "Use GPU acceleration" toggle (default on). Select **CPU** → record → runs on CPU; **GPU** → runs on the Vulkan sidecar; **Auto** → GPU when available, falls back to CPU on GPU failure. (Picker/toggle + persistence are gate-covered; the real GPU-vs-CPU effect is the Windows-hardware residue.)
- **ACCEL-S2** (macOS): no acceleration picker in Settings and no GPU toggle in onboarding (Metal stays automatic); transcription unaffected.

## Single-key PTT + shortcuts clarity (NEEDS-SMOKE)

- **HK-S1**: Settings ▸ Shortcuts ▸ Recording ▸ Hold to record → the "Use a single key" option is visible without hunting; enable it, bind a single key (e.g. F1) → saving succeeds; holding that key records, releasing stops. With it off, a single key is rejected as before.
- **HK-S2**: the General recording section reads as the primary shortcut + mode and points to Shortcuts for additional/single-key bindings; the two screens no longer look like duplicate hotkey editors.
- **HK-S3**: Settings ▸ Shortcuts → on a non-recording action (e.g. Copy last transcription) the "Use a single key" toggle is offered; enable it and bind **F1** → saves and the key triggers the action. Try to bind a typing key (e.g. **E**) as a single key → rejected with a clear message; bind single keys until **5** are set → a 6th is rejected (cap), and the "N of 5 single-key shortcuts used" hint tracks the count.

## Toggle AI formatting shortcut (NEEDS-SMOKE)

- **AITOGGLE-S1**: Settings ▸ Shortcuts ▸ Formatting → bind "Toggle AI formatting"; press it → pill shows "AI formatting on/off" and the Enhancements AI toggle reflects it; the next recording honors the new state. Pressing it to enable when no AI model/key is configured shows "Set up an AI model in Settings to use formatting" and does not enable.

## Release rule

015 + 016 smoke are ship gates for the AI-polish release; 004/008 smoke are
ship gates for the recording-path release. None block further feature
development on this branch.
