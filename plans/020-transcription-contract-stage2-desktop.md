# Plan 020: Shared transcription contract — Stage 2 (port desktop recording)

> **STATUS: DRAFT for review — not claimed, not committed.** Plan number 020 is
> provisional (next free). Do not execute until approved; on approval, claim the
> board row + commit per `plans/README.md` concurrency protocol.
>
> **Executor instructions**: This is **Stage 2** of the migration map
> for the shared transcription contract. Stage 2
> makes the **desktop recording hot path** the first real consumer of
> `transcribe_with_app` — but **only for local + cloud engines**. The active
> **remote** desktop path stays on the existing inline branch until **Stage 5**
> (remote client / multipart). This is a hot path: it ends in a **manual smoke**,
> not just unit tests.
>
> **Baseline**: branch `plan/v2-roadmap` after plan 014 Stage 1 (`fac6977`,
> `dc2731a`) + Fix #2 (translation hard-fail). **Oracle-reviewed approach**
> (`agent://ConfirmStage2AndFix2`).

## Status

- **Priority**: P1 (proves the contract on the hot path; lands 015 at the seam)
- **Effort**: L
- **Risk**: HIGH — desktop record→transcribe→insert is the core flow.
- **Depends on**: 014 Stage 1 (DONE), Fix #2 (translation hard-fail), and
  coordinates with **015** (NEEDS-SMOKE — its watchdog/retry move here).
- **Category**: backend architecture / hot-path migration
- **Planned at**: 2026-06-13

## Decision (oracle-confirmed)

**Option A**: port only local/cloud `ActiveEngineSelection` (Whisper / Parakeet /
Cloud) through `TranscriptionRequest` + `transcribe_with_app`. Leave
`ActiveEngineSelection::Remote` on the **current inline branch unchanged** — it
carries fragile remote-failure history preservation (`audio.rs:3806-3816`,
`4129-4176`) we do not disturb until Stage 5. Accept the short-lived two-path
split on the desktop flow.

The Stage 1 executor is the right shape but **not yet a drop-in** for the hot
path. Stage 2 = harden the executor seam (carry today's behaviors) **then** swap
the desktop local/cloud branch to call it.

## Part 1 — Executor seam hardening (must precede the desktop swap)

The executor currently misses six behaviors the desktop path has. Close each so
the swap is behavior-preserving:

1. **Cancellation shares the app flag.** Add `CancellationToken::from_arc(Arc<AtomicBool>)`
   to `transcription/request.rs`; desktop builds the token from
   `AppState.should_cancel_recording` so user-cancel and the watchdog both flip
   the same flag the engines already observe (`audio.rs:3500-3516`).
2. **Timeout/watchdog at the seam (015).** See Part 2.
3. **Whisper retry at the seam.** Desktop retries Whisper 3× (`audio.rs:3552-3607`);
   the executor calls once. Move retry into the policy wrapper (Part 2).
4. **Whisper `initial_prompt`.** Desktop passes `compile_whisper_initial_prompt(...)`
   (`audio.rs:3549-3572`); executor passes `None` (regression). Add
   `initial_prompt: Option<String>` to `TranscriptionRequest` (additive); the
   Whisper arm forwards it. Desktop computes it as today and sets the field.
5. **Duration gate / normalization ownership.** Desktop rejects too-short
   recordings *before* dispatch and normalizes (`audio.rs:3315-3423`); the
   executor now normalizes internally (`executor.rs:127-149`). Keep the
   **too-short pre-flight gate in the desktop caller** (before building the
   request); desktop passes the **raw** recording path so the executor
   normalizes exactly once (no double-normalize).
6. **Cleanup vs history race.** Desktop must own history/preservation, so it
   passes `CleanupPolicy::CallerOwns`; the executor never deletes the desktop
   recording. Desktop keeps its existing save/cleanup ordering
   (`audio.rs:3806-3821`, `4056-4062`).

## Part 2 — 015 reconciliation (Option C: executor policy wrapper)

Make `transcribe_with_app` the **policy boundary**, split internally:
- `resolve_active(...)` — engine resolution (exists).
- `run_with_policy(request, active, job, input_path)` — owns **timeout +
  retry**; calls `route_once`.
- `route_once(...)` — one engine attempt (today's per-engine arms).

Rules (from plan 015):
- `TimeoutPolicy::Interactive` runs the **015 sibling watchdog** that, on
  deadline, **sets the cancellation flag** — NEVER `tokio::time::timeout` around
  the blocking CPU `state.full(...)` decode (015 proved it does not preempt;
  `plans/015...md:239-244`, watchdog at `:281-321`).
- Whisper retry wraps `route_once` **for Whisper only** (one attempt = one engine
  call; the wrapper owns retry).
- Cloud/network arms may keep an async timeout (matches `audio.rs:3683-3708`).
- Remote (old inline path) keeps its own client timeout until Stage 5.

**Coordinate with 015**: treat current inline 015 code as the behavioral
baseline but **not done** until smoke. Stage 2 moves those semantics to the seam
and the **integrated** path is what gets smoked (not the old inline path).

## Part 3 — Desktop swap (local/cloud only)

In the desktop spawned task (`commands/audio.rs` ~3120-4077):
- Resolve the engine (existing `resolve_engine_for_model`). If
  `ActiveEngineSelection::Remote` → **unchanged old inline branch**.
- Else build `TranscriptionRequest { source: DesktopRecording, audio:
  Path{raw_recording, CallerOwns}, engine: Explicit{..}, spoken_language, task,
  context, timeout: Interactive, cancellation: from app flag, initial_prompt }`
  and call `transcribe_with_app`.
- Map `Result<TranscriptionResult, TranscriptionError>` onto the existing
  delivery + history + pill/state flow, then the writing pipeline
  (`process_transcription`, now typed via Fix #2). Preserve the too-short gate,
  failure history, and cancel/non-speech behaviors exactly.

## Scope boundaries (Stage 2 does NOT)

- Touch the active **remote** desktop branch (Stage 5) or `HostDefault`/wire.
- Port upload / audio-bytes / CLI callsites (Stage 3) or the remote server
  (Stage 4).
- Delete the old inline local/cloud code paths beyond what the swap replaces in
  the desktop task (broad deletion is Stage 6); keep the diff to the desktop
  flow + executor seam.

## Tests

- Executor policy wrapper (`run_with_policy`): timeout-policy → watchdog wiring,
  retry-count for Whisper, cancellation propagation (pure where possible; the
  watchdog/flag interaction is the key invariant).
- `CancellationToken::from_arc` shares state.
- Desktop: too-short still rejected pre-dispatch; cancel mid-transcription;
  non-speech path; failure preserves history.

## Verification + smoke (REQUIRED)

```bash
cargo fmt --check; cargo clippy --all-targets -- -D warnings; cargo test
pnpm typecheck && pnpm lint && pnpm test --run
```
**Manual smoke (hot path — cannot be skipped):** hotkey → record → insert for
Whisper, Parakeet, and a cloud provider; cancel mid-transcription; long decode
hits the watchdog without losing speech; too-short recording rejected cleanly;
active-remote desktop still works (old path untouched).

## Rollback

Revert the desktop-task swap (restore the inline local/cloud branch) and the
executor policy-wrapper split. The contract module + Fix #2 remain. Because the
remote branch was untouched, remote desktop is unaffected either way.

## STOP conditions

- If moving 015 to the seam can't preserve never-lose-speech (watchdog must set
  cancellation, not abort a blocking decode), STOP and keep 015 inline for the
  smoke baseline.
- If the executor's `TranscriptionError` can't map cleanly onto the desktop
  failure/history path without behavior change, STOP and reconcile before swap.

## Post-oracle implementation decisions (2026-06-15)

Oracle hardening (`agent://HardenStage2`) + a code-grounded saved-recording
quality review settled these. The plan above stands with these refinements:

- **Saved-recording format = UNCHANGED (user-decided).** Today the desktop
  already saves the normalized 16 kHz-mono WAV for Whisper/Parakeet and the raw
  capture for Cloud (cloud skips normalization). The normalized file is the
  ASR-native contract (`ffmpeg -ac 1 -ar 16000 -sample_fmt s16`,
  `ffmpeg/mod.rs:173`) and is already the production re-transcription artifact —
  zero meaningful quality loss (16 kHz mono is the engines' native input).
  Stage 2 keeps the desktop normalize+gate+save flow and does NOT switch to
  saving raw. Full normalization-ownership cutover is deferred to Stage 6.
- **Executor must NOT re-normalize already-normalized input.** Add an
  idempotency guard to the executor's normalize step: if the input WAV is
  already 16 kHz / mono / 16-bit int, use it as-is (no temp, no ffmpeg). The
  desktop pre-normalizes Whisper/Parakeet and passes that path; Cloud passes raw
  and is not normalized. Keeps normalize-exactly-once; benefits future callers.
- **Normalization happens OUTSIDE the Whisper retry loop.** `run_with_policy`
  prepares the attempt path once and keeps it alive across all attempts;
  `route_once` never normalizes.
- **Watchdog budget computed at the seam.** The executor computes
  `transcription_watchdog_budget` from the attempt-path duration (widen it to
  `pub(crate)`); the desktop stops passing it for local/cloud.
- **Single-save + marker invariant preserved.** `transcribe_with_app` returns
  `Ok(TranscriptionResult)` with NO history side effects. The existing
  `WritingError::TranslationFailed` arm stays the sole writing-history save and
  still returns `should_deliver=false` (early return before the post-match
  save). Translation failure is NOT mapped to `TranscriptionFailure`.
- **Delegation:** request.rs (from_arc + initial_prompt) DONE via subagent.
  executor.rs (policy split) and commands/audio.rs (desktop swap) are
  collision-prone → owned by the main agent, sequential; tests delegated.
- **Oracle unavailable mid-session** (subagent tool-schema error); the quality
  review was done code-level by the main agent and is high-confidence.
