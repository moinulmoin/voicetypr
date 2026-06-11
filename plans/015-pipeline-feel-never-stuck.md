# Plan 015: Pipeline feel — start latency, decode watchdogs, never-lose-speech

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 080663b..HEAD -- src-tauri/src/commands/audio.rs src-tauri/src/whisper/transcriber.rs src-tauri/src/parakeet/manager.rs src-tauri/src/parakeet/sidecar.rs`
> On drift, compare "Current state" excerpts to live code first; mismatch = STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (touches the dictation hot path; every change has a
  user-visible failure mode if wrong)
- **Depends on**: plans 004/008 smoke completed (recommended, not hard);
  independent of the transcription-contract work — if the executor finds the
  engine dispatch refactored into a shared executor, apply the same changes at
  the equivalent seam.
- **Category**: reliability / UX-feel
- **Planned at**: commit `080663b`, 2026-06-11

## Why this matters

The V2 bar is "it just works, like magic". Three classes of leaks break that
today:

1. **Start latency**: every recording with the start sound enabled sleeps a
   hard 300ms before the microphone is even initialized. Words spoken on the
   sound cue are lost. "It ate my first word" is the #1 dictation trust
   killer.
2. **Unbounded waits**: CPU Whisper decode and Parakeet sidecar transcription
   have no timeout and no effective cancellation — a hang leaves the pill on
   "transcribing" forever. Remote (120s cap), GPU sidecar (180s–30min), and
   AI formatting (35s) all have deadlines; the two most-used local engines do
   not.
3. **Lost speech**: `[SOUND]` (and similar Whisper noise annotations) can be
   pasted as if it were a transcript; and when `process_transcription`
   returns an error (e.g. output-language transform required but AI failed),
   the transcript is discarded entirely (`should_deliver = false`) — the user
   spoke, transcription succeeded, and the text went nowhere.

## Current state

### A. Start-sound gate (`src-tauri/src/commands/audio.rs:2157-2172`)

```rust
    if let Ok(store) = app.store("settings") {
        let play_sound = store
            .get("play_sound_on_recording")
            .and_then(|v| v.as_bool())
            .unwrap_or(true); // Default to true
        if play_sound {
            play_recording_start_sound();
            // Delay to let sound complete before microphone initialization
            // This helps with Bluetooth headsets (e.g., AirPods) that switch audio modes
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            ...
        }
    }
```

The 300ms sleep runs **before** recorder/device init. Audio capture begins
only after it. The comment documents the reason: Bluetooth headsets (AirPods)
switch from A2DP to HFP when the mic activates, which can clip the sound.

### B. Whisper cancellation is preprocessing-only
(`src-tauri/src/whisper/transcriber.rs`)

`should_cancel` is checked at `:328`, `:391`, `:403`, `:469` — all before
inference. The decode itself is:

```rust
        match state.full(params, &resampled_audio) {            // :616
```

with no abort callback registered on `params` (built at `:481-483`,
`FullParams::new(SamplingStrategy::BeamSearch { beam_size: 5, .. })`).
Cancel during decode does nothing until decode finishes on its own.
Crate: `whisper-rs = "0.16.0"` (`src-tauri/Cargo.toml:29`).

### C. No outer deadline on the transcription task
(`src-tauri/src/commands/audio.rs:3242-3251, 3933-3945`)

```rust
    let task_handle = tokio::spawn(async move {
        ...
        update_recording_state(&app_for_task, RecordingState::Transcribing, None);
```

The spawned task runs engine dispatch with no independent watchdog that can
set the cancellation flag while an engine is busy. A naive
`tokio::time::timeout` around the dispatch future is **not enough** for CPU
Whisper because the current CPU path blocks inside `state.full(...)` and the
future will not be polled until that call returns. The handle is stored in
`app_state.transcription_task` (`:3933-3942`) — abortable only by explicit
`cancel_recording`. Whisper has a retry loop (3 attempts, 500ms,
`:3282-3331`) but no per-attempt deadline.

### D. Parakeet transcribe: no cancel, no timeout

`transcribe_with_custom_vocabulary` builds the command and calls plain
`send_command` (`src-tauri/src/parakeet/manager.rs:375-398`):

```rust
        self.send_command(app, &command).await
```

`ParakeetClient::send` (`src-tauri/src/parakeet/sidecar.rs:213-240`) awaits
`sidecar.request(..)` with no deadline. The cancellable machinery **already
exists** for downloads: `request_with_progress_and_cancel`
(`sidecar.rs:81-177`) polls a `cancel_flag: Option<Arc<AtomicBool>>` every
100ms via `tokio::select!` and kills the child on cancel (`:99-121`) — its
cancel error message is download-specific ("Download cancelled by user",
`:108-111`). `ParakeetClient::send_with_progress_and_cancel` exists at
`:242-264`.

### E. Blank/noise filter misses `[SOUND]`
(`src-tauri/src/commands/audio.rs:3530-3534`)

```rust
                if transcription.raw_text.is_empty()
                    || transcription.raw_text.trim().is_empty()
                    || transcription.raw_text == "[BLANK_AUDIO]"
                {
```

Whisper emits other non-speech annotations (`[SOUND]`, `[MUSIC]`, `[NOISE]`,
`(silence)` variants); `[SOUND]` is even logged as no-speech inside the
transcriber but still returned as text.

### F. Formatting hard-failure discards the transcript
(`src-tauri/src/commands/audio.rs:3621-3694`)

On `process_transcription` Err: toast is shown, then
`(text_for_process.clone(), None, should_deliver /* = false */)` and:

```rust
                    if !should_deliver {
                        update_recording_state(&app_for_process, RecordingState::Idle, None);
                        return;
                    }
```

Note `resolve_smart_formatting_outcome`
(`src-tauri/src/writing.rs:1698-1720`) already falls back to deterministic
text for non-translation AI failures — so this Err branch fires only for
**hard** failures: output-language transform required and AI failed, or an
AI-required mode that cannot run. In those cases the user's speech is
discarded: not inserted, not copied. Whether it reaches history must be
verified in Step 5.

## Commands you will need

| Purpose            | Command                                        | Expected on success |
|--------------------|------------------------------------------------|---------------------|
| Compile            | `cd src-tauri && cargo check`                  | exit 0              |
| Backend tests      | `cd src-tauri && cargo test`                   | all pass            |
| Lint               | `cd src-tauri && cargo clippy -- -D warnings`  | exit 0              |
| Rust format        | `cd src-tauri && cargo fmt --check`            | exit 0              |
| Frontend gates     | `pnpm typecheck && pnpm lint && pnpm test --run` | all pass          |

## Scope

**In scope**:
- `src-tauri/src/commands/audio.rs` — start-sound block, transcription task
  watchdog, blank filter, no-deliver path.
- `src-tauri/src/whisper/transcriber.rs` — abort callback wiring.
- `src-tauri/src/parakeet/manager.rs`, `src-tauri/src/parakeet/sidecar.rs` —
  cancel flag + deadline for transcribe; generalize the cancel error message.
- Soniox live transcription task deadline (transport-level reqwest timeout is
  still a shared-contract follow-up; this plan must at least prevent the UI
  from waiting forever).

**Out of scope**:
- Mic pre-warm / cached device config (follow-up; riskier invalidation).
- Replacing ffmpeg normalization with in-process resampling (follow-up).
- Streaming/chunked decode while recording (post-contract work).
- Soniox transport retries and richer error taxonomy (belongs to the
  shared-contract executor work); this plan only adds a live task deadline.
## Git workflow

- Branch: `advisor/015-pipeline-feel`
- Commit message: `fix: cut start latency, bound transcription waits, never drop speech`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Make the start sound concurrent with mic init

In the block at `audio.rs:2157-2172`: keep `play_recording_start_sound()`,
**delete** the `tokio::time::sleep(300ms)` and its timing log, and update the
comment to state the sound now plays concurrently with device init.

Bluetooth caveat (the reason the sleep existed): on A2DP→HFP switching
headsets, the start sound may now clip. Capture-first is strictly better for
transcription correctness — a clipped chime loses nothing the user said; the
300ms gate loses spoken words. To preserve an escape hatch without a new
setting: none required — the user can disable the sound entirely via the
existing `play_sound_on_recording` setting. Document this tradeoff in the
commit body.

**Verify**: `cargo check` → exit 0. Use the Search tool for
`from_millis\\(300\\)` in `src-tauri/src/commands/audio.rs`; there must be
no hit inside the start-sound block (other 300ms uses elsewhere, e.g. toggle
debounce, are out of scope and must remain).

### Step 2: Wire cancellation into Whisper decode

In `transcriber.rs`, before `state.full(params, &resampled_audio)` (`:616`),
register the existing `should_cancel` closure as whisper's abort callback:

```rust
        params.set_abort_callback_safe(move || should_cancel_for_abort());
```

Implementation notes:
- whisper-rs 0.16 exposes `set_abort_callback_safe` on `FullParams` taking an
  `FnMut() -> bool` where returning `true` aborts inference. Confirm the
  exact signature against the local crate
  (`cargo doc -p whisper-rs --no-deps` or the source in
  `~/.cargo/registry`). If the method does not exist on 0.16 → STOP.
- The callback must be `'static`. Update the live call path explicitly:
  clone `app_state.should_cancel_recording` into an `Arc<AtomicBool>` and
  pass a `move || cancel_flag.load(std::sync::atomic::Ordering::SeqCst)`
  closure through `transcribe_whisper_with_acceleration` and the
  `transcribe_*` family. Do not capture the borrowed Tauri `State<AppState>`
  inside the abort callback.
- When inference is aborted, `state.full` returns an error; map it to the
  existing `"Transcription cancelled"` string so the retry loop's
  cancellation check (`audio.rs:3308-3310`) breaks out instead of retrying.

**Verify**: `cargo check` → exit 0. `cargo test whisper` → existing
whisper tests pass.

### Step 3: Add a sibling watchdog that sets cancellation without dropping engine work

Do **not** wrap the engine dispatch in `tokio::time::timeout` and drop the
future. That does not preempt CPU Whisper while it is blocked inside
`state.full(...)`, and it can abandon a Parakeet sidecar request before the
cancellable request loop has a chance to observe the flag and kill the child.

Instead, in `stop_recording`'s spawned task (`audio.rs:3243`), start a
sibling watchdog task before engine dispatch:

1. Add a helper near the top of `audio.rs` (unit-testable, pure), matching
   the current GPU-sidecar formula (`gpu_sidecar.rs:618-630`):

```rust
/// Deadline for a transcription attempt, scaled to audio length.
/// Mirrors the GPU sidecar policy: ceil(duration_seconds) * 4 + 60,
/// clamped to 180s..30min.
fn transcription_watchdog_budget(audio_duration_ms: Option<u64>) -> std::time::Duration {
    const FLOOR_SECS: u64 = 180;
    const CEIL_SECS: u64 = 30 * 60;
    let timeout_secs = audio_duration_ms
        .map(|ms| ((ms.saturating_add(999)) / 1000).saturating_mul(4).saturating_add(60))
        .unwrap_or(FLOOR_SECS)
        .clamp(FLOOR_SECS, CEIL_SECS);
    std::time::Duration::from_secs(timeout_secs)
}
```

2. The audio duration is known before the task spawns for Whisper/Parakeet
   (the 0.5s-minimum gate in the normalization block computes it). Thread it
   into the task. For Soniox/remote where normalization is skipped, pass
   `None` → 180s floor. This budget applies to every live transcription,
   but the enforcement differs by engine:
   - CPU Whisper / Parakeet: sibling watchdog sets the shared cancellation
     flag and the engine must keep running until its cancellation path
     returns cleanly.
   - Soniox: wrap the awaited `soniox_transcribe_async` future itself in
     `tokio::time::timeout(watchdog_budget, ...)` because it is async
     network IO and has no internal cancellation flag today.
   - Remote: keep the existing remote-client timeout; an outer live budget
     may be added defensively but must not double-report timeout errors.

3. Inside the spawned task, create:

```rust
let timed_out = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
let watchdog_timed_out = timed_out.clone();
let watchdog_app = app_for_task.clone();
let watchdog = tauri::async_runtime::spawn(async move {
    tokio::time::sleep(watchdog_budget).await;
    watchdog_timed_out.store(true, std::sync::atomic::Ordering::SeqCst);
    let state = watchdog_app.state::<AppState>();
    state.request_cancellation();
    log::warn!("Transcription watchdog fired after {:?}", watchdog_budget);

    #[cfg(target_os = "windows")]
    {
        let gpu_client = watchdog_app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
        gpu_client.abort_active_process().await;
    }
});
```

4. Run the existing engine dispatch normally — do not drop it on timeout.
   Step 2 makes CPU Whisper's C callback read the flag during decode; Step 4
   makes live Parakeet's request loop read the same flag and kill the child
   before returning.

5. Immediately after engine dispatch returns, abort the watchdog if it has
   not fired:

```rust
if !timed_out.load(std::sync::atomic::Ordering::SeqCst) {
    watchdog.abort();
}
```

6. If the local-engine result is `Err("Transcription cancelled")` and
   `timed_out` is true, map it to the existing local-failure path with
   message `"Transcription timed out"` instead of treating it as user
   cancellation. If `timed_out` is false, keep the current user-cancel
   behavior. If the Soniox branch's `tokio::time::timeout` elapses, map it
   to the same `"Transcription timed out"` user-facing path.

Do not clear the cancellation flag in this plan; plan 004's start-path rule
clears it before the next `Starting`.

**Verify**: `cargo check` → exit 0; new unit test for
`transcription_watchdog_budget` (None/floor/scale/+60s/ceiling cases) passes.

### Step 4: Parakeet transcribe — cancel flag + deadline

1. In `parakeet/sidecar.rs`, generalize the cancel error
   (`:108-111`): replace the hardcoded
   `"Download cancelled by user"` with `"Cancelled by user"` (code stays
   `"cancelled"`). Use the Search tool for `Download cancelled` under
   `src-tauri/` and update any matching tests/string assertions.
2. In `parakeet/manager.rs`, change `transcribe_with_custom_vocabulary`
   (`:375-398`) to accept a `cancel_flag: Option<Arc<AtomicBool>>` and route
   through `send_command_with_progress_and_cancel` (`:459+`) with a no-op
   progress callback, instead of `send_command`. Update `transcribe`
   (`:356-373`) and every call site found by searching for
   `transcribe_with_custom_vocabulary` and Parakeet `.transcribe(` call
   sites under `src-tauri/src/`:
   - live path in `commands/audio.rs`: pass a clone of
     `app_state.should_cancel_recording`;
   - upload path in `commands/audio.rs` and remote server path in
     `remote/transcription.rs`: pass `None` for now (their cancellation
     story belongs to the shared-contract work).
3. Deadline: the Step 3 sibling watchdog owns the live deadline by setting
   the same cancel flag the request loop polls. Do not wrap Parakeet
   transcribe in `tokio::time::timeout`, because dropping the future skips
   the cleanup path that removes/kills the sidecar after a `cancelled`
   response (`sidecar.rs:290-296`).

Cancellation semantics note: killing the sidecar child on cancel is correct
and matches the download path; the next Parakeet use re-spawns via `ensure`
(`sidecar.rs:201-211`) and re-loads the model — acceptable cost for an
explicit user cancel.

**Verify**: `cargo check` → exit 0; `cargo test parakeet` → pass.

### Step 5: Never lose speech

1. **Noise-token filter**: extract the condition at `audio.rs:3530-3534`
   into a pure helper `fn is_non_speech_transcript(raw: &str) -> bool` that
   trims, then matches the whole text (case-insensitive) against:
   empty, `[BLANK_AUDIO]`, `[SOUND]`, `[MUSIC]`, `[NOISE]`,
   `[INAUDIBLE]`, `(silence)`, `(music)`, `(noise)`. Whole-string match
   only — a real transcript containing `[MUSIC]` mid-sentence must NOT be
   filtered. Use it at the call site.
2. **No-deliver path** (`audio.rs:3621-3694`): instead of silently dropping
   the text on `process_transcription` Err:
   - copy the deterministic/raw text (`text_for_process`) to the clipboard
     using the existing clipboard-copy helper used by the
     `auto_paste=false` branch;
   - change the toast to make recovery obvious, e.g.
     `"<existing reason> — original text copied to clipboard"`;
   - save the raw transcript with the existing history mechanism: the
     success path already calls `save_transcription_with_recording` after
     insertion/copy (`audio.rs:3753-3775`), and the helper is public in the
     same module (`audio.rs:4028-4089`). Reuse that helper so the failed
     formatting transcript is recoverable from the app.
   - keep `should_deliver = false` semantics for *insertion* (do not paste
     possibly-wrong-language text into the target app) — the guarantee is
     "speech always lands somewhere recoverable", not "always paste".

**Verify**: `cargo check`; new unit tests for `is_non_speech_transcript`
(positive: `[SOUND]`, `(silence)`, case variants, surrounding whitespace;
negative: real sentence containing `[MUSIC]`, non-empty prose) pass.

### Step 6: Full gates

**Verify**: `cd src-tauri && cargo test` → all pass.
**Verify**: `cargo clippy -- -D warnings` → exit 0; `cargo fmt --check` → exit 0.
**Verify**: `pnpm typecheck && pnpm lint && pnpm test --run` → all pass
(frontend should be untouched; this catches accidental event/contract drift).

## Test plan

- Unit: `transcription_watchdog_budget` (None / floor / 4× + 60s scale / ceiling).
- Unit: `is_non_speech_transcript` (see Step 5).
- Existing characterization tests (plan 003) must stay green — they pin
  state-machine behavior this plan must not alter.
- Manual smoke (required, macOS):
  1. Sound ON, wired/builtin mic: hotkey → speak immediately → first word
     present in transcript.
  2. Sound ON, Bluetooth headset (if available): start recording → note
     whether chime clips; transcription must include first word.
  3. Esc-cancel mid-decode of a long (~60s) recording on CPU Whisper →
     pill returns to idle within ~1s (abort callback working).
  4. Parakeet: cancel during transcription → pill idle, next recording
     works (sidecar respawn + model reload).
  5. Force a formatting hard-failure (set an output language with an
     invalid AI key) → toast appears, transcript in clipboard, entry in
     history, state returns to idle.

## Done criteria

ALL must hold:

- [ ] No 300ms sleep in the start-sound block; sound still plays.
- [ ] `set_abort_callback_safe` wired; Esc during CPU decode aborts within
      one whisper internal iteration (smoke 3).
- [ ] Live local transcription is bounded by a sibling watchdog that sets
      cancellation without dropping the engine future; live Soniox is bounded
      by an async timeout; timeout paths reset UI to a recordable state
      (quote the hunks).
- [ ] Live Parakeet transcribe accepts and honors the cancel flag.
- [ ] `is_non_speech_transcript` used at the blank-check site; tests pass.
- [ ] Formatting hard-failure copies text to clipboard + saves history; no
      silent discard remains (quote the hunk).
- [ ] All commands in "Commands you will need" pass.
- [ ] Manual smoke 1-5 performed and reported (unperformable items downgrade
      this plan to NEEDS-SMOKE, not DONE).
- [ ] Only in-scope files modified (`git status`).
- [ ] `plans/README.md` status row updated.

## STOP conditions

Stop and report back if:

- whisper-rs 0.16 does not expose `set_abort_callback_safe` (or equivalent
  abort mechanism) — do not upgrade the crate inside this plan.
- The engine-dispatch expression in the spawned task no longer matches the
  shape described (e.g. the shared-contract executor landed first) — report
  the new seam; the watchdog belongs there instead.
- Wiring the abort callback forces call-site changes outside
  `commands/audio.rs` and `whisper/transcriber.rs`; the live call-site change
  inside `commands/audio.rs` is expected and in scope.
- The no-deliver branch turns out to also fire for plain AI-cleanup failures
  (i.e. `resolve_smart_formatting_outcome`'s fallback is dead code) — that
  contradicts this plan's analysis; re-verify before changing behavior.

## Maintenance notes

- Deliberately deferred follow-ups (same theme, separate plans): mic
  pre-warm/cached device config; in-process resample replacing the ffmpeg
  spawn; streaming/chunked decode while recording (needs the shared
  contract first); recorder stop busy-poll (flagged in plans/README.md).
- The watchdog duration mirrors `gpu_sidecar.rs` policy (`ceil(seconds)*4+60`,
  clamped to 180s..30min); if that policy changes, change both or extract a
  shared constant.
- Reviewer should scrutinize: double-abort interactions — user cancel and
  watchdog timeout racing (both call `request_cancellation`; both paths are
  idempotent by design, verify no double state-reset toast).
