# Plan 004: Make cancellation during `Starting` stick (stop the start path from erasing it)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 41351bd..HEAD -- src-tauri/src/commands/audio.rs src-tauri/src/state/app_state.rs`
> On drift, compare "Current state" excerpts to live code first; mismatch = STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (recording start/cancel timing is the app's most
  concurrency-sensitive path; PTT behavior must not regress)
- **Depends on**: plans/003-recording-state-characterization-tests.md
- **Category**: bug
- **Planned at**: commit `41351bd`, 2026-06-10

## Why this matters

Pressing Escape (or invoking `cancel_recording`) while the recorder is still
in `Starting` (slow microphone/device init) is silently lost: the in-flight
`start_recording` continuation unconditionally clears the cancellation flag
and then commits to `Recording`. The UI flashes Idle, then recording
resurrects. Cancel is the user's safety control — it must win the race.

## Current state

All in `src-tauri/src/commands/audio.rs`.

1. `start_recording` transitions to `Starting` at line 2085 after validation:

```rust
    update_recording_state(&app, RecordingState::Starting, None);   // :2085
```

2. After device init / audio capture start (a `MutexGuard` scope ends at
   `:2428`), the continuation runs (`:2430-2506`, abbreviated):

```rust
    // Now perform async operations after mutex is released

    // Clear cancellation flag for new recording
    app_state.clear_cancellation();                                  // :2433

    // Second PTT guard: ... if PTT key was released ... stop immediately
    if ptt_key_released(&app_state) {                                // :2438
        ... stops recorder, clear_pending_stop_after_start, resume media,
        ... cleanup_recording_path(), update_recording_state(Idle),
        return Err(PTT_START_ABORTED_AFTER_RELEASE.to_string());     // :2484
    }

    // Update state to recording
    update_recording_state(&app, RecordingState::Recording, None);   // :2488

    // If a stop was requested while starting ... honor it immediately
    if app_state
        .pending_stop_after_start
        .swap(false, std::sync::atomic::Ordering::SeqCst)            // :2494-2496
    { ... spawn stop_recording ... }
```

3. `cancel_recording` (`:4889-5011`): calls `app_state.request_cancellation()`
   first (`:4895`), stops the recorder **only if `is_recording()`** (`:4918-4937`
   — false during device init), then maps `Starting → Idle` (`:4990-4992`).
   It never sets `pending_stop_after_start`.

4. Flag API in `src-tauri/src/state/app_state.rs:120-131`:
   `request_cancellation()` / `clear_cancellation()` /
   `is_cancellation_requested()` over `should_cancel_recording: Arc<AtomicBool>`.

The race: cancel during device init → flag set, state Idle → continuation
clears the flag (`:2433`) → state forced to `Recording` → recording runs with
no user intent and ESC handling re-registered.

The fix principle: **the start path may only clear a cancellation that
predates this start attempt — it must do so before entering `Starting`, and
must check (not clear) at the commit point.**

Note the existing PTT-release guard (`:2438-2485`) already implements the
exact abort sequence we need at the commit point: stop recorder synchronously,
`clear_pending_stop_after_start`, `MEDIA_CONTROLLER.resume_if_we_paused()`,
remove the partial recording file, `update_recording_state(Idle)`, return Err.

## Commands you will need

| Purpose            | Command                                                       | Expected on success |
|--------------------|---------------------------------------------------------------|---------------------|
| Compile            | `cd src-tauri && cargo check`                                 | exit 0              |
| Characterization   | `cd src-tauri && cargo test recording_state_characterization` | all pass (after Step 4 update) |
| Full backend tests | `cd src-tauri && cargo test`                                  | all pass            |
| Rust format        | `cd src-tauri && cargo fmt --check`                           | exit 0              |

## Scope

**In scope**:
- `src-tauri/src/commands/audio.rs` — `start_recording` only (the two
  locations below).
- `src-tauri/src/tests/recording_state_characterization.rs` — update the
  `KNOWN_BUG_PLAN_004` test (created by plan 003).

**Out of scope**:
- `cancel_recording` — its behavior is correct given the fix; do not modify.
- `app_state.rs` flag API — no new methods needed.
- PTT key handling (`ptt_key_released`, `pending_stop_after_start`
  semantics) — must remain byte-for-byte except where this plan says.
- The ESC registration/unregistration logic.

## Git workflow

- Branch: `advisor/004-cancel-during-starting`
- Commit message: `fix: honor cancellation requested during recording start`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Move the flag clear to before `Starting`

In `start_recording`, immediately **before**
`update_recording_state(&app, RecordingState::Starting, None);` (line 2085),
add:

```rust
    // Clear any stale cancellation from a previous attempt. From this point
    // on, a cancellation request targets THIS start attempt and must win.
    {
        let app_state = app.state::<AppState>();
        app_state.clear_cancellation();
    }
```

(There is an existing `let app_state = app.state::<AppState>();` block at
`:2071-2077` for the PTT guard; placing the clear inside that existing block
is equally acceptable.)

**Verify**: `cd src-tauri && cargo check` → exit 0.

### Step 2: Replace the commit-point clear with a check-and-abort

At line 2432-2433, replace:

```rust
    // Clear cancellation flag for new recording
    app_state.clear_cancellation();
```

with a guard mirroring the PTT-release guard right below it:

```rust
    // If cancellation was requested while we were starting (e.g. Escape
    // during slow device init), abort instead of committing to Recording.
    if app_state.is_cancellation_requested() {
        log::info!("Cancellation requested during start; aborting before Recording state");
        let recorder_state_handle = app.state::<RecorderState>();
        let stop_result = recorder_state_handle
            .inner()
            .0
            .lock()
            .map_err(|e| format!("Failed to acquire recorder lock: {}", e))
            .and_then(|mut recorder| {
                if recorder.is_recording() {
                    recorder.stop_recording()
                } else {
                    Ok(String::new())
                }
            });

        clear_pending_stop_after_start(&app_state);
        MEDIA_CONTROLLER.resume_if_we_paused();

        // same cleanup closure pattern as the PTT guard below
        if let Ok(mut path_guard) = app_state.current_recording_path.lock() {
            if let Some(path) = path_guard.take() {
                if let Err(error) = std::fs::remove_file(&path) {
                    log::warn!(
                        "Failed to remove cancelled recording file {}: {}",
                        path.display(),
                        error
                    );
                }
            }
        }

        update_recording_state(&app, RecordingState::Idle, None);
        stop_result?;
        return Err("Recording start cancelled".to_string());
    }
```

Implementation notes:
- Reuse the existing `cleanup_recording_path` closure if you can hoist it
  above both guards without changing the PTT guard's behavior; otherwise
  duplicate the small block as shown (the PTT guard at `:2461-2473` defines
  it locally — hoisting it so both guards share one definition is the cleaner
  option and is in scope).
- `cancel_recording` already set the state to Idle; calling
  `update_recording_state(Idle)` again is idempotent
  (`UnifiedRecordingState::transition_with_fallback` tolerates it — verified
  by plan 003's tests).
- Keep the PTT-release guard immediately after, unchanged.

**Verify**: `cd src-tauri && cargo check` → exit 0.

### Step 3: Audit remaining `clear_cancellation` call sites

Run `grep -n "clear_cancellation" src-tauri/src/ -r`. Expected after this
change: the new pre-`Starting` site (Step 1), and any sites in
`stop_recording`/transcription paths. For each remaining site, confirm it
clears only at the **start of a new user-initiated operation**, never between
`request_cancellation` and the point that honors it. List each site + verdict
in your report. Do not change them unless one exhibits the same
clear-after-request race **inside `start_recording`**.

**Verify**: report lists every site with a one-line verdict.

### Step 4: Update the plan-003 characterization test

In `src-tauri/src/tests/recording_state_characterization.rs`, rename
`clear_cancellation_erases_pending_cancel_KNOWN_BUG_PLAN_004` to reflect the
flag-level contract that still holds (`clear_cancellation` does clear the
flag — that primitive is unchanged), and update its comment: the start-path
race is fixed because `start_recording` now clears only before `Starting` and
checks at commit. The flag-API assertions themselves stay valid.

**Verify**: `cd src-tauri && cargo test recording_state_characterization` →
all pass.

### Step 5: Full suite + format

**Verify**: `cd src-tauri && cargo test` → all pass.
**Verify**: `cd src-tauri && cargo fmt --check` → exit 0.

## Test plan

- Updated characterization test (Step 4).
- The commit-point guard is inside a function requiring a live `AppHandle`,
  recorder, and devices — not unit-testable in this codebase today. Manual
  smoke (required, macOS):
  1. Toggle mode: trigger record hotkey, immediately press Escape repeatedly
     during startup → app must end Idle, no pill stuck, no recording running
     (verify by triggering record again: it starts fresh).
  2. PTT mode: hold PTT, release during init → unchanged behavior (recorder
     stops, `PTT_START_ABORTED_AFTER_RELEASE` path).
  3. Normal record/stop/transcribe → unchanged.

## Done criteria

ALL must hold:

- [ ] `grep -n "Clear cancellation flag for new recording" src-tauri/src/commands/audio.rs`
      → no match (old comment gone from the commit point)
- [ ] `start_recording` clears cancellation before `Starting` and checks
      `is_cancellation_requested()` at the commit point (quote both hunks in
      the report)
- [ ] `cd src-tauri && cargo test` → exit 0
- [ ] `cargo fmt --check` → exit 0
- [ ] Manual smoke scenarios 1-3 performed and reported (or explicitly
      reported as not performable in your environment — that downgrades this
      plan to NEEDS-SMOKE, not DONE)
- [ ] Only in-scope files modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back if:

- The continuation block (`:2430-2506`) no longer matches the excerpts.
- `clear_pending_stop_after_start` or `MEDIA_CONTROLLER` symbols don't resolve
  at the insertion point (helper moved/renamed).
- Plan 003's test module does not exist (003 not executed) — execute 003
  first or report.
- Your Step 3 audit finds a `clear_cancellation` site inside
  `stop_recording`'s normal path that would now swallow the commit-point
  abort (interaction this plan didn't predict).

## Maintenance notes

- A tiny race window remains between the commit-point check and the
  `Recording` transition (microseconds; no awaits between them). Closing it
  fully needs a start-attempt generation token — deliberately deferred; note
  for whoever splits `audio.rs`.
- Reviewer should scrutinize: PTT double-guard ordering (cancel-check must
  come BEFORE the PTT-release guard or after — this plan puts it before;
  either is correct, but the recorder-stop must run exactly once on the
  combined path).
