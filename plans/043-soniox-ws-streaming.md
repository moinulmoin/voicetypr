# Plan 043 — Soniox WebSocket streaming (real live preview, multilingual)

> Authored from `docs/review/grounding-soniox-ws.md` (ultracode 2026-07-03; external
> API facts verified against soniox.com with source URLs). This is the slice that
> makes live preview ACTUALLY WORK for a shipping engine — Soniox streams interim
> tokens over WS, multilingual, no local-model dependency. It plugs into the exact
> streaming substrate (037-042) and the capability-driven "Transcription mode" toggle
> already built.
>
> ⚠️ VERIFICATION CEILING: the live WS handshake needs a Soniox API key in the user's
> secure store. Codex verifies compile + unit tests (token→committed/tentative mapping,
> capability flip, sync/async bridge, gate generalization); the live end-to-end
> handshake is a MANUAL smoke the user runs with their key. State this honestly.
>
> ⚠️ FRESHNESS: slice 045 touched audio.rs/stream_tap.rs/sidecar.rs streaming paths, so
> the grounding's line refs there may have shifted. Codex MUST re-verify every file:line
> before editing and stop-and-report on mismatch. (stream_tap.rs trait confirmed still at
> :28-31; the Parakeet sink/gate refs in audio.rs likely moved.)

## Where it plugs in (verified substrate)

- **RT tap → sink trait** (`stream_tap.rs:28-31`, confirmed current):
  `trait StreamTapSink { fn send_frame(&mut self, &[i16]); fn finalize(&mut self) ->
  Option<String>; fn cancel(&mut self); }` + `StreamTapSinkFactory = Arc<dyn Fn(u32,u16)
  -> Option<Box<dyn StreamTapSink>>>`. Factory gets `(sample_rate, channels)` — exactly
  what Soniox `pcm_s16le` raw config needs. **No resampling** (native-rate i16 frames).
- **Reference sink:** `ParakeetPreviewStreamSink` (grounding cited audio.rs:79-157 —
  re-locate) bridges the sync worker thread to an async handle, emits
  `TranscriptionStreamEvent`s to the pill via a `StreamSessionGate`. Copy the shape (with
  a hard finalize timeout).
- **Contract** (`transcription/stream.rs`): `Started/Partial{committed,tentative}/
  Final{text}/Cancelled/Error` with `session_id` (recording generation) + monotonic
  `revision`; `StreamSessionGate::admit` drops stale; `assert_committed_monotonic` enforces
  committed is byte-prefix-append-only.
- **Frontend:** `pill.tsx`, `types/streaming.ts`, `RecordingPill.streaming.test.tsx`
  already render committed/tentative — NO schema change.

## Soniox RT WebSocket API (verified 2026-07)

- **Endpoint:** `wss://stt-rt.soniox.com/transcribe-websocket` — DIFFERENT origin than REST
  `api.soniox.com` (warm-up currently warms the wrong origin for RT).
- **Auth:** first frame is JSON config with `"api_key"` in the BODY (not a header). Desktop
  holds the key in the encrypted store — no temp-key minting needed.
- **Config:** `model: "stt-rt-v5"` (pairs with our async `stt-async-v5`), `audio_format:
  "pcm_s16le"` + `sample_rate` + `num_channels` (raw), `language_hints[]`, `context{general,
  text,terms}` (our `SonioxContext` is a subset — reuse `compile_soniox_context`),
  `enable_endpoint_detection: true` (fast dictation commits), `enable_language_identification`.
- **Audio:** binary frames after config. Our ~10-40ms callback frames are fine (optionally
  coalesce toward the docs' 120ms/3840-byte cadence). Max 300 min/stream.
- **Responses (JSON):** `{tokens:[{text,start_ms,end_ms,confidence,is_final,speaker,language}],
  ...}`. Non-final tokens "may change/disappear/be replaced" and are **reissued each
  response**; final tokens "sent only once, never repeated". RT tokens carry their OWN
  leading spaces — do NOT copy the space-inserting join from REST `soniox.rs:230-241`.
- **Close:** send an **empty frame** → server flushes → `{...,"finished":true}` → closes.
  `{"type":"finalize"}` forces pending final mid-stream; `{"type":"keepalive"}` required if
  idle >20s (send every ~10s).
- **Errors:** in-band `{tokens:[],error_code,error_type,error_message,...}` with HTTP-style
  codes — a MESSAGE, not an HTTP status. Needs an int→`SttError` map parallel to
  `classify_status` (common.rs:44-55).
- **Billing:** charged for FULL stream duration incl. keepalive idle — never hold the socket
  open between recordings.

## Mapping onto `TranscriptionStreamEvent`

- `Started { engine: "soniox" }` after WS open + config sent, revision 0.
- `Partial`: `committed` = running concat of ALL `is_final:true` token texts, concatenated
  VERBATIM (tokens carry own spaces) — append-only, satisfies `assert_committed_monotonic`.
  `tentative` = concat of the current response's `is_final:false` tokens, wholly replaced
  each response. `revision` = per-response `AtomicU64`.
- `Final` on worker `Finalize`: send empty frame, drain (fold late finals into committed)
  until `finished:true` or timeout; `text` = committed total.
- `Cancelled` on `Cancel`/stale generation: close without finalize.
- `Error` on in-band error / transport failure: map code, emit, let executor REST-on-WAV
  produce the authoritative result.

## Scope

1. **New dep:** a WS client (`tokio-tungstenite` + rustls, or `reqwest-websocket`) — enforce
   `wss` (mirror `https_only` rationale, common.rs:106-113), TLS consistent with reqwest 0.13.
2. **`SonioxStreamSink`** implementing `StreamTapSink`: sync `send_frame` → bounded tokio mpsc
   → spawned WS task (mirror `ParakeetStreamHandle`); NEVER block the RT worker; async connect
   + local frame buffering until config-ack (must NOT block recorder start under the recorder
   mutex — grounding risk #2). `finalize()`'s `block_on` gets a HARD timeout (~1-2s) then REST
   fallback. Config frame NEVER logged (api_key hygiene).
3. **Result authority (the biggest decision — grounding risk #1):** with a WS sink both the
   stream AND the executor's REST-on-WAV run → double billing (Soniox bills full duration).
   Decision for this plan: **WS `Final` becomes the pasted result; REST-on-WAV runs only as
   fallback on WS error/gap.** This flips the Parakeet stance (batch authoritative); the
   worker's `finalize()` discard (stream_tap.rs:299-303) must change for Soniox-authoritative
   mode. Wire this behind the mode so Parakeet stays batch-authoritative.
4. **Capability flip:** `EngineStreamCapabilities::SONIOX` (stream.rs:83)
   FINAL_ONLY → `{supports_streaming, supports_committed_prefix, supports_tentative_tail,
   supports_endpointing: true, final_only: false}`. Updates the pinned truth-table test at
   stream.rs:246-271 (intentional site).
5. **Generalize the gate:** `build_parakeet_stream_sink_factory`'s engine check and the
   `parakeet_streaming_enabled` clamp assume one engine — make the factory dispatch on
   `EngineStreamCapabilities::for_engine(current_engine)` + engine id, so Soniox registers by
   its capability row + `SonioxStreamSink`. Suppress WS streaming when a remote connection is
   active (same check as warm-up). Rename the `parakeet`-specific emit helper to be engine-neutral.
6. **Warm-up:** add a warm for `stt-rt.soniox.com` (current warm hits `api.soniox.com`).
7. **Keepalive:** 10s idle timer sending `{"type":"keepalive"}`; never hold the socket between
   recordings.
8. **Reconnect (grounding risk #3):** on mid-stream drop, committed prefix is safe locally;
   accept the preview gap and rely on REST-on-WAV for the authoritative final (do NOT build
   audio-replay reconnection in this slice — note it as a follow-up).

## Tests
- Token→committed/tentative mapping: finals accumulate verbatim + monotonic; tentative
  replaced each response; reissued non-finals don't corrupt committed.
- int error_code → SttError map.
- capability truth-table update.
- sync→async bridge: `send_frame` never blocks; finalize timeout → falls back.
- gate generalization: factory returns the Soniox sink for soniox engine + live_preview,
  None otherwise; suppressed under active remote connection.

## Acceptance
- `cargo test` + clippy + `pnpm typecheck/lint/test` green.
- Default (regular mode / non-soniox engine): zero behavior change.
- Live WS handshake = MANUAL smoke by the user with a Soniox key (documented commands).
- Result-authority + billing decision documented in the plan Outcome after implementation.
- Do not commit.
```
