# Plan 001: Stop logging dictated text, clipboard contents, and Whisper segments

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 41351bd..HEAD -- src-tauri/src/commands/text.rs src-tauri/src/whisper/transcriber.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: security
- **Planned at**: commit `41351bd`, 2026-06-10

## Why this matters

VoiceTypr's product promise is private, offline dictation. Today every
transcription is written **in full** to the app's rotating log files: the text
placed on the clipboard is logged, the clipboard is read back and logged again,
and each Whisper segment is logged individually. The bug-report feature
(`src/utils/crashReport.ts`) attaches the tail of the latest log file to crash
and manual reports submitted to `https://voicetypr.com/api/v1/bug-reports`. The
log redactor in `src-tauri/src/commands/logs.rs` (`redact_log_content`,
lines 196–281) removes secrets/emails/paths — it cannot recognize arbitrary
dictated speech. Net effect: user dictation can leave the machine inside a
diagnostic payload. Removing transcript content from logs closes this leak
without touching the report pipeline.

## Current state

Files involved:

- `src-tauri/src/commands/text.rs` — clipboard-based text insertion; logs full text twice.
- `src-tauri/src/whisper/transcriber.rs` — Whisper transcription loop; logs each segment's text.
- `src-tauri/src/commands/logs.rs` — log tail + redaction for bug reports (do NOT modify; context only).
- `src/utils/crashReport.ts` — attaches `latestLog.content` to outbound reports (do NOT modify; context only).

Exact sites (verified at commit `41351bd`):

`src-tauri/src/commands/text.rs:188` and `:195`:

```rust
        log::info!("Set clipboard content: {}", text);

        // Small delay to ensure clipboard is ready
        thread::sleep(Duration::from_millis(50));

        // Verify clipboard content was set
        if let Ok(clipboard_check) = clipboard.get_text() {
            log::info!("Clipboard content verified: {}", clipboard_check);
        }
```

`src-tauri/src/whisper/transcriber.rs:669-673`:

```rust
        for (i, segment) in state.as_iter().enumerate() {
            let segment_text = segment.to_string();
            log::info!("[TRANSCRIPTION_DEBUG] Segment {}: '{}'", i, segment_text);
            text.push_str(&segment_text);
            text.push(' ');
```

Repo convention for content-free logging (already used elsewhere, e.g.
`src-tauri/src/remote/http.rs:486-491` logs `{} chars` and
`src-tauri/src/commands/audio.rs:3434-3437` logs
`"Transcription successful, {} chars"`): log **lengths and stage names, never
content**. Match it.

## Commands you will need

| Purpose        | Command                                      | Expected on success |
|----------------|----------------------------------------------|---------------------|
| Install        | `pnpm install --frozen-lockfile`             | exit 0              |
| Backend tests  | `cd src-tauri && cargo test`                 | all pass            |
| Rust format    | `cd src-tauri && cargo fmt --check`          | exit 0              |
| Grep gate      | see Done criteria                            | no matches          |

## Scope

**In scope** (the only files you should modify):
- `src-tauri/src/commands/text.rs`
- `src-tauri/src/whisper/transcriber.rs`
- Any additional Rust file where Step 3's bounded search finds a log line that
  prints full transcript/clipboard content (each such edit is a one-line
  change of the same shape).

**Out of scope** (do NOT touch, even though they look related):
- `src-tauri/src/commands/logs.rs` — the redaction layer is a second line of
  defense; changing its patterns is a separate concern.
- `src/utils/crashReport.ts` — report payload shape is intentional.
- Log lines that print *lengths*, *model names*, *timings*, or *error
  messages* — those are fine and must stay.
- Log lines printing user-configured dictionary/replacement *rule names* —
  leave them; report them in your summary if you think they're sensitive.

## Git workflow

- Branch: `advisor/001-remove-transcript-logging`
- Conventional commits (repo style, cf. `git log`: `fix: harden v2 readiness gaps`).
  Suggested: `fix: stop logging transcript and clipboard content`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Replace the two clipboard content logs in `text.rs`

In `src-tauri/src/commands/text.rs` (function `insert_via_clipboard`):

- Replace `log::info!("Set clipboard content: {}", text);` with
  `log::info!("Set clipboard content ({} chars)", text.chars().count());`
- Replace the verification block's
  `log::info!("Clipboard content verified: {}", clipboard_check);` with
  `log::info!("Clipboard content verified ({} chars)", clipboard_check.chars().count());`

**Verify**: `cd src-tauri && cargo check` → exit 0.

### Step 2: Replace the per-segment log in `transcriber.rs`

In `src-tauri/src/whisper/transcriber.rs`, in the segment loop (~line 669):

- Replace
  `log::info!("[TRANSCRIPTION_DEBUG] Segment {}: '{}'", i, segment_text);`
  with
  `log::debug!("[TRANSCRIPTION_DEBUG] Segment {}: {} chars", i, segment_text.chars().count());`

(Downgrade to `debug` is intentional: per-segment lines are diagnostic noise.)

**Verify**: `cd src-tauri && cargo check` → exit 0.

### Step 3: Bounded sweep for other content logs

Run exactly these searches from the repo root:

```
grep -rn "Set clipboard content" src-tauri/src/
grep -rn "Clipboard content verified" src-tauri/src/
grep -rnE 'log::(info|warn|debug|error)!\([^)]*"[^"]*\{\}[^"]*"[^)]*\b(text|transcript|transcription|segment_text|raw_text|clipboard)' src-tauri/src/ | grep -viE '(len\(\)|chars\(\)|count\(\)|\.is_empty|chars\b|bytes\b|size)'
```

Decision rule for each hit of the third search: if the interpolated value is
**dictated/transcribed text or clipboard content**, apply the same fix
(log `chars().count()` instead of content). If the value is an error message,
a model name, a file path, or a rule/profile name — leave it and list it in
your final report. When unsure: leave it, list it.

**Verify**: first two greps → no matches. Third grep → no remaining hits that
print transcript content.

### Step 4: Run the backend test suite

**Verify**: `cd src-tauri && cargo test` → all pass (no test asserts on the
removed log strings; if one does, update the assertion to the new
content-free message — that is in scope).

**Verify**: `cd src-tauri && cargo fmt --check` → exit 0.

## Test plan

No new tests: logging output is not asserted in this codebase and adding a
log-capture harness is not worth the machinery. The enforcement mechanism is
the grep gate in Done criteria (cheap to re-run in review).

## Done criteria

ALL must hold:

- [ ] `grep -rn "Set clipboard content: {}" src-tauri/src/` → no matches
- [ ] `grep -rn "Clipboard content verified: {}" src-tauri/src/` → no matches
- [ ] `grep -n "Segment {}: '{}'" src-tauri/src/whisper/transcriber.rs` → no matches
- [ ] `cd src-tauri && cargo test` → exit 0
- [ ] `cd src-tauri && cargo fmt --check` → exit 0
- [ ] `git status` shows no modified files outside the Scope list
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- The excerpts in "Current state" don't match the live code (drift).
- A test fails because product code *parses* one of these log lines (i.e. the
  log text is load-bearing, not just diagnostic).
- Step 3's sweep finds more than ~10 additional content-logging sites —
  that means the problem is systemic and deserves a broader logging-policy
  change, not piecemeal edits.

## Maintenance notes

- Review rule going forward: any `log::*!` that interpolates `text`,
  `transcript*`, or clipboard reads must log a length, never content.
- A follow-up (deliberately out of scope) is adding a transcript-aware pattern
  to `redact_log_content` in `src-tauri/src/commands/logs.rs` as
  defense-in-depth for logs written by older app versions.
