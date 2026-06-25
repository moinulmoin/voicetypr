# Plan 007: Delay clipboard restore until the paste has been consumed, with behavioral tests

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 41351bd..HEAD -- src-tauri/src/commands/text.rs src-tauri/src/tests/regression_tests.rs`
> On drift, compare "Current state" excerpts to live code first; mismatch = STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (touches the text-insertion path every dictation goes
  through; behavior must stay identical except for restore timing)
- **Depends on**: none
- **Category**: bug + tests
- **Planned at**: commit `41351bd`, 2026-06-10

## Why this matters

"Your clipboard is preserved" is a product promise, and `AGENTS.md` documents
it as "restored after 500ms". The code restores the previous clipboard
**immediately** after dispatching the Cmd+V keystroke. Many apps process the
paste event asynchronously: if the restore lands before the target app reads
the clipboard, the user gets their *old* clipboard pasted instead of the
transcription (intermittent, target-app-dependent). Additionally, the only
test of this behavior is `#[ignore]`d and tests nothing real. This plan adds
the documented delay and makes the save/set/paste/restore sequencing unit-
testable through a small seam.

## Current state

`src-tauri/src/commands/text.rs`:

- Entry: `insert_text` command (`:90-139`) — reads
  `keep_transcription_in_clipboard` from the settings store (default `false`,
  cf. `commands/settings.rs:93`), then runs `insert_via_clipboard` inside
  `tokio::task::spawn_blocking` (so `thread::sleep` is acceptable in this
  path).
- `insert_via_clipboard(text, has_accessibility_permission, app_handle, keep_transcription_in_clipboard)`
  (`:156-283`):
  1. Captures previous clipboard text when `keep == false` (`:167-180`).
  2. Sets transcription on clipboard, sleeps 50 ms, verifies (`:182-196`).
  3. If no accessibility permission → returns Err, text intentionally left on
     clipboard (`:199-205`).
  4. Pastes via `try_paste_with_rdev()`, falls back to AppleScript with panic
     protection; paste failures do NOT fail the function (text stays in
     clipboard, pill toast shown) (`:211-257`).
  5. Restore (`:262-274`) — the bug:

```rust
    if !keep_transcription_in_clipboard {
        if insertion_result.is_ok() {
            if let Some(previous_text) = previous_clipboard_text {
                if let Err(e) = clipboard.set_text(&previous_text) {
                    log::error!("Failed to restore original clipboard text: {}", e);
                } else {
                    log::debug!("Restored original clipboard text after paste");
                }
            } else { ... leave unchanged ... }
        } else { ... }
    }
```

No delay exists between paste dispatch and restore.

- The dead test: `src-tauri/src/tests/regression_tests.rs:70-72` —
  `#[ignore]`d, creates/restores clipboard text itself, never calls the real
  path ("implementation is private").

Important interaction: paste-failure with fallback success semantics — when
rdev fails but AppleScript succeeds, `insertion_result` is `Ok`. When BOTH
fail, the code still returns `Ok(())` (text intentionally left in clipboard
for manual paste, `:228-255`) — **note**: in that case restore would
overwrite the transcription the user was just told is "copied to clipboard".
That is a second latent bug this plan fixes via the outcome enum below.

## Target design

Extract the sequencing decision into a pure, testable layer in `text.rs`:

```rust
/// Minimal clipboard seam so sequencing is unit-testable.
trait ClipboardOps {
    fn get_text(&mut self) -> Result<String, String>;
    fn set_text(&mut self, text: &str) -> Result<(), String>;
}

/// What happened to the paste attempt — decides restore behavior.
#[derive(Debug, PartialEq)]
enum PasteOutcome {
    Pasted,                 // a paste keystroke was delivered
    LeftInClipboard,        // paste failed; transcription must stay on clipboard
    NoPermission,           // accessibility missing; transcription stays, Err returned
}

const CLIPBOARD_RESTORE_DELAY: Duration = Duration::from_millis(500);
```

New pure function (no app handle, no real clipboard, no rdev):

```rust
fn run_clipboard_insertion(
    clipboard: &mut dyn ClipboardOps,
    paste: &mut dyn FnMut() -> PasteOutcome,
    sleep: &mut dyn FnMut(Duration),
    text: &str,
    keep_transcription_in_clipboard: bool,
) -> Result<PasteOutcome, String>
```

Behavior (must replicate today's semantics + the two fixes):
1. previous = if keep { None } else { clipboard.get_text().ok() }
2. clipboard.set_text(text)?; sleep(50ms); (verification read optional —
   keep it, logging only lengths per plan 001's convention)
3. outcome = paste()
4. Restore rules:
   - `keep == true` → never restore.
   - outcome `NoPermission` → no restore; return Err (message text identical
     to today's, `:204`).
   - outcome `LeftInClipboard` → **no restore** (fix #2: don't clobber the
     fallback copy).
   - outcome `Pasted` → `sleep(CLIPBOARD_RESTORE_DELAY)` then restore
     previous if `Some` (fix #1: the delay).

`insert_via_clipboard` becomes a thin adapter: wraps the real `Clipboard` in
`ClipboardOps`, maps the existing rdev/AppleScript/panic ladder into
`PasteOutcome` (rdev Ok → Pasted; AppleScript Ok → Pasted; both failed or
panic → LeftInClipboard + existing pill toast; no permission → NoPermission),
passes `std::thread::sleep` as the sleep fn. All existing log lines and the
pill toasts stay in the adapter.

## Commands you will need

| Purpose            | Command                                              | Expected on success |
|--------------------|------------------------------------------------------|---------------------|
| Compile            | `cd src-tauri && cargo check`                        | exit 0              |
| Module tests       | `cd src-tauri && cargo test clipboard_insertion`     | all new tests pass  |
| Full backend tests | `cd src-tauri && cargo test`                         | all pass            |
| Rust format        | `cd src-tauri && cargo fmt --check`                  | exit 0              |
| Clippy (CI parity) | `cd src-tauri && cargo clippy -- -D warnings`        | exit 0              |

## Scope

**In scope**:
- `src-tauri/src/commands/text.rs` — the seam, the pure function, the adapter
  rewrite of `insert_via_clipboard`, and an in-file `#[cfg(test)] mod`
  (`clipboard_insertion` tests).
- `src-tauri/src/tests/regression_tests.rs` — DELETE the `#[ignore]`d
  clipboard test (`:70+`), superseded by real unit tests.

**Out of scope**:
- `insert_text` command signature/behavior, `copy_text_to_clipboard`.
- `try_paste_with_rdev` / `try_paste_with_applescript` internals.
- AGENTS.md (its 500 ms claim becomes true; no edit needed).
- Windows paste path differences — adapter maps whatever ladder exists per
  platform into `PasteOutcome`; do not redesign it.

## Git workflow

- Branch: `advisor/007-clipboard-restore-timing`
- Commit message: `fix: delay clipboard restore until paste is consumed`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Introduce the seam and pure function

Add `ClipboardOps`, `PasteOutcome`, `CLIPBOARD_RESTORE_DELAY`, and
`run_clipboard_insertion` to `text.rs` as specified. Implement
`ClipboardOps` for the real `arboard::Clipboard` (thin inherent wrapper
mapping errors to `String`, matching existing error texts).

**Verify**: `cd src-tauri && cargo check` → exit 0.

### Step 2: Rewrite `insert_via_clipboard` as the adapter

Move the rdev/AppleScript ladder into a closure producing `PasteOutcome`;
call `run_clipboard_insertion`. Preserve: every log line (content-free if
plan 001 landed), both pill-toast sites, the exact accessibility error
string, and the `IS_INSERTING` guard in `insert_text` (untouched).

**Verify**: `cargo check` → exit 0; then `cargo test` → all pass (existing
suite green proves no signature breakage).

### Step 3: Unit tests for the sequencing

In-file `#[cfg(test)] mod clipboard_insertion` with a hand-rolled mock
(`Vec`-backed `MockClipboard { current: String, history: Vec<String> }`,
recorded sleeps `Vec<Duration>`). Cases:

1. `restores_after_delay_when_pasted`: keep=false, paste→Pasted →
   final clipboard == previous; recorded sleeps contain
   `CLIPBOARD_RESTORE_DELAY` AFTER the paste call (order-assert via a shared
   event log if needed).
2. `keeps_transcription_when_setting_enabled`: keep=true → final clipboard ==
   transcription; no restore-delay sleep recorded.
3. `no_restore_when_paste_left_in_clipboard`: paste→LeftInClipboard →
   final clipboard == transcription (regression test for fix #2).
4. `no_restore_without_permission`: paste→NoPermission → Err returned,
   clipboard == transcription.
5. `no_previous_text_leaves_clipboard`: get_text errs (non-text clipboard) →
   Pasted → clipboard == transcription, no restore, no error.

### Step 4: Delete the dead ignored test

Remove the `#[ignore]` clipboard test from
`src-tauri/src/tests/regression_tests.rs` (and its helper if now unused).

**Verify**: `cd src-tauri && cargo test` → all pass;
`grep -n "ignore" src-tauri/src/tests/regression_tests.rs` → no clipboard
entry.

### Step 5: Format + clippy

**Verify**: `cargo fmt --check` exit 0; `cargo clippy -- -D warnings` exit 0.

## Test plan

Five unit tests (Step 3) covering: happy path with delay, keep-setting, both
paste-failure modes, and non-text previous clipboard. Manual smoke (macOS,
recommended): dictate into a slow Electron app (e.g. Slack) with something on
the clipboard → transcription pastes, ~0.5 s later clipboard shows the old
content again.

## Done criteria

ALL must hold:

- [ ] `grep -n "CLIPBOARD_RESTORE_DELAY" src-tauri/src/commands/text.rs` →
      const defined as 500 ms and used on the Pasted path
- [ ] 5 new tests pass: `cargo test clipboard_insertion`
- [ ] Ignored clipboard test removed from `regression_tests.rs`
- [ ] `cargo test`, `cargo fmt --check`, `cargo clippy -- -D warnings` exit 0
- [ ] Only in-scope files modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back if:

- `insert_via_clipboard` no longer matches the excerpts (drift).
- The Windows build (`cargo check --target x86_64-pc-windows-msvc` if
  cross-check available; otherwise reading the cfg branches) shows a paste
  ladder that cannot be mapped onto the three `PasteOutcome` variants without
  inventing semantics.
- Any existing test depends on the immediate-restore behavior.

## Maintenance notes

- The 500 ms delay blocks one `spawn_blocking` thread per insertion — fine at
  dictation cadence; revisit only if insertion ever moves to a hot path.
- Reviewer should scrutinize: `LeftInClipboard` no-restore is a deliberate
  behavior CHANGE (old code restored over the fallback copy); the changelog
  entry should mention it.
- If a future "paste verification" lands (reading the target app's reaction),
  it slots into `PasteOutcome` cleanly.
