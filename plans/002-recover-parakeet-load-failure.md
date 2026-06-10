# Plan 002: Route Parakeet model-load failure through the shared error recovery path

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 41351bd..HEAD -- src-tauri/src/commands/audio.rs`
> If the file changed since this plan was written, compare the "Current state"
> excerpts against the live code before proceeding; on a mismatch, treat it as
> a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `41351bd`, 2026-06-10

## Why this matters

When transcription starts with the Parakeet engine and the model fails to
load, the spawned transcription task sets the recording state to `Error`,
shows a pill toast, and **returns immediately**. Every other failure branch in
the same task goes through a shared handler that schedules an `Error → Idle`
recovery after ~2 s and hides the pill. The Parakeet branch bypasses it, so
the app can sit in `Error` indefinitely — the only thing that saves the next
recording is an unrelated "stuck in Error" recovery hack at the top of
`start_recording`. Routing the failure through the normal
`TranscriptionFailure::Local` path makes recovery consistent and deletes a
special case.

## Current state

File: `src-tauri/src/commands/audio.rs` (5,322 lines). Inside the transcription
task spawned at line 3150 (`let task_handle = tokio::spawn(async move {`),
there is a `match &engine_selection_for_task` producing
`transcription_result: Result<TranscriptionResult, TranscriptionFailure>`
(line 3178).

The buggy branch, `audio.rs:3250-3261`:

```rust
                ActiveEngineSelection::Parakeet { model_name } => {
                    let parakeet_manager = app_for_task.state::<ParakeetManager>();
                    if let Err(e) = parakeet_manager.load_model(&app_for_task, model_name).await {
                        let message = format!("Parakeet model load failed: {e}");
                        update_recording_state(
                            &app_for_task,
                            RecordingState::Error,
                            Some(message.clone()),
                        );
                        pill_toast(&app_for_task, &message, 1500);
                        return;
                    }
```

The `return;` exits the whole spawned task. Nothing schedules `Error → Idle`,
and the temp audio file cleanup at line 3411
(`std::fs::remove_file(&audio_path_clone)`) is also skipped.

The shared recovery handler this should flow into —
`audio.rs:3804-3836`, the generic `TranscriptionFailure::Local(e)` arm:

```rust
                    TranscriptionFailure::Local(e) => {
                        // For other errors, show error state briefly
                        update_recording_state(
                            &app_for_task,
                            RecordingState::Error,
                            Some(e.clone()),
                        );

                        // Emit error via pill toast
                        pill_toast(&app_for_task, e, 1500);

                        // Transition back to Idle after a delay
                        // This ensures we don't get stuck in Error state
                        let app_for_reset = app_for_task.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            ...
                            update_recording_state(&app_for_reset, RecordingState::Idle, None);
                        });
                    }
```

Note: the same arm shape (`Err(TranscriptionFailure::Local(e))`) is already
produced by the Whisper branch at `audio.rs:3313` — that is the pattern to
match.

How the match arm must produce a value: each arm of the engine match evaluates
to `Result<TranscriptionResult, TranscriptionFailure>`. The Parakeet arm
continues after the load to call
`parakeet_manager.transcribe_with_custom_vocabulary(...)` (line 3268) whose
result is already mapped into the same `Result` type.

## Commands you will need

| Purpose        | Command                                   | Expected on success |
|----------------|-------------------------------------------|---------------------|
| Compile        | `cd src-tauri && cargo check`             | exit 0              |
| Backend tests  | `cd src-tauri && cargo test`              | all pass            |
| Rust format    | `cd src-tauri && cargo fmt --check`       | exit 0              |

## Scope

**In scope** (the only file you should modify):
- `src-tauri/src/commands/audio.rs` — only the Parakeet load-failure branch
  (lines ~3250-3261).

**Out of scope**:
- The shared error handler at 3692-3838 — do not change its behavior.
- `ParakeetManager::load_model` itself.
- Any other engine branch (Whisper, Soniox/Cloud, Remote).

## Git workflow

- Branch: `advisor/002-parakeet-load-recovery`
- Commit message: `fix: recover to idle after parakeet model load failure`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Replace the early-return with a `TranscriptionFailure::Local` error

In `src-tauri/src/commands/audio.rs`, change the load-failure block inside the
`ActiveEngineSelection::Parakeet { model_name }` arm from manual
state-setting + `return;` to producing an error value from the match arm:

```rust
                ActiveEngineSelection::Parakeet { model_name } => {
                    let parakeet_manager = app_for_task.state::<ParakeetManager>();
                    if let Err(e) = parakeet_manager.load_model(&app_for_task, model_name).await {
                        Err(TranscriptionFailure::Local(format!(
                            "Parakeet model load failed: {e}"
                        )))
                    } else {
                        // ... existing vocabulary + transcribe_with_custom_vocabulary code,
                        // unchanged, as the else-branch value of this arm
                    }
                }
```

Mechanically: wrap the existing post-load code (from
`let custom_vocabulary = ...` through the end of the existing
`match parakeet_manager.transcribe_with_custom_vocabulary ... }` block) in the
`else` branch, and make the `if let Err(e)` branch evaluate to
`Err(TranscriptionFailure::Local(...))`. Delete the `update_recording_state`,
`pill_toast`, and `return;` lines from the failure branch — the shared
handler at 3804 now performs all three (with the added Idle recovery).

**Verify**: `cd src-tauri && cargo check` → exit 0.

### Step 2: Confirm behavior equivalence of the user-visible toast

Read the generic `TranscriptionFailure::Local` arm (3804-3836) and confirm it
still: sets `RecordingState::Error` with the message, calls
`pill_toast(..., e, 1500)`, and schedules Idle after 2 s. No code change —
this is a read gate so you know the replacement covers the deleted lines.

**Verify**: visual confirmation; quote the three lines in your report.

### Step 3: Run tests and format

**Verify**: `cd src-tauri && cargo test` → all pass.
**Verify**: `cd src-tauri && cargo fmt --check` → exit 0.

## Test plan

No new automated test: exercising this branch requires a live `AppHandle`,
a registered `ParakeetManager`, and a failing model load — the existing suite
has no harness for that (`load_model` is not trait-abstracted). The change
reduces this branch to the already-exercised shared error path.

Manual smoke (macOS only, optional but recommended — Parakeet is
macOS-only): select a Parakeet model, rename its model directory on disk,
trigger a recording, observe: error toast appears, and after ~2 s the app
returns to Idle and a new recording can start.

## Done criteria

ALL must hold:

- [ ] The strings `update_recording_state` / `pill_toast` / `return;` no
      longer appear inside the Parakeet **load-failure** branch
      (`grep -n "Parakeet model load failed" src-tauri/src/commands/audio.rs`
      shows it only inside a `TranscriptionFailure::Local(...)` constructor)
- [ ] `cd src-tauri && cargo test` → exit 0
- [ ] `cd src-tauri && cargo fmt --check` → exit 0
- [ ] Only `src-tauri/src/commands/audio.rs` modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back if:

- The Parakeet arm no longer looks like the excerpt (drift — e.g. plan 012's
  contract work landed first).
- The match-arm types don't line up (e.g. the surrounding match no longer
  produces `Result<TranscriptionResult, TranscriptionFailure>`).
- Any existing test fails after the change and the failure is not a trivially
  updated assertion on this branch's behavior.

## Maintenance notes

- Plan 012 (shared transcription contract) will eventually replace
  `TranscriptionFailure` with a typed error taxonomy; this change keeps the
  Parakeet branch aligned with whatever that handler becomes.
- Reviewer should scrutinize: the moved block must not change `await`
  ordering (the `else` branch still awaits the same futures in the same
  order).
