# Plan 003: Characterization tests for the recording state machine and cancellation flags

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 41351bd..HEAD -- src-tauri/src/state_machine.rs src-tauri/src/state/unified_state.rs src-tauri/src/state/app_state.rs src-tauri/src/tests/`
> On any in-scope drift, compare "Current state" excerpts to live code first;
> mismatch = STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (tests only — risk is writing tests that assert the wrong
  current behavior; no production code changes allowed)
- **Depends on**: none (but plan 004 depends on THIS)
- **Category**: tests
- **Planned at**: commit `41351bd`, 2026-06-10

## Why this matters

`src-tauri/src/commands/audio.rs` (5,322 lines) hosts the recording lifecycle:
`start_recording`, `stop_recording`, `cancel_recording`, plus a transcription
task with many failure branches. It contains only ~24 in-file `#[test]`s,
mostly request builders. The *decision layer* underneath — state-transition
validation, the unified state wrapper, and the `AppState` cancellation/PTT
flags — is pure and testable, but has thin coverage. Plan 004 will change
cancellation semantics during `Starting`; without characterization tests of
today's behavior, that change (and the eventual split of `audio.rs`) cannot be
reviewed with confidence. This plan pins current behavior, including the
known-bad behavior plan 004 will fix (documented as such).

## Current state

Relevant production code (read, do not modify):

- `src-tauri/src/state_machine.rs` — `RecordingStateMachine` with
  `transition_to` (lines 41-84), private `is_valid_transition` (87-120),
  `reset` (126-132), `force_state` (135-152). Existing tests live in its
  `mod tests` (156-254).
- `src-tauri/src/state/unified_state.rs` — `UnifiedRecordingState`: `transition_to`
  (34-47), `current` (50-59), `reset`, `force_set` (70-76),
  `transition_with_fallback` (80-108), poison-recovering `lock_or_recover`
  (111-119). Existing tests in `mod tests` (123-174).
- `src-tauri/src/state/app_state.rs` — `AppState` flags:

```rust
    pub should_cancel_recording: Arc<AtomicBool>,     // line 51
    pub pending_stop_after_start: Arc<AtomicBool>,    // line 52
    ...
    pub fn request_cancellation(&self) {              // line 120
        self.should_cancel_recording.store(true, Ordering::SeqCst);
        ...
    }
    pub fn clear_cancellation(&self) {                // line 125
        self.should_cancel_recording.store(false, Ordering::SeqCst);
    }
    pub fn is_cancellation_requested(&self) -> bool { // line 129
        self.should_cancel_recording.load(Ordering::SeqCst)
    }
```

- The interleaving this suite must document (today's actual behavior, which
  plan 004 fixes): `cancel_recording` (`commands/audio.rs:4889-5011`) calls
  `request_cancellation()` then maps `Starting → Idle`; the in-flight
  `start_recording` continuation later calls `clear_cancellation()`
  (`commands/audio.rs:2432-2433`) and transitions to `Recording`
  (`:2488`) — i.e. **a cancellation requested while `Starting` is erased by
  the start path**.

Test conventions in this repo:

- Integration-style backend tests live in `src-tauri/src/tests/*.rs`,
  registered as modules in `src-tauri/src/tests/mod.rs`. Use
  `src-tauri/src/tests/shortcut_bindings.rs` as the structural exemplar
  (plain `#[test]` fns, helper constructors at top of file).
- Run with `cd src-tauri && cargo test <module_name>`.

## Commands you will need

| Purpose            | Command                                                       | Expected on success |
|--------------------|---------------------------------------------------------------|---------------------|
| Run new module     | `cd src-tauri && cargo test recording_state_characterization` | all new tests pass  |
| Full backend tests | `cd src-tauri && cargo test`                                  | all pass            |
| Rust format        | `cd src-tauri && cargo fmt --check`                           | exit 0              |

## Scope

**In scope**:
- `src-tauri/src/tests/recording_state_characterization.rs` (create)
- `src-tauri/src/tests/mod.rs` (add one `mod` line)
- Visibility-only tweaks if strictly needed (e.g. `pub(crate)` on an existing
  fn so the test module can call it). Behavior changes are PROHIBITED.

**Out of scope**:
- Any behavior change in `state_machine.rs`, `unified_state.rs`,
  `app_state.rs`, `commands/audio.rs`.
- Tests requiring a live `AppHandle`/Tauri runtime — stay at the pure layer.
- Duplicating cases already covered in the in-file `mod tests` of
  `state_machine.rs` / `unified_state.rs` (read them first; add only missing
  cases).

## Git workflow

- Branch: `advisor/003-recording-state-tests`
- Commit message: `test: characterize recording state machine and cancellation flags`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Read the existing tests to avoid duplication

Read `src-tauri/src/state_machine.rs:156-254` and
`src-tauri/src/state/unified_state.rs:123-174`. List which (from → to)
transition pairs are already asserted.

**Verify**: produce the list in your report.

### Step 2: Create the test module

Create `src-tauri/src/tests/recording_state_characterization.rs` and register
`mod recording_state_characterization;` in `src-tauri/src/tests/mod.rs`
(match the existing `mod` list style there).

**Verify**: `cd src-tauri && cargo test recording_state_characterization` →
compiles, 0 tests run (empty module ok at this point).

### Step 3: Full transition-validity table test

Add a table-driven test that exercises `RecordingStateMachine::transition_to`
for **every** ordered pair of `RecordingState` variants (enumerate variants
explicitly; find the enum with
`grep -rn "pub enum RecordingState" src-tauri/src/`). Assert exactly which
pairs are `Ok` and which are `Err`, matching the live `is_valid_transition`
(read `state_machine.rs:87-120` — recover the elided body with
`sed`-free reading: open the file at those lines). The table in the test IS
the characterization: copy the truth from the code, then assert it.

Also assert:
- `reset()` always lands in the initial state regardless of prior state
  (drive the machine into each reachable state via valid transitions or
  `force_state`).
- `force_state` bypasses validation (pick one invalid pair and confirm
  `force_state` succeeds where `transition_to` errs).

**Verify**: `cd src-tauri && cargo test recording_state_characterization` →
all pass.

### Step 4: `UnifiedRecordingState` semantics

Add tests (skip any already covered, per Step 1):
- `transition_with_fallback` invokes the fallback exactly when the primary
  transition is invalid, and applies the fallback's returned state.
- `transition_with_fallback` where the fallback returns `None` → the call
  errors and state is unchanged.
- `force_set` then `current()` round-trips.

**Verify**: same test command, all pass.

### Step 5: Cancellation-flag characterization (documents the plan-004 bug)

Add tests against `AppState`'s flag API. If constructing a full `AppState` is
impractical without a Tauri handle, test the same semantics on the underlying
pattern via the public methods if `AppState::default()`/`new()` is
constructible without an `AppHandle` (check `app_state.rs:82-86` — fields are
plain `Arc` constructions, so plain construction is expected to work).

1. `request_cancellation` → `is_cancellation_requested()` is true.
2. `clear_cancellation` after a request → `is_cancellation_requested()` is
   false. Name this test
   `clear_cancellation_erases_pending_cancel_KNOWN_BUG_PLAN_004` and add a
   comment: *"Documents current behavior: a cancel requested while Starting is
   erased by start_recording's clear_cancellation (commands/audio.rs:2432).
   Plan 004 changes the start path to check-before-clear; when it lands,
   update/rename this test to assert the new contract."*
3. `pending_stop_after_start`: `swap(false)` returns the prior value and
   resets the flag (mirrors the consumption at `commands/audio.rs:2494-2497`).

**Verify**: same test command, all pass.

### Step 6: Full suite + format

**Verify**: `cd src-tauri && cargo test` → all pass.
**Verify**: `cd src-tauri && cargo fmt --check` → exit 0.

## Test plan

This plan IS the test plan. Expected new coverage: ~10-15 tests across the
three layers above. Structural model: `src-tauri/src/tests/shortcut_bindings.rs`.

## Done criteria

ALL must hold:

- [ ] `cd src-tauri && cargo test recording_state_characterization` → ≥10
      tests, all pass
- [ ] Full `cargo test` passes
- [ ] `cargo fmt --check` → exit 0
- [ ] No production source file shows behavior diffs (`git diff` touches only
      `src-tauri/src/tests/` plus at most visibility keywords)
- [ ] One test is named with `KNOWN_BUG_PLAN_004` and carries the comment from
      Step 5
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back if:

- `AppState` cannot be constructed without a Tauri `AppHandle` (then the flag
  tests need a seam — report instead of refactoring production code).
- The transition table in `is_valid_transition` is unreadable/ambiguous such
  that you'd be guessing expected values.
- You find an existing test that contradicts the live code's behavior (that's
  a finding, not something to silently "fix").

## Maintenance notes

- Plan 004 must update the `KNOWN_BUG_PLAN_004` test to the fixed contract —
  that update is part of plan 004's done criteria.
- Any future split of `commands/audio.rs` (see plans index: deferred
  god-module finding) should keep this module green as the behavioral anchor.
