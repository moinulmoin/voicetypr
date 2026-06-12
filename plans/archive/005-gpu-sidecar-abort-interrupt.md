# Plan 005: Let GPU-sidecar abort interrupt an in-flight transcription request

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 41351bd..HEAD -- src-tauri/src/whisper/gpu_sidecar.rs src-tauri/src/commands/audio.rs`
> On drift, compare "Current state" excerpts to live code first; mismatch = STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (sidecar protocol is single-flight over stdin/stdout; the
  abort path must not corrupt request/response pairing)
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `41351bd`, 2026-06-10

## Why this matters

On Windows, cancelling during `Transcribing` calls
`GpuSidecarClient::abort_active_process()` (from
`commands/audio.rs:4910-4914`). But `abort_active_process` must acquire the
same async mutex that `send()` holds **across the entire sidecar request**
(`process.request(request).await`). A Vulkan transcription of a long
recording can take minutes (timeout up to 30 min); during all of it, abort is
lock-starved and Escape appears dead. The abort path must be able to kill the
sidecar without waiting behind the request it is trying to kill.

## Current state

File: `src-tauri/src/whisper/gpu_sidecar.rs` (783 lines; whole file compiled
on all platforms via the `#![cfg_attr(not(windows), allow(dead_code, ...))]`
header — so this change builds and unit-tests on macOS too).

Client state (`:255-274`):

```rust
pub struct GpuSidecarClient {
    next_id: AtomicU64,
    process: AsyncMutex<Option<GpuSidecarProcess>>,
    status: AsyncRwLock<AccelerationRuntimeStatus>,
}
```

The starved abort (`:280-287`):

```rust
    pub async fn abort_active_process(&self) {
        let mut guard = self.process.lock().await;          // waits for send()
        if let Some(process) = guard.as_mut() {
            log::info!("Aborting active Whisper Vulkan sidecar process");
            process.kill_and_wait().await;
        }
        guard.take();
    }
```

The request path (`:459-501`, abbreviated):

```rust
    async fn send(&self, app: &AppHandle, request: &SidecarRequest<'_>)
        -> Result<SidecarResponse, String>
    {
        let mut guard = self.process.lock().await;
        if guard.is_none() {
            guard.replace(GpuSidecarProcess::spawn(app).await?);
        }
        let result = match guard.as_mut() {
            Some(process) => process.request(request).await,   // held for minutes
            None => Err("Vulkan sidecar was not started".to_string()),
        };
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                if let Some(process) = guard.as_mut() {
                    process.kill_and_wait().await;             // existing cleanup
                }
                guard.take();
                return Err(error);
            }
        };
        ...
    }
```

`GpuSidecarProcess::request` (`:206-231`) writes one JSON line to stdin, then
awaits one stdout line under `tokio::time::timeout(request.response_timeout(), ...)`.
`kill_and_wait` (`:233-244`) calls `child.start_kill()` then waits ≤5 s.
If the child dies mid-request, `next_line()` yields `Ok(None)` → request
returns Err("Vulkan sidecar exited before responding") → `send`'s error arm
cleans up the slot. That existing error path is what makes a select-based
abort safe.

Caller of abort: `src-tauri/src/commands/audio.rs:4910-4914`:

```rust
    #[cfg(target_os = "windows")]
    if matches!(current_state, RecordingState::Transcribing) {
        let gpu_client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
        gpu_client.abort_active_process().await;
    }
```

Existing tests: `gpu_sidecar.rs` `mod tests` (`:661-782`) — pure helpers
(timeouts, search dirs). No process-spawning tests; keep it that way.

## Target design

Add an abort signal that does not require the `process` mutex:

```rust
pub struct GpuSidecarClient {
    next_id: AtomicU64,
    process: AsyncMutex<Option<GpuSidecarProcess>>,
    status: AsyncRwLock<AccelerationRuntimeStatus>,
    abort_requested: std::sync::atomic::AtomicBool,   // NEW
    abort_notify: tokio::sync::Notify,                // NEW
}
```

- `send()` clears `abort_requested` after acquiring the lock (a new request
  begins; stale aborts must not kill it), then races the request against the
  notify:

```rust
        let result = match guard.as_mut() {
            Some(process) => {
                tokio::select! {
                    biased;
                    _ = self.abort_notify.notified(), if !aborted_already => {
                        Err("Vulkan sidecar request aborted".to_string())
                    }
                    res = process.request(request) => res,
                }
            }
            None => Err("Vulkan sidecar was not started".to_string()),
        };
```

  Concretely: capture `let aborted_already = self.abort_requested.load(SeqCst);`
  is NOT sufficient alone (request may start before abort) — the pattern is:
  check the flag immediately before the select (if already set, skip straight
  to Err), and otherwise select on `notified()`. On the abort arm, fall
  through to the EXISTING error-cleanup arm (`kill_and_wait` + `guard.take()`)
  so the child is killed and the slot cleared by the same code path that
  handles protocol errors today.

- `abort_active_process()` becomes lock-free first, best-effort second:

```rust
    pub async fn abort_active_process(&self) {
        self.abort_requested.store(true, Ordering::SeqCst);
        self.abort_notify.notify_waiters();
        // Best-effort: if no request is in flight, tear down an idle process.
        if let Ok(mut guard) = tokio::time::timeout(
            Duration::from_millis(250), self.process.lock()).await
        {
            if let Some(process) = guard.as_mut() {
                process.kill_and_wait().await;
            }
            guard.take();
            self.abort_requested.store(false, Ordering::SeqCst);
        }
        // If the lock is busy, the in-flight send() observes the notify,
        // kills the child via its error path, and clears the slot itself.
    }
```

- In `send()`, when the select returns the abort Err, also reset
  `abort_requested` to false after cleanup (consume the abort).

Why this is safe: stdin/stdout pairing cannot be corrupted because the abort
arm never leaves a live child with an unread response — the cleanup arm kills
the child and drops the process slot; the next request spawns a fresh sidecar.

## Commands you will need

| Purpose            | Command                                          | Expected on success |
|--------------------|--------------------------------------------------|---------------------|
| Compile            | `cd src-tauri && cargo check`                    | exit 0              |
| Module tests       | `cd src-tauri && cargo test gpu_sidecar`         | all pass            |
| Full backend tests | `cd src-tauri && cargo test`                     | all pass            |
| Rust format        | `cd src-tauri && cargo fmt --check`              | exit 0              |
| Clippy (CI parity) | `cd src-tauri && cargo clippy -- -D warnings`    | exit 0              |

## Scope

**In scope**:
- `src-tauri/src/whisper/gpu_sidecar.rs` (struct fields, `Default`/`new`,
  `send`, `abort_active_process`, new unit tests in the existing `mod tests`).

**Out of scope**:
- `commands/audio.rs` caller — its call shape is unchanged.
- `GpuSidecarProcess::request` / `spawn` / `kill_and_wait` internals.
- The warm/preload path (`warm`/`should_attempt_vulkan_warm_on_preload`) —
  unchanged; it routes through `send()` and inherits abort support for free.
- Windows-specific E2E (no Windows runner assumed here).

## Git workflow

- Branch: `advisor/005-sidecar-abort`
- Commit message: `fix: allow gpu sidecar abort to interrupt in-flight request`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Add the abort fields

Add `abort_requested: AtomicBool` and `abort_notify: tokio::sync::Notify` to
`GpuSidecarClient`; initialize in `new()` (`:268-274`) — `Notify::new()`,
`AtomicBool::new(false)`. `Default` delegates to `new()` already (`:261-265`).

**Verify**: `cd src-tauri && cargo check` → exit 0.

### Step 2: Rewrite `abort_active_process`

Per the target design above. Keep the log line
`"Aborting active Whisper Vulkan sidecar process"` on the path that actually
kills (both arms are fine).

**Verify**: `cargo check` → exit 0.

### Step 3: Race the request against the notify in `send`

Per the target design. Sequence inside `send` after the process is ensured:

1. `self.abort_requested.store(false, Ordering::SeqCst);` happens at the TOP
   of `send` (before locking) is WRONG — it would erase an abort aimed at the
   previous, still-running request. Correct placement: immediately after
   acquiring the lock and BEFORE spawning/using the process, because holding
   the lock proves no other request is in flight.
2. Build the select with `biased;` and the abort arm first, guarded by a
   fresh flag check so an abort that fired between lock-acquisition and
   select-entry is not missed:
   check `if self.abort_requested.load(SeqCst) { Err("...aborted".into()) }`
   before the select, then select on `notified()` vs `request()`.
3. Abort-arm Err must flow into the existing `Err(error)` cleanup arm
   (`kill_and_wait` + `guard.take()`), then reset `abort_requested` to false.

**Verify**: `cargo check` → exit 0; `cargo clippy -- -D warnings` → exit 0.

### Step 4: Unit tests

In the existing `mod tests` (no process spawning — test the signaling logic
by factoring it): extract a small pure helper if needed, or test via the
public surface with a mock-free approach:

- `abort_sets_flag_and_is_consumed`: call `abort_active_process()` on a fresh
  client (no process; lock uncontended) → completes quickly (< 1 s),
  afterwards `abort_requested` is false (idle-teardown arm consumed it).
  (Make the field `pub(crate)` or add a `#[cfg(test)] fn abort_pending()`
  accessor — either is acceptable.)
- `notify_wakes_waiter`: tokio test — spawn a task awaiting
  `client.abort_notify.notified()` via a `#[cfg(test)]` accessor, call
  `abort_active_process()`, assert the task completes within a timeout.

Model the async tests on existing `#[tokio::test]` usage in the repo
(`grep -rn "#\[tokio::test\]" src-tauri/src/ | head` for exemplars).

**Verify**: `cd src-tauri && cargo test gpu_sidecar` → all pass.

### Step 5: Full suite, format, clippy

**Verify**: `cargo test` all pass; `cargo fmt --check` exit 0;
`cargo clippy -- -D warnings` exit 0.

## Test plan

- Unit tests from Step 4 (2 new tests minimum).
- Manual smoke on Windows (REQUIRED before release, optional for this plan if
  no Windows machine): start a long recording with Vulkan acceleration,
  press Escape during "Transcribing…" → pill returns to idle within ~1 s and
  log shows the abort line; next recording works (fresh sidecar spawn).
  If not performable, mark the plan DONE-PENDING-WINDOWS-SMOKE in the index.

## Done criteria

ALL must hold:

- [ ] `abort_active_process` no longer unconditionally awaits
      `self.process.lock()` (quote the new body in the report)
- [ ] `send` races the sidecar request against the abort notify and routes
      abort through the existing kill/cleanup arm
- [ ] ≥2 new unit tests in `gpu_sidecar.rs` pass
- [ ] `cargo test`, `cargo fmt --check`, `cargo clippy -- -D warnings` all
      exit 0
- [ ] Only `src-tauri/src/whisper/gpu_sidecar.rs` modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back if:

- `GpuSidecarClient`/`send`/`abort_active_process` no longer match the
  excerpts (drift).
- The select-based abort cannot reuse the existing error-cleanup arm without
  restructuring `send` beyond ~40 changed lines — report the obstacle instead
  of redesigning the protocol.
- Clippy flags the `biased` select or `Notify` usage in a way that requires
  an architectural workaround.

## Maintenance notes

- If the sidecar ever becomes multi-request (pipelined), `Notify` +
  single-flag must be replaced with per-request cancellation tokens.
- Reviewer should scrutinize: the flag reset points (exactly two: post-lock
  in `send`, post-cleanup on the abort paths) — a missed reset turns the next
  transcription into an instant abort.
- The 250 ms best-effort lock timeout in `abort_active_process` is a
  tunable; it only affects teardown of an *idle* sidecar.
