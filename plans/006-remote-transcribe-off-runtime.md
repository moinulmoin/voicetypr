# Plan 006: Move remote transcription off the async runtime worker

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 41351bd..HEAD -- src-tauri/src/remote/http.rs src-tauri/src/remote/transcription.rs`
> On drift, compare "Current state" excerpts to live code first; mismatch = STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (touches the LAN transcription handler; 1,300 lines of
  existing tests in `http.rs` plus `src-tauri/src/tests/remote_http_tests.rs`
  are the safety net)
- **Depends on**: none
- **Category**: bug / perf
- **Planned at**: commit `41351bd`, 2026-06-10

## Why this matters

The network-sharing host handles `POST /api/v1/transcribe` by calling
`ctx.transcribe_with_context(...)` — a **synchronous, CPU-bound** function
(it runs a full Whisper/Parakeet transcription) — directly on a tokio worker
thread of the warp server. A multi-minute transcription therefore pins a
runtime worker. On small thread counts this delays every other route
(`/status` polls from clients, model-control) and any other async work on
that runtime. The fix is mechanical: run the blocking call in
`tokio::task::spawn_blocking`, taking the context read-lock inside the
blocking thread via `blocking_read()`.

## Current state

`src-tauri/src/remote/http.rs:462-477` (inside
`async fn handle_transcribe<T: ServerContext + 'static>`):

```rust
    // Serialize transcription work on the sharing host.
    let _permit = transcription_guard
        .acquire()
        .await
        .expect("transcription semaphore closed");

    let request_context = decode_context_header(request_context);

    // Perform transcription
    let ctx = ctx.read().await;
    match ctx.transcribe_with_context(
        &body,
        spoken_language.as_deref(),
        transcription_task.as_deref(),
        request_context.as_deref(),
    ) {
        Ok(result) => { ... build TranscribeResponse ... }
        Err(error) => { ... 500 with ErrorResponse ... }
    }
```

Handler signature facts (`:397-406`): `body: bytes::Bytes` (cheap clone),
`spoken_language: Option<String>`, `transcription_task: Option<String>`,
`request_context: Option<String>` after decode, and
`ctx: Arc<RwLock<T>>` where `RwLock` is `tokio::sync::RwLock` and
`T: ServerContext + 'static` (trait defined at `:28-64`, `Send + Sync`).

The trait method is sync (`src-tauri/src/remote/transcription.rs:232-240`):

```rust
    fn transcribe_with_context(
        &self, audio_data: &[u8], spoken_language: Option<&str>,
        transcription_task: Option<&str>, context: Option<&str>,
    ) -> Result<TranscriptionResult, String> {
        self.transcribe_inner(audio_data, spoken_language, transcription_task, context)
    }
```

Precedent for blocking-bridge patterns in this exact file:
`transcription.rs:242-246` (`get_model_control_snapshot`) uses
`tokio::task::block_in_place`. We deliberately use `spawn_blocking` here
instead, because `block_in_place` panics on current-thread runtimes (which
`#[tokio::test]` uses by default) and `handle_transcribe` is heavily covered
by in-file tests (`http.rs:510-1807`).

## Target shape

Replace the lock-and-call block with:

```rust
    let request_context = decode_context_header(request_context);

    // Perform transcription on a blocking thread: transcribe_with_context is
    // synchronous CPU work (full Whisper/Parakeet run) and must not pin a
    // runtime worker.
    let ctx_for_blocking = ctx.clone();
    let body_for_blocking = body.clone();
    let transcription_outcome = tokio::task::spawn_blocking(move || {
        let guard = ctx_for_blocking.blocking_read();
        guard.transcribe_with_context(
            &body_for_blocking,
            spoken_language.as_deref(),
            transcription_task.as_deref(),
            request_context.as_deref(),
        )
    })
    .await
    .unwrap_or_else(|join_err| Err(format!("transcription task failed: {join_err}")));

    match transcription_outcome {
        Ok(result) => { ... unchanged response building ... }
        Err(error) => { ... unchanged 500 arm ... }
    }
```

Notes:
- `spoken_language`/`transcription_task`/`request_context` are owned
  `Option<String>`s — move them into the closure (the later log lines use
  `server_name`, which stays outside; if any later code still needs the moved
  values, clone before the closure).
- `tokio::sync::RwLock::blocking_read()` is correct (not a deadlock) inside
  `spawn_blocking` because that thread is not an async context.
- `body` is `bytes::Bytes`; `.clone()` is a refcount bump.
- The semaphore permit (`_permit`) must remain held across the await of the
  blocking task — keep its binding above this block, unchanged.

## Commands you will need

| Purpose            | Command                                            | Expected on success |
|--------------------|----------------------------------------------------|---------------------|
| Compile            | `cd src-tauri && cargo check`                      | exit 0              |
| HTTP module tests  | `cd src-tauri && cargo test remote`                | all pass            |
| Full backend tests | `cd src-tauri && cargo test`                       | all pass            |
| Rust format        | `cd src-tauri && cargo fmt --check`                | exit 0              |
| Clippy (CI parity) | `cd src-tauri && cargo clippy -- -D warnings`      | exit 0              |

## Scope

**In scope**:
- `src-tauri/src/remote/http.rs` — `handle_transcribe` only, plus one new
  test in its `mod tests`.

**Out of scope**:
- `ServerContext` trait shape and `transcription.rs` implementations
  (including their `block_in_place` bridges) — unchanged.
- The read-lock-held-during-transcription semantics: after this plan the
  lock is STILL held for the duration (now on a blocking thread). Releasing
  it requires a snapshot redesign that belongs to the shared-contract work
  (plan 012). Do not attempt it here.
- Auth, size limits, status endpoint, model-control endpoints.

## Git workflow

- Branch: `advisor/006-remote-transcribe-blocking`
- Commit message: `fix: run remote transcription on blocking thread pool`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Apply the target shape

Edit `handle_transcribe` exactly as specified above. Keep the
`info!("[Remote Server] Starting transcription ...")` log (`:457-460`) before
the block, and the COMPLETED/FAILED logs in the match arms unchanged.

**Verify**: `cd src-tauri && cargo check` → exit 0.

### Step 2: Add a non-starvation regression test

In `http.rs`'s existing `mod tests` (`:510+`), find the mock `ServerContext`
implementation used by transcribe tests (search `impl ServerContext` within
the tests module). Add a test, modeled structurally on the existing
transcribe-endpoint tests:

- `transcribe_runs_off_runtime_worker`: configure the mock context's
  `transcribe_with_context` to sleep (std::thread::sleep) ~300 ms and return
  Ok. Use a `#[tokio::test(flavor = "multi_thread", worker_threads = 1)]`
  runtime. Fire the transcribe request via `warp::test::request()` in a
  spawned task; while it is in flight, `tokio::time::timeout(150ms, /* a
  trivial async task, e.g. requesting GET /api/v1/status via warp::test */)`
  must complete — proving the single runtime worker is not pinned by the
  transcription. Assert both: status responded within the timeout, and the
  transcribe call ultimately returned 200.
- If the mock context cannot sleep without modification, extending the mock
  with a configurable delay field is in scope (tests module only).

**Verify**: `cd src-tauri && cargo test remote` → all pass including the new
test. To prove the test bites: temporarily revert Step 1 (stash), run the new
test, confirm it FAILS (status times out), restore. Report this red/green
evidence.

### Step 3: Full suite, format, clippy

**Verify**: `cargo test` all pass; `cargo fmt --check` exit 0;
`cargo clippy -- -D warnings` exit 0.

## Test plan

- New test from Step 2 (red/green verified).
- Existing transcribe tests in `http.rs` and
  `src-tauri/src/tests/remote_http_tests.rs` / `remote_transcription_tests.rs`
  must stay green — they pin auth, limits, response shapes.

## Done criteria

ALL must hold:

- [ ] `handle_transcribe` contains `spawn_blocking` and `blocking_read`, and
      no longer calls `transcribe_with_context` under `ctx.read().await`
- [ ] New test exists, passes, and was demonstrated to fail against the old
      code (red/green note in report)
- [ ] `cargo test`, `cargo fmt --check`, `cargo clippy -- -D warnings` all
      exit 0
- [ ] Only `src-tauri/src/remote/http.rs` modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back if:

- `handle_transcribe` no longer matches the excerpt (drift).
- The tests' mock `ServerContext` is structured so that adding a delay
  requires changing the production trait — report; do not change the trait.
- The new test is flaky across 5 consecutive runs
  (`cargo test transcribe_runs_off_runtime_worker -- --test-threads=1`
  five times) — timing margins may need widening (sleep 500 ms / timeout
  250 ms); if still flaky, report.

## Maintenance notes

- KNOWN LIMITATION (intentional): the ctx read-lock is still held during
  transcription, so model-control PATCH (`ctx.write()`) waits for in-flight
  jobs. The fix is a snapshot/handle redesign tracked under plan 012's
  contract design — when that lands, revisit this handler.
- If anyone adds *parallel* remote transcription (semaphore > 1), the
  blocking pool usage here is already compatible; the lock becomes the next
  bottleneck (see above).
