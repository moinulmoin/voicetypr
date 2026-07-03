# Plan 044 — Whisper WER-gated wins (auto-detect fix + adaptive audio_ctx + speed mode)

> Authored from `docs/review/grounding-whisper-wins.md` (ultracode 2026-07-03,
> file:line-verified against this branch). Whisper files (transcriber.rs,
> languages.rs, settings.rs, manager.rs, cli.rs, gpu_sidecar.rs) were NOT touched
> by slice 045, so the grounding refs are current — but Codex MUST re-verify each
> file:line before editing and stop-and-report on any mismatch.
>
> Three INDEPENDENTLY-GATED sub-changes. Each ships through the perf-harness WER
> gate SEPARATELY (own A/B run, own commit). Do them in order; do not bundle.

## The gate (applies to every sub-change)

Harness: `scripts/perf-harness.mjs` (runs `<bin> transcribe --file … --engine whisper
--model … --language <lang> --json`; per-language WER + `last_word_present`; emits
`perf-report.{json,md}`). Corpus: `scripts/gen-corpus.mjs` (en/fr/de/es/it/pt/nl +
sv/pl, 2s/5s/15s). **Synthetic corpus = relative deltas only, never absolute WER.**
Reps ≥ 5. Requires the `base` (and for sub-change C, `large-v3-turbo`) Whisper model
downloaded — the harness SKIPs uninstalled models.

```
node scripts/gen-corpus.mjs --out perf-corpus/synthetic          # one-time
cd src-tauri && cargo build --release && cd ..
cp src-tauri/target/release/voicetypr /tmp/voicetypr-baseline    # before the change
# after each change: rebuild, then A/B baseline-bin vs candidate-bin, diff the reports
```

---

## Sub-change A — fix `auto` → `en` (multilingual correctness bug)

**The bug:** `language=auto` is squashed to `en` in *four* places, so a French speaker
with auto-detect gets transcribed as English. Squash happens BEFORE the transcriber:
1. `whisper/languages.rs:438-457` `validate_language()` returns `"en"` for any code not
   in `SUPPORTED_LANGUAGES` (`auto` isn't listed; test at :475 asserts this).
2. `commands/settings.rs:299-351` `normalize_speech_language_for_model()` calls
   `validate_language` at :304 — every dispatch path (desktop `audio.rs:5328`, upload/CLI
   `audio.rs:6685`, bytes `audio.rs:7051`) converts auto→en before the transcriber.
3. `whisper/transcriber.rs:504-517` the transcriber's own auto→`Some("en")` ("30-second
   requirement" comment at :506 — stale folklore; whisper.cpp v1.8.3 auto-detects on
   short zero-padded clips fine).
4. Vulkan sidecar `sidecar/whisper-vulkan/src/main.rs:288-291`.

**What whisper.cpp does with auto** (vendored v1.8.3, verified): null/empty/"auto"
language → `whisper_lang_auto_detect_with_state` (1 extra encoder + 1-token decode),
stores `state->lang_id`, decodes in the detected language. `set_language(None)` in
whisper-rs 0.16 sets `fp.language = null` → same real auto-detect. `set_detect_language(true)`
is WRONG (returns early after detection, no transcription).

**Design:**
- `settings.rs:303-306` in `normalize_speech_language_for_model`: short-circuit
  `if speech_language == "auto" && engine == "whisper" && !model_requires_english_speech(...)
  { return "auto".to_string() }`. `.en` models (settings.rs:291-297) stay forced-en;
  parakeet/soniox/cohere arms unchanged.
- `languages.rs`: `validate_language` passes `"auto"` through (explicit arm before the
  supported check); update the test at :475.
- `transcriber.rs:504-522`: for `Some("auto")` set `final_lang = None`, call
  `params.set_language(None)` (null ptr, no CString alloc). Delete the "no longer
  supported" branch.
- **Detected-language reporting:** `transcriber.rs:781-785` fabricates `transcript_language`
  from `final_lang`; with auto that's `None`. After `state.full(...)` (:640) read the real
  one: `state.full_lang_id_from_state()` + `whisper_rs::get_lang_str(id)`, used when
  `final_lang.is_none() && !translate`.
- **Vulkan sidecar:** `main.rs:288-291` → `Some("auto") | None => None`; switch its
  `resolve_transcript_language` (`main.rs:250-266`) to `full_lang_id_from_state` (drop the
  redundant `lang_detect` encode).
- **Cost note (document, don't fight):** auto adds ~1 encoder pass (detect runs at full
  audio_ctx before `exp_n_audio_ctx` is applied).
- **UI (include):** add `{ value: "auto", label: "Auto-detect" }` gated to whisper engine
  in `LanguageSelection.tsx:22` / `ModelsSection.tsx:783`. `.en`-model force-en reset
  already at `ModelsSection.tsx:510-513` and `ModelsTab.tsx:96`.

**Gate A:** harness needs a `--language-mode manifest|auto` flag (default manifest) that
substitutes `"auto"` in `runCli` (perf-harness.mjs:158-159). Then: `auto` per-language WER
within noise of forced-language WER for EVERY corpus language; `last_word_pass_rate`
unchanged; the forced-language (non-auto) run byte-identical to baseline (the fix must not
perturb the explicit-language path).

---

## Sub-change B — length-adaptive `audio_ctx` default (short-clip speed, plan-015 gated)

**Plumbing today:** `audio_ctx: Option<i32>` exists end-to-end (plan 032) but is only ever
`Some` from the CLI. Product paths pass `None` (executor.rs:197, audio.rs:7082,
remote/transcription.rs:398). Set at `transcriber.rs:586-589` via `params.set_audio_ctx()`.

**Design — the SHARED layer** (`transcriber.rs`, after `resampled_audio` + duration known):
move the duration computation (currently ~:608-609) above the params block, then:
`let effective_audio_ctx = audio_ctx.or_else(|| adaptive_audio_ctx(resampled_audio.len()));`
(explicit CLI override always wins; log branch: `custom`/`adaptive`/`full`).

New pure, unit-testable `fn adaptive_audio_ctx(samples: usize) -> Option<i32>`:
```
positions_per_sec = 50            // 30s = 3000 mel frames @10ms → conv stride 2 → 1500 pos
needed  = ceil(dur_s * 50) + 50   // +50 = 1.0s tail pad (plan-015 never-lose-speech)
rounded = round_up_to_multiple_of_64(needed)
ctx     = max(rounded, 256)       // floor 256 (~5.1s): tiny ctx = known hallucination zone
if ctx >= 1500 → None             // long/multi-window clip → stock full-window behavior
```
The `>=1500 → None` guard is why it only affects short clips; multi-window (>30s) clips are
never trimmed (`exp_n_audio_ctx` applies to every window — whisper.cpp:6951).

**Windows GPU sidecar caveat (must be in scope note):** `audio.rs:1616` — when
`audio_ctx.is_some()` the GPU sidecar is bypassed; the sidecar's `SidecarRequest::Transcribe`
has no audio_ctx field and never calls `set_audio_ctx`. Putting the default in transcriber.rs
covers macOS Metal, Windows CPU fallback, and remote host; **the Vulkan sidecar keeps stock
full-ctx behavior and is OUT OF SCOPE for this sub-change** (a separate gated follow-up would
thread it through the protocol — do NOT bundle).

**Gate B (this is the plan-015 never-lose-speech gate):** per-language WER delta ≈ 0 for
EVERY language and bucket; `last_word_pass_rate == 1.000` on ALL rows (the 2s bucket is the
tail-loss canary); `inference` p50 improvement on 2s/5s buckets is the win purchased. Pre-work:
sweep floor with the existing CLI `--audio-ctx` flag before landing the default.

---

## Sub-change C — speed mode opt-in (flash-attn; turbo as UI steering)

**flash_attn (verified):** whisper-rs 0.16 `WhisperContextParameters.flash_attn: bool`
(context-level, model-load time — not per-run FullParams). Disables DTW; repo has zero DTW
usage (safe). CPU no-op-to-marginal → gate to the Metal branch.

**Wiring:**
- `Transcriber::new(model_path)` → `Transcriber::new(model_path, speed_mode: bool)`; set
  `ctx_params.flash_attn(true)` next to `ctx_params.use_gpu(...)` at transcriber.rs:66-88,
  gated to `is_apple_silicon` (Metal). Windows main-binary path is CPU-only (:173-177).
- `TranscriberCache::get_or_create` (cache.rs:48) carries the flag; cache key at :63 →
  `format!("{path}|fa={flag}")` (MAX_CACHE_SIZE=1 → toggle flip evicts old ctx, correct).
  Update call sites: audio.rs:1668, lib.rs:1146, commands/model.rs:1150,
  remote/transcription.rs:385, tests/mod.rs:104.
- Dispatch reads the setting in `transcribe_whisper_with_acceleration` (audio.rs:1665-1669),
  same pattern as `transcription_acceleration_mode` (audio.rs:1610).
- **Setting:** `whisper_speed_mode: bool` (serde default false) — add to `Settings`
  (settings.rs:37-81), `Default` (:83-117), mirror in `src/types.ts`. UI: "Speed mode"
  toggle in the whisper block of `ModelsSection.tsx` (near :783), copy: "Faster
  transcription (flash attention); pairs best with Large v3 Turbo".
- **Turbo:** `large-v3-turbo` already recommended (manager.rs:136-149). Speed mode must NOT
  silently swap the user's model — wire turbo as UI steering only (badge/recommend when
  speed mode on).
- **Harness hook:** add `transcribe --speed-mode` to `TranscribeArgs` (cli.rs:88-116, next
  to `--audio-ctx`) threaded through `transcribe_audio_file_for_cli` so the gate doesn't
  mutate the settings store. Vulkan sidecar out of scope.

**Gate C:** single-binary A/B (candidate without flag = baseline, with flag = candidate).
WER delta per language within noise on `large-v3-turbo`, last-word 1.000, inference p50/p95
down; report `base` too to prove the toggle is safe on small models.

## Acceptance (whole plan)
- Each sub-change: its gate passes, `cargo test` + `cargo clippy --workspace --all-targets -D
  warnings` + `pnpm typecheck/lint/test` green, own commit.
- `adaptive_audio_ctx` has pure unit tests (boundary: 1s, 2s, 5s, 27s, 30s, 60s).
- Default paths unchanged when the new setting is off / language is explicit.
- Do not commit (Claude commits after gate review).
```
