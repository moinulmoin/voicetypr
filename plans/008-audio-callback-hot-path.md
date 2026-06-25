# Plan 008: Take allocations and lock contention out of the real-time audio callback

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 41351bd..HEAD -- src-tauri/src/audio/recorder.rs`
> On drift, compare "Current state" excerpts to live code first; mismatch = STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED-HIGH (real-time capture path; a mistake here breaks
  recording, the product's core function — manual smoke is mandatory)
- **Depends on**: none (003/004 recommended first so cancel/stop behavior is
  pinned, but not a hard dependency)
- **Category**: perf / bug
- **Planned at**: commit `41351bd`, 2026-06-10

## Why this matters

The CPAL audio callback — code that runs on the OS audio thread with a
deadline of a few milliseconds — currently:

1. **Heap-allocates one or two `Vec`s per callback** for sample-format
   conversion (F32→i16, I16→f32, U16→both).
2. **Writes samples one at a time into the WAV writer under a `try_lock`** —
   and when the lock is contended, **silently skips the whole chunk**,
   dropping recorded audio precisely when the system is busy (e.g. while a
   previous transcription is still running).
3. Takes a mutex on a byte counter every callback.

Dropped chunks = missing words in transcripts, with no error anywhere. This
plan removes the allocations via reusable scratch buffers and eliminates the
drop window by handing chunks to a dedicated writer via a channel instead of
locking the writer in the callback.

## Current state

`src-tauri/src/audio/recorder.rs` (636 lines). Inside the recording thread's
setup (one function, ~lines 140-380):

- Writer created at `:184-186`:
  `let writer = Arc::new(Mutex::new(Some(hound::WavWriter::create(&output_path, spec)...)));`
- Byte counter `:210`: `let bytes_written = Arc::new(Mutex::new(0u64));`
- Drain-barrier flags `:213-214`: `stop_requested: Arc<AtomicBool>`,
  `callback_drained: Arc<AtomicBool>`.
- `process_audio` closure (`:217-300`), the per-callback core:

```rust
                move |f32_samples: &[f32], i16_samples: &[i16]| {
                    // Drain barrier: if stop was requested, handle final write then exit
                    if stop_requested_clone.load(Ordering::SeqCst) { ... uses lock(), not
                        try_lock(), for the final drain write ... }
                    ...
                    if let Ok(mut bytes_guard) = bytes_clone.lock() {           // :276
                        ... RecordingSize::check(new_total) ... *bytes_guard = new_total;
                    }
                    // Write audio data (i16 format)
                    if let Ok(mut guard) = writer_clone.try_lock() {            // :286
                        if let Some(writer) = guard.as_mut() {
                            for &sample in i16_samples {
                                if let Err(e) = writer.write_sample(sample) { ... break; }
                            }
                        }
                    }   // NOTE: try_lock failure -> entire chunk silently dropped
                }
```

- Per-format conversion allocations in the CPAL `build_input_stream`
  callbacks:
  - F32 `:308-321`: `let i16_samples: Vec<i16> = data.iter().map(...).collect();`
  - I16 `:332-339`: `let f32_samples: Vec<f32> = data.iter().map(...).collect();`
  - U16 `:349-362`: allocates BOTH `f32_samples` and `i16_samples`.
- The closure also feeds `silence_detector` and `level_meter`
  (both `Arc<Mutex<...>>`, created `:165-174`) — their locking stays as-is
  in this plan (see Out of scope).
- Stop path `stop_recording` (`:474-512`): sends `RecorderCommand::Stop`,
  then polls `thread_handle.is_finished()` in 100 ms steps up to 5 s. The
  recording thread finalizes the WAV writer when it exits (read the thread
  body around the stream teardown to locate finalize — search
  `finalize` in the file).

## Target design

Two independent, individually-verifiable changes:

**A. Zero steady-state allocations in the conversion callbacks.**
Each CPAL callback closure owns reusable scratch buffers
(`Vec::with_capacity`), captured mutably by the `move` closure:

```rust
                            let mut i16_scratch: Vec<i16> = Vec::with_capacity(4096);
                            move |data: &[f32], _: &_| {
                                i16_scratch.clear();
                                i16_scratch.extend(data.iter().map(|&s| {
                                    let clamped = s.clamp(-1.0, 1.0);
                                    (clamped * 32767.0) as i16
                                }));
                                process_clone(data, &i16_scratch);
                            }
```

Same pattern for I16 (f32 scratch) and U16 (both scratches). Growth beyond
capacity can still allocate on the first oversized callback — acceptable; it
amortizes to zero.

**B. Replace the writer mutex + byte mutex with a channel to a writer
thread.**

- `bytes_written: Arc<Mutex<u64>>` → `Arc<AtomicU64>` (`fetch_add` in the
  callback, `load` wherever it is read — find all uses with
  `grep -n "bytes_written\|bytes_clone" src-tauri/src/audio/recorder.rs`).
- Writer ownership moves to a dedicated thread:
  - Channel: `std::sync::mpsc::SyncSender<WriterMsg>` with a generous bound
    (e.g. 64) where `enum WriterMsg { Chunk(Vec<i16>), Finalize }`.
  - To avoid re-introducing per-callback allocation for the chunk `Vec`, add
    a recycle channel: writer thread sends drained `Vec<i16>`s back on a
    `mpsc::Sender<Vec<i16>>`; the callback pops a recycled buffer
    (`try_recv().unwrap_or_else(|_| Vec::with_capacity(4096))`), fills it
    from the (scratch-converted) i16 slice, and sends it.
  - Callback send uses `try_send`; on `Full`, increment an
    `Arc<AtomicU64>` `dropped_chunks` counter (visible failure instead of
    silent drop — log it once from the writer thread or on stop). A 64-deep
    queue at ~10 ms/chunk gives the writer ~640 ms of slack; full queue means
    disk stall, which the old code would have dropped silently anyway.
  - The writer thread loop: receive, write samples, recycle the Vec; on
    `Finalize` (or channel disconnect), finalize the WAV writer, then exit,
    reporting `Result` through the existing recording-thread join value.
- The drain barrier (`stop_requested` / `callback_drained`) currently exists
  so the final chunk is written with a blocking `lock()`; with the channel
  design the callback's final chunk is just another `send` — KEEP the flags
  and their semantics (the stop path still must know callbacks have ceased
  before `Finalize` is sent). Read the full thread body before rewiring; the
  ordering must remain: stop requested → last callback drained → Finalize →
  writer thread finalizes → recording thread joins writer thread → returns
  path.
- `RecordingSize::check` (the size cap) moves to the writer thread (it has
  the authoritative byte count after each write; on breach it triggers the
  same stop signal the callback used to send — find the existing breach
  handling at `:276-283` and replicate its effect).

## Commands you will need

| Purpose            | Command                                            | Expected on success |
|--------------------|----------------------------------------------------|---------------------|
| Compile            | `cd src-tauri && cargo check`                      | exit 0              |
| Audio tests        | `cd src-tauri && cargo test audio`                 | all pass            |
| Full backend tests | `cd src-tauri && cargo test`                       | all pass            |
| Rust format        | `cd src-tauri && cargo fmt --check`                | exit 0              |
| Clippy (CI parity) | `cd src-tauri && cargo clippy -- -D warnings`      | exit 0              |

## Scope

**In scope**:
- `src-tauri/src/audio/recorder.rs` — conversion closures, `process_audio`,
  writer/byte-counter plumbing, writer thread, plus new unit tests for the
  pure conversion helpers (extract `f32_to_i16`/`u16_to_i16`/`u16_to_f32`
  mapping fns so they're testable).

**Out of scope**:
- `silence_detector` / `level_meter` locking — separate concern; leave their
  call sites byte-identical.
- `stop_recording`'s 5 s poll-join (that is plan-009-adjacent debt, not this
  plan; do not "fix" it here).
- `cpal` stream configuration, device selection, sample-rate handling.
- Any change to the WAV format/spec.

## Git workflow

- Branch: `advisor/008-audio-callback-hot-path`
- Commit message: `perf: remove allocations and lock contention from audio callback`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Read the whole recording thread body

Read `src-tauri/src/audio/recorder.rs` lines 100-470 end-to-end. Map: where
the writer is finalized today, where the thread's `Result<String, String>`
(the output path) is produced, and every consumer of `bytes_written`,
`writer`, `stop_requested`, `callback_drained`. List them in your report
BEFORE editing.

**Verify**: report contains the map with line numbers.

### Step 2: Change A — scratch buffers in conversion closures

Apply the scratch-buffer pattern to F32/I16/U16 closures. Extract the three
sample-mapping expressions into named pure fns and add unit tests for them
(clamping at ±1.0, U16 midpoint 32768 → 0, symmetry).

**Verify**: `cargo check` exit 0; `cargo test audio` → new conversion tests
pass; existing tests pass.

### Step 3: Change B — atomic byte counter

Swap `bytes_written` to `Arc<AtomicU64>` everywhere (callback add, size
check, any reader).

**Verify**: `cargo check` exit 0; `cargo test` passes.

### Step 4: Change B — writer thread + channel

Implement `WriterMsg`, the bounded channel, recycle channel, writer thread,
`dropped_chunks` counter, and the `Finalize` handshake per the target design.
Remove the `writer` mutex from `process_audio` entirely. Preserve the
drain-barrier ordering mapped in Step 1.

**Verify**: `cargo check` exit 0; `cargo test` passes (existing
`audio_recording_tests.rs` and `audio_commands.rs` suites must stay green).

### Step 5: Format, clippy, smoke

**Verify**: `cargo fmt --check` exit 0; `cargo clippy -- -D warnings` exit 0.

Manual smoke (MANDATORY — this is the core product path):
1. Normal dictation: record 10 s, transcribe → text appears; WAV plays back
   cleanly if `save_recordings` is on.
2. Long recording: 3+ min → no memory growth beyond queue bound, transcript
   complete.
3. Stop variants: hotkey stop, Escape cancel, silence auto-stop → all return
   to Idle; no leftover temp files.
4. Device yank: unplug/switch input mid-recording → graceful stop (existing
   `err_fn` path at `:191-207` must still trigger).

## Test plan

- Unit tests: 3 conversion fns (Step 2); optionally a writer-thread test
  feeding synthetic chunks through the channel into a temp-file WavWriter
  and asserting sample count + finalize success (recommended; use
  `tempfile`, already a dependency).
- Existing suites: `cargo test audio` (recording tests), full `cargo test`.
- Manual smoke per Step 5.

## Done criteria

ALL must hold:

- [ ] `grep -n "try_lock" src-tauri/src/audio/recorder.rs` → no hit inside
      `process_audio`'s steady-state write path
- [ ] `grep -n "collect()" src-tauri/src/audio/recorder.rs` → no per-callback
      conversion `collect()`s remain in the three CPAL closures
- [ ] `bytes_written` is `AtomicU64` (no `Mutex<u64>`)
- [ ] Writer thread owns the `WavWriter`; `dropped_chunks` counter exists and
      is logged on stop when non-zero
- [ ] `cargo test`, `cargo fmt --check`, `cargo clippy -- -D warnings` exit 0
- [ ] Manual smoke 1-4 performed and reported (else mark NEEDS-SMOKE, not DONE)
- [ ] Only `src-tauri/src/audio/recorder.rs` modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back if:

- The recording thread body diverges materially from the Step 1 map mid-edit
  (you discover a second writer consumer, a seek-based WAV patcher, or
  another thread touching `writer`).
- Preserving the drain-barrier semantics with the channel requires changing
  `stop_recording`'s public behavior or timing contract.
- Any existing audio test fails for a reason you cannot trace to an
  assertion on the old locking internals.
- Smoke test 4 (device yank) misbehaves — abort, this path is safety-critical.

## Maintenance notes

- `silence_detector`/`level_meter` per-callback mutexes are the next hot-path
  cleanup (same recipe: atomics or channel); deliberately deferred.
- If `dropped_chunks` is ever observed non-zero in the field, raise the queue
  bound before doing anything cleverer.
- Reviewer should scrutinize: finalize-on-disconnect (writer thread must
  finalize even if the recording thread panics before sending `Finalize`,
  i.e. treat `RecvError` as Finalize).
