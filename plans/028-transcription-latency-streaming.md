# Plan 028: Transcription latency + streaming ("fast af" dictation)

> **STATUS: TODO — investigation + design (no code yet).** Drafted 2026-06-20,
> scout-verified against live code and source-verified against pinned library
> sources (whisper-rs 0.16.0, FluidAudio 0.15.2 rev `7f963cd`). Not claimed, not
> executing. Each **Phase** graduates to its own executable plan file (029, 030, …)
> when claimed, per the `plans/README.md` concurrency protocol; this file is the
> index/design. The goal is the user's: "from hitting the hotkey to getting the
> transcription, no extra delay" + streaming for Whisper/Parakeet/cloud.

## Status

- **Priority**: P2 (feel/perf; behind the 2.0.0 smoke gates and the hot-path risk bar)
- **Effort**: XL (multi-phase; per-phase effort below). Phase 0 alone is S.
- **Risk**: Phase 0 LOW (pure plumbing); Phases 1–5 MED→HIGH (new live audio path on the dictation hot path — the #1 trust killer per plan 015).
- **Depends on**: 015/020 smoke (shared executor + watchdog/never-lose-speech invariants are the seam this builds on).
- **Category**: latency / UX-feel / new capability (streaming)
- **Planned at**: 2026-06-20

## TL;DR — the decision the numbers force

1. **For Parakeet, latency today is ~100% plumbing, not decode.** FluidAudio TDT
   batch runs ~120× RTF on Apple Silicon (a 5 s clip decodes in tens of ms). The
   perceived stop→text delay is dominated by **~450 ms of stacked fixed sleeps in
   the paste path + a 500 ms blocking clipboard restore + an ffmpeg process spawn
   to normalize every recording**. None of that is decode. **Phase 0 (plumbing) is
   the single highest-ROI, lowest-risk win and is independent of streaming.**
2. **For Whisper, latency IS decode-bound** (especially `large-v3` on CPU / Windows
   Vulkan). Best levers: faster model (`large-v3-turbo` ≈ 6× faster), `set_audio_ctx`
   for short clips, smaller beam, and **chunked decode-ahead** at silence
   boundaries. whisper-rs has **no KV-reuse streaming** — true Whisper "streaming"
   is redundant sliding-window re-decode; avoid it on CPU.
3. **Parakeet streaming needs NO library bump.** FluidAudio 0.15.2 already ships
   `StreamingAsrManager` (EOU/Nemotron, cache-aware, partial callbacks) and
   `SlidingWindowAsrManager` (reuses the current TDT v3 model). But since batch
   Parakeet is already ~instant for short clips, streaming's payoff is **long-form
   decode-ahead + live-partial UX**, not short-clip raw latency.
4. **Cloud: 3 of 5 providers stream.** OpenAI (realtime WS), Deepgram (live WS),
   Soniox (real-time WS). Groq + Cohere are batch-only. Our *recommended* provider
   (Soniox) currently uses REST async-poll with a **1000 ms poll-interval floor** —
   switching it to its WebSocket is a clear win.
5. **The hard part of "streaming" for dictation is insertion, not decoding.**
   Revisable partials (Parakeet volatile, Deepgram interim, Soniox non-final)
   cannot be naively pasted at the cursor. Recommended order: **decode-ahead first**
   (decode during recording, still insert once at stop), then optional **pill
   preview**, then maybe **type-as-you-speak** incremental insert.

## Evidence A — the latency budget (hotkey → text, macOS local success path)

All fixed/controllable delays on the *successful* path (engine decode excluded).
Paths verified: capture = `src-tauri/src/audio/*`, hotkey = `src-tauri/src/recording/hotkeys.rs`
(the live path; `src-tauri/src/trigger/*` is the separate native-trigger engine, plan 022).

| # | Step | Evidence | Fixed cost |
|---|------|----------|-----------|
| 1 | Hotkey accepted (toggle/PTT) | `recording/hotkeys.rs:136-197,232-286` | 0 ms (300 ms throttle only drops *duplicate* presses, `:157-162`) |
| 2 | `start_recording` validate/config | `commands/audio.rs:3308-3508` | 0 ms (store/device-bound) |
| 3 | Start sound | `commands/audio.rs:3401-3417` | **0 ms** — plan 015's 300 ms sleep is gone (confirmed; only a `from_secs(300)` watchdog *test* remains, `audio.rs:1455-1456`) |
| 4 | Device/recorder init (CPAL build, `stream.play()`) | `audio/recorder.rs:139-442` | device-bound; **duplicate device enumeration** at `recorder.rs:145-155,185-191` |
| 5 | Stop → final WAV (callback drain + writer finalize) | `audio/recorder.rs:454-463,468-502,568-581` | 0–200 ms drain + up to 100 ms poll granularity (5000 ms failure cap; Windows 3000 ms stream-drop) |
| 6 | **Normalize to 16 kHz mono via ffmpeg** | `commands/audio.rs:4404-4420` → `ffmpeg/mod.rs:195-201` (`normalize_streaming` → whole-file `to_wav_streaming`) | **ffmpeg process spawn + file I/O on every non-16 kHz recording** (i.e. almost always on macOS 44.1/48 kHz mics). In-process `audio/normalizer.rs` + `audio/resampler.rs` (rubato) already exist but are NOT wired to the desktop path |
| 7 | Min-duration gate | `commands/audio.rs:4430-4474` | gate at 500 ms (rejects <0.5 s) |
| 8 | **Engine decode** | engines | **the variable cost** — see Evidence B |
| 9 | Pre-insert UI-stabilize sleep | `commands/audio.rs:4958-4959` | **50 ms** |
| 10 | `insert_text` pre-delay | `commands/text.rs:99-101` | **50 ms** |
| 11 | Clipboard set + settle | `commands/text.rs:194-209` | **50 ms** |
| 12 | macOS paste (rdev): pre 50 + initial 50 + 4×50 key events | `commands/text.rs:449-521` | **300 ms** (+50 ms on retry) |
| 13 | Clipboard restore (blocks `insert_text` return) | `commands/text.rs:178-179,231-238` (`CLIPBOARD_RESTORE_DELAY = 500`) | **500 ms** (text already visible, but Idle/history wait on it) |

**Fixed post-decode tail ≈ 450 ms before text appears + 500 ms blocking restore ≈ 950 ms of pure plumbing**, every time, on every engine. AI formatting (`config.ai_enabled` + AI-requiring preset, `audio.rs:4755-4814`) adds a network hop only when enabled.

First-transcription cold start: model preload is async on startup (`lib.rs:1065-1115`) and on model-select (`settings.rs:768-790`); whisper cache holds **exactly 1** model (`whisper/cache.rs:8-10,34-35`). Cold CoreML/ANE (Parakeet) and GGML/Metal (Whisper) compile is paid on first decode if preload hasn't finished.

## Evidence B — how each engine decodes today (all batch / whole-file)

**Whisper** (`whisper/transcriber.rs`): one whole-file `state.full(params, &resampled_audio)` at `:623`, fresh `state` per call at `:585`, abort callback wired at `:621` (plan 015 landed). Params at `:481-581`: CPU = `Greedy{best_of:1}`, `no_context=true`, `no_timestamps=true`; Apple-Silicon Metal = `BeamSearch{beam_size:5, patience:-1}`, timestamps on. `set_audio_ctx` is **not** set (= full context). Segment text is emitted but `start_ms/end_ms` are `None` (`:692-693`). Windows path is a Vulkan **sidecar** (`whisper/gpu_sidecar.rs`): newline-JSON over a child process, `Transcribe{model_path,audio_path,...}` (`:57-78`), timeout `ceil(s)*4000+60000` clamped 180k–1.8M ms (`:616-635`). Model speed/size (`whisper/manager.rs:103-163`): `base.en` 148 MB (speed 8), `small.en` 488 MB (7), `large-v3-turbo` 1.62 GB (7, "6× faster than large-v3"), `large-v3` 3.1 GB (2).

**Parakeet** (`parakeet/*` + `sidecar/parakeet-swift/Sources/main.swift`): long-lived Swift sidecar, newline-JSON over stdin/stdout (`sidecar.rs:117-124,181-185`; Swift redirects native stdout→stderr around FluidAudio, `main.swift:52-66`). Whole-file only: `Transcribe{audio_path,...}` (`messages.rs:42-62`; Rust sends a path, `manager.rs:416-427`). Swift uses **batch** `manager.transcribe(fileURL, decoderState:&decoderState)` (`main.swift:478-482`). Dead config: Rust sends `chunk_duration:120s/overlap:15s/attention:local` (`manager.rs:307-318`) that Swift **ignores** (`main.swift:237-253`). `duration=0.0` is guarded by a WAV-header fallback in Rust (`executor.rs:406-418`).

**Cloud** (`cloud_stt/*`): all batch. OpenAI/Groq/Cohere = REST multipart upload; Deepgram = REST raw-body `/v1/listen`; **Soniox = REST async (upload → create job → poll every 1000 ms, 180 000 ms ceiling)** — not its WebSocket (`soniox.rs:158-198,354-390`). Shared client 120 000 ms request / 15 000 ms connect, retry-once after 400 ms (`common.rs:68-85`). Executor cloud route returns only a final `TranscriptionResult::new(job, text)` (`executor.rs:240-257`); no partials. `ProviderCapabilities` has **no streaming flag** (`provider_capabilities.rs:17-24`).

## Evidence C — streaming feasibility, source-verified

### Whisper — segmented decode-ahead only (no KV reuse) — VERIFIED against the user's references
whisper-rs 0.16.0 (`src/whisper_params.rs`, `whisper_state/*`):
- **No KV/encoder-state-reuse streaming API exists** — `WhisperState::full(&[f32])` takes a complete slice each call and runs `whisper_full_with_state`; there is no append-PCM/reuse-encoder path. True incremental decode would require patching whisper.cpp's C core. Confirmed by both repos the user pointed at (box below): neither adds KV reuse.
- Relevant knobs **confirmed present**: `set_audio_ctx(n)` (`:186-253`, "0 = default"; the key short-clip speed knob), `set_single_segment`, `set_no_context`, `set_n_threads`, `set_temperature`, `Greedy{best_of}` vs `BeamSearch{beam_size,patience}` (`patience` not implemented in whisper.cpp v1.7.6).
- `set_abort_callback_safe` (`:574-654`, `true`=abort) — already used.
- **`set_segment_callback_safe(SegmentCallbackData{segment,start,end,text})`** (`:397-479`) fires **during** `state.full(...)` as segments are created → enables emitting *completed-segment* text live (not token-by-token partials).
- The viable pattern is **segmented decode-ahead**: decode the captured buffer in chunks *during* recording and finalize completed segments, so stop only flushes the tail. whisper.cpp's `examples/stream` does the naive version (sliding window, re-decodes overlap); scribble does it smarter (box).

> **Verified 2026-06-20 — the user's two references (read at source).**
> - **`itsmontoya/scribble`** (MIT, `whisper-rs = 0.15.1`) *does* stream on whisper-rs — exactly via segmented decode-ahead, **NOT** KV reuse. `src/backends/whisper/incremental.rs` keeps a **growing sample buffer**, runs a fresh `whisper_full` once a min-window is reached (`Opts.incremental_min_window_seconds`), emits all-but-the-last segment as **final**, then **advances the buffer head past the emitted audio** so finalized audio is never re-decoded. This *validates* Phase 3 and gives a proven reference algorithm (steal the advance-head boundary).
>   - Embeddable as a library? Only at the backend layer: public `Backend`/`BackendStream` (`on_samples(&[f32] 16 kHz mono) -> Result<bool>`, `finish()`) + `WhisperBackend`; the demux/VAD/live pipeline are **private** (`audio_pipeline`/`decoder`/`samples_rx` are `pub(crate)`). It pins **whisper-rs 0.15.1** vs our 0.16 (duplicate-build/downgrade friction) and drags `symphonia[all]`. → **Lean: reimplement its ~100-line loop on our 0.16** (reuses our transcriber/model-mgmt/Vulkan sidecar), citing scribble as the reference.
> - **`moinulmoin/whisper-rs`** is **upstream whisper-rs 0.16.0 with ZERO code changes** — the only diff vs upstream `7558e1b` is a PR template + `AGENTS.md`/`CLAUDE.md` (anti-AI notes). No streaming, no new params/callbacks. Switching to it (git dep) is API-equivalent and buys latency **nothing**.

### Parakeet — FluidAudio 0.15.2 already ships true streaming (no bump)
Verified in the local checkout `sidecar/parakeet-swift/.build/checkouts/FluidAudio/Sources/FluidAudio/ASR/Parakeet/`:
- **`StreamingAsrManager` protocol** (`Streaming/StreamingAsrManager.swift:7-54`): `loadModels()`, `appendAudio(AVAudioPCMBuffer)`, `processBufferedAudio()`, `finish() -> String`, `reset()`, `cleanup()`, `setPartialTranscriptCallback(@Sendable (String)->Void)`, `getPartialTranscript()`.
- **Concrete engines** via `StreamingModelVariant.createManager()` (`Streaming/ParakeetModelVariant.swift:8-117`): `StreamingEouAsrManager` (Parakeet EOU 120M, chunks 160/320/1280 ms, cache-aware encoder+RNNT state, partial callbacks + **end-of-utterance** detection, ~5×@160 ms/~12×@320 ms RTF) and `StreamingNemotronAsrManager` (Nemotron 0.6B, 560/1120/2240 ms, 42–93× RTF). **These require new CoreML model downloads.**
- **`SlidingWindowAsrManager`** (`SlidingWindow/SlidingWindowAsrManager.swift:113-220`): `startStreaming()`, `streamAudio(AVAudioPCMBuffer)`, `transcriptionUpdates` AsyncStream, `finish()`. **Reuses the current TDT v3 model — no new download.** But it is *pseudo*-streaming: defaults chunk 15 s / hypothesis 2 s / left-context 10 s / **min-confirm 10 s** (`:698-706`); `.streaming` preset 11 s/1 s (`:709-718`). So for short dictation (2–8 s) confirmed text barely promotes before `finish()`; its real value is long-form decode-ahead.
- **Audio format**: both accept *any* AVAudio format and resample to 16 kHz mono Float32 internally (`AudioConverter`, `Shared/AudioConverter.swift:7-34`) — Rust can ship native-rate PCM and let Swift convert.
- **Partials are revisable** (volatile/confirmed two-tier, `isConfirmed`; EOU/Nemotron emit accumulated transcript). Plan accordingly for insertion.

### Cloud — streaming matrix (vendor-confirmed, 2026)
| Provider | Today (code) | Streams? | How |
|---|---|---|---|
| OpenAI | batch multipart `openai.rs` | **Yes** | Realtime WS/WebRTC (`gpt-realtime-whisper`, tunable `delay`; sub-second) + `stream=true` SSE on gpt-4o-transcribe |
| Deepgram | batch raw-body `/v1/listen` | **Yes** | Live WS, ~150 ms first word, interim + `is_final`/`speech_final` |
| Soniox | **REST async-poll** `soniox.rs` (1 s floor) | **Yes** | Real-time WS, token-by-token `is_final`, ~249 ms median, `max_non_final_tokens_duration_ms` |
| Groq | batch multipart `groq.rs` | No | REST only (fast batch, 216× RTF, no interim) |
| Cohere | batch multipart `cohere.rs` | No | hosted API batch-only |

Candidate not yet integrated: AssemblyAI Universal-Streaming (~300 ms, immutable transcripts).

## Prerequisites shared by ALL decode-ahead / streaming work

1. **Live PCM tap.** Today the only consumers of the CPAL callback are the WAV
   writer, level meter, and silence detector; samples are streamed to disk via a
   bounded `sync_channel` (cap 1024), never exposed mid-recording
   (`audio/recorder.rs:301` `process_audio`, `:323-359`). Add a broadcast / shared
   ring-buffer subscriber at `process_audio` (before/alongside `writer_tx.try_send`).
2. **Stateful chunked resampler.** `audio/resampler.rs` is whole-buffer batch
   (rubato Fft, processes the entire input). A live consumer needs a stateful
   chunk resampler — or push native-rate chunks and resample in the consumer
   (FluidAudio does this itself; whisper/cloud need their own).
3. **A streaming request variant.** `TranscriptionRequest` carries only completed
   `Path`/`Bytes` (`request.rs:9-19`); the executor cloud/local routes return one
   final `TranscriptionResult`. Add a stream/live source variant *before*
   `transcribe_with_app`, with partial/final event callbacks — without mutating the
   batch seam (`executor.rs:240-257`, `provider.transcribe_typed`).
4. **`ProviderCapabilities.supports_streaming`** (currently no streaming model at
   all, `provider_capabilities.rs:17-24`).

## The insertion problem (why "streaming" ≠ "paste partials")

Dictation inserts at the cursor via clipboard paste (`commands/text.rs`). Revisable
partials can't be pasted then un-pasted in arbitrary apps. Three insertion modes,
increasing risk:
- **Decode-ahead, insert-once-at-stop (RECOMMENDED first):** decode during
  recording so by stop only the tail remains; still one paste of the final text.
  Collapses decode latency with **zero** new UX risk. Helps long dictation most.
- **Pill preview:** show live partials in the recording pill (NSPanel), never the
  editor. Visible responsiveness, no editor corruption risk.
- **Type-as-you-speak:** incremental insert into the focused app with revision
  handling (backspace/replace). Highest "juice", highest risk — defer.

## Phased plan

### Phase 0 — Kill the fixed plumbing tail [S, LOW risk, all engines] → plan 029
The biggest immediate "no extra delay" win; **no streaming required**.
- Make clipboard restore **non-blocking** (don't hold `insert_text` 500 ms; restore
  async, guarded by "clipboard still equals our transcript") — `text.rs:231-238`.
- Trim/merge the stacked 50 ms sleeps: pre-insert (`audio.rs:4958`), `insert_text`
  pre-delay (`text.rs:99`), clipboard settle (`text.rs:208`).
- Tighten macOS paste: replace rdev's 50+50+4×50 ms with a single CGEvent
  Cmd-V batch (`text.rs:449-521`) — keep a verified fallback.
- Replace the **ffmpeg normalize spawn** on the desktop path with the existing
  in-process `audio/normalizer.rs`/`resampler.rs` (or capture-at-16 kHz when the
  device supports it) — `audio.rs:4404-4420`.
- Drop duplicate device enumeration on start (`recorder.rs:145-155,185-191`).
- **Acceptance**: measured stop→text on a fixed clip drops by the tail budget
  (~target ≥300 ms) with no paste-reliability or clipboard-clobber regression;
  never-lose-speech invariants (plan 015) intact. Manual paste QA across apps.

### Phase 1 — Decode-ahead foundation [M] → plan 030
Build the 4 prerequisites above (live tap, stateful resampler, streaming request
variant, capabilities flag). No engine behavior change yet; ship behind a flag with
the live tap feeding a no-op consumer + tests. **Acceptance**: mid-recording PCM is
observable by a second consumer; batch path byte-identical; gates green.

### Phase 2 — Parakeet streaming (macOS) [L] → plan 031
- **v1 (no new model):** `SlidingWindowAsrManager` decode-ahead reusing TDT v3 —
  feed chunks during recording, `finish()` at stop returns fast.
- **v2 (new model, best latency):** `StreamingEouAsrManager` (EOU 120M) for true
  low-latency partials + end-of-utterance.
- IPC: extend `messages.rs` to a session protocol (`StartStream`/`AudioChunk`/
  `EndStream`/`CancelStream` + `Partial`/`Final`); prefer a binary side-channel over
  base64-in-JSON. Swift: own a streaming manager at the `main.swift:267-273`
  dispatch + `transcribeFile` (~`:446-503`).
- **Acceptance**: long-dictation stop→text collapses to ~tail; short-dictation no
  worse than Phase 0; cancel/partial paths tested.

### Phase 3 — Whisper chunked decode-ahead [L] → plan 032
Reference impl: `itsmontoya/scribble`'s `incremental.rs` (growing buffer → decode at min-window → emit all-but-last segment as final → advance head past emitted audio). **Build-vs-embed:** reimplement that ~100-line loop on our own `whisper-rs 0.16` (reuses `whisper/transcriber.rs`, model-mgmt, and the Vulkan sidecar; no 0.15↔0.16 skew) rather than embedding the `scribble` crate. Embedding is the fallback.
- Boundary options: Whisper's own segment end-timestamps (scribble's trick) and/or our existing silence detector (`audio/silence_detector.rs`). Decode finalized spans during recording with `no_context=true`, `single_segment`; mirror in the Vulkan sidecar protocol.
- Param/model knobs: `set_audio_ctx` for short clips, `large-v3-turbo` default consideration, beam tuning. `set_segment_callback_safe` → pill partials.
- **Acceptance**: long-dictation latency drops without WER regression beyond an agreed bound; short-clip path unchanged or faster.

### Phase 4 — Cloud streaming [M] → plan 033
- Soniox real-time **WS** first (recommended provider; removes the 1 s poll floor),
  then Deepgram live WS, then OpenAI realtime. Sibling live path beside
  `transcribe_typed` (`cloud_stt/mod.rs:152-164`); set `supports_streaming`.
- **Acceptance**: streaming providers deliver first text well under the current
  batch round-trip; batch providers (Groq/Cohere) unaffected; key/error taxonomy
  reused.

### Phase 5 — Live partial UX [M→L] → plan 034
- Pill preview of partials (all streaming engines). Then evaluate type-as-you-speak
  incremental insert as a separate, opt-in experiment with revision handling.

## Open decisions (need user input before claiming any phase)

1. **Scope/order**: ship Phase 0 (safe, immediate "no extra delay") now, independent
   of streaming? (Strongly recommended.)
2. **Parakeet**: SlidingWindow reusing the current model (v1, no download) first, or
   go straight to the EOU model (new ~120 M CoreML download) for best latency?
3. **UX**: decode-ahead-only (insert at stop) vs pill preview vs type-as-you-speak?
4. **Cloud**: is Soniox-WS the priority, and do we keep batch providers as-is?
5. **Quality bar**: acceptable WER delta for `audio_ctx`/turbo/streaming tradeoffs.
6. **Whisper Phase 3 build-vs-embed**: reimplement scribble's segmented decode-ahead on our own whisper-rs 0.16 (recommended), or embed the `scribble` crate (whisper-rs 0.15 skew + private pipeline)?

## STOP conditions

- If a Phase-0 sleep removal causes paste to use the *previous* clipboard or steals
  focus → restore that delay; correctness over speed.
- If the live tap perturbs the CPAL hot path (xruns/drops) → back out; plan 008's
  no-allocation/no-drop invariants are non-negotiable.
- If a streaming partial is ever *pasted* and then revised in a user's app → stop;
  fall back to insert-at-stop. Never corrupt the user's document for "juice".
- If decode-ahead can lose tail speech vs batch → stop; plan 015's never-lose-speech
  invariants win.
