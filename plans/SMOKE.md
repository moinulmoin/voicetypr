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
- [ ] 008-S3 Stop variants: hotkey stop, Escape cancel, silence auto-stop →
      all return to Idle, no leftover temp files.
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
- [ ] 017-S3 Experimental providers (Groq, OpenRouter) show the Experimental
      badge by default. No hidden-tier providers currently, so the Advanced
      toggle does not appear (it returns only if a hidden provider is added).
- [ ] 017-S4 Per-provider model memory persists across provider switches.

## Plan 018 — provider graduation (code `2026-06-13`, experimental; graduate per provider)

Each provider graduates `experimental`→`production` ONLY after its row passes
with a real key (flip overlay status + `python3 generate.py` + commit). Without
a key it stays `experimental` — acceptable end state, not a failure.

- [ ] 018-OpenRouter real key → select `openai/gpt-4.1-mini` → polish round-trip;
      invalid key → InvalidApiKey error; then flip overlay to production.
- [ ] 018-Groq real key → `llama-3.3-70b-versatile` (routes `groq::`) polish
      round-trip; reasoning control hidden; then flip to production.
- [ ] 018-common forced failure (cut network mid-polish) → raw-transcript
      fallback + "polish failed" notice; app stays responsive.

## Release rule

015 + 016 smoke are ship gates for the AI-polish release; 004/008 smoke are
ship gates for the recording-path release. None block further feature
development on this branch.
