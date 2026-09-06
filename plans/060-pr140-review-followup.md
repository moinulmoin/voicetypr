# PR #140 review follow-up — 2026-09-06

Baseline: `e97aa900a29f8ace1ddc5967b9eb53394d12e152` on
`fix/047-silent-failures`. This follow-up stays within plans 060/061.
The published PR CI results do not validate this local follow-up.

## Confirmed findings addressed

| Finding | Result |
| --- | --- |
| Account-wide Soniox cleanup can delete other clients' data | Only IDs returned to this app session are eligible, scoped to API endpoint/key; unknown records stay untouched and the UI explains console review. No filename/age assumption grants ownership. |
| Awaited DELETE can turn a finished transcript into a watchdog timeout | Typed and diarized results return before background maintenance; the task retains active protection until cleanup completes. |
| Empty/repeated pagination can run indefinitely; cursors are unescaped | Encoded query parameters, cursor cycle detection, bounded pages/items, malformed/incomplete listing failures prevent destructive work. |
| Generic 429 parsing can apply Soniox quota handling to another provider | Explicit Soniox response handler; other providers retain ordinary rate-limit behavior. |
| Malformed secure-store writes register an empty cache and overwrite data on exit | Validate unopened store before write/delete registration; keep existing cache authoritative and preserve explicit replacement of corrupt entries in valid stores. |
| Superseded Polish loaders write stale settings | AbortSignal ownership reaches async settings, provider probes and model requests. Completed initialization keeps legitimate background work alive; unmount/replacement aborts it. |
| Refreshed CLI cache can reuse help from another binary | Capability cache includes resolved binary path, preserving epoch invalidation. |
| Canceled delivery counted as failure | Explicit canceled/failed/succeeded outcomes; genuine paste/clipboard errors mark failure. |
| Failure telemetry depends on user-facing copy | Executor error code survives desktop conversion; legacy untyped messages retain fallback classification. Display/history/retry behavior stays unchanged. |
| Non-log names can be attached to reports | Exact calendar-valid YYYY-MM-DD matching shared by deletion and attachment. |
| Cleanup button loses its accessible name during progress | Stable accessible name; unrecognized records reported distinctly. |
| Local release script lacks Node preflight | Node 22.19+ required before credential loading/building, matching the minimum CI version and frontend tooling. |
| Diagnostic serialization and stale docs | Assert debugRing wire key; use Sources → Cloud route, plan 060 references, and the actual bounded retry contract. |
| Online/offline telemetry smoke conflated | Separate online alert routing from offline local handling; add consent-on/off checks. |

The combined independent review found one additional progress-count regression:
skipped unknown/active transcription records now count as processed work, and
an HTTP test asserts final progress equals total.

## Findings not applied

- Logging filters do not replace each other: installed tauri-plugin-log 2.9.0
  `Target::filter` appends to a vector and logger construction applies every
  predicate. No behavior change warranted.
- The DEBUG ring already inherits the plugin's desktop formatter, including
  timestamp, target and log level. Adding another formatter would duplicate it.
- Old 047-S1..S5 smoke requests refer to the pre-renumbering plan. The same
  work is now 060-S1..S5; 047 belongs to another workstream. No duplicate rows.
- The old processing-job deletion issue was already protected by current
  reference tracking. The ownership change also leaves all other-client
  uploads untouched; an age heuristic is unnecessary.
- Changelog CLI removal was already corrected on the baseline (pinned 5.0.0,
  invoked with pnpm exec).
- Cleanup wait already exits when AUTO_CLEANUP_RUNNING becomes false.
- Recorder timeout without a callback has no samples to drain. Do not invent
  a second buffer scan; real callback timing remains packaged 060-S9 smoke.

## Validation

- Frontend: 697 tests across 65 files pass (`pnpm exec vitest run src`).
- Typecheck, lint and frontend production build pass.
- The unrestricted frontend script also discovers ignored `.agents`/`.claude`
  skill tests, which fail outside their intended harness. No app test failed;
  the complete application suite above is the relevant gate.
- Release shell syntax passes. Isolated preflight accepts the installed
  supported Node runtime and rejects unsupported runtime before credentials.
- Rust workspace tests: 1,525 passed, 16 ignored. MockRuntime store exit tests
  and the concurrent HTTP/CLI regressions pass.
- Rust formatting passes for all changed files. Clippy passes across the
  workspace and all targets with warnings denied.
- Independent review: no confirmed new data-loss/race blocker; progress issue
  corrected as described above.

## Remaining runtime evidence

Packaged 060-S1..S10 and 061-S1/S2, plus existing 045/050/058/059 release rows,
remain unchecked in SMOKE.md. No real-provider records were created/deleted,
no consent was changed, and no support report was submitted. Windows hardware
and the initiating Windows license-decryption cause remain unverified.

Soniox ownership is deliberately process-local. Failed cleanup from an earlier
app session cannot safely be attributed and requires provider-console review.
This fixes unsafe deletion without claiming to repair historical account data.

## Overnight review follow-through — 2026-09-06

A new review reproduced the Soniox event arriving before Sources mounted.
AppContainer now owns the source filter, and the long-lived event handler saves
Cloud before navigating. Regressions cover escalation from another tab,
escalation while Sources is active, and repeated navigation.

Canceled Polish loader/probe failures no longer emit stale error diagnostics.
Partial Soniox cleanup uses a warning rather than a success toast.
The complete frontend suite now passes 710 tests across 65 files; typecheck and
lint pass. These changes do not modify Rust behavior.

The request to copy executor `retryable` into failed-recording preservation is
not applied: executor retryability controls immediate automatic retries, while
`TranscriptionFailure::is_retryable_failure` controls user recovery from History.
`SMOKE.md` FP-S1 explicitly requires preserving failed audio for an invalid key
when save_recordings is on, so the user can fix the key and re-transcribe.
Discarding that audio for `Unauthorized` would violate the existing recovery
contract. Cancellation and too-short input remain excluded.

The follow-through review also caught preexisting render-time source syncing
calling the newly controlled parent setter. It now initializes tracking from
the current source and synchronizes only later source-category changes from an
effect. An actual stateful parent harness proves Cloud survives initial local
model mounting, no parent-render warning occurs, and browsing survives unrelated
renders. The full frontend suite passes 711 tests; typecheck, lint and build
pass after this correction.

## Provider ambiguity and report redaction — overnight continuation

Accepted Soniox creates now protect uploads before response decoding. A missing,
empty, or undecodable job ID cannot trigger immediate or later automatic orphan
deletion. Cleanup running state and file/record progress are isolated per API
endpoint/key, so overlapping key changes cannot suppress or wake another
account's recovery. The running guard releases on completion/panic/cancellation.

Wrapped-secret redaction now handles escaped delimiters/backslashes in JSON and
Rust Debug values, preserving surrounding context without exposing suffixes.
The legacy remote-settings raw debug statement was also removed at its source.
New HTTP and redaction regressions cover these boundaries. Independent review
found no confirmed new data-loss/privacy/race defect. Final Rust validation:
1,535 passed, 16 ignored; Clippy workspace/all-targets with warnings denied and
changed-file formatting pass. No real provider records or settings were modified.

The ambiguity handling also covers transport loss before response headers:
non-idempotent create POSTs are not blindly retried on transport/5xx failures.
One retry remains for an explicit transient 429 rejection; outer storage-cap
recovery is unchanged. HTTP regressions prove no deletion or duplicate create
on ambiguous failure/cancellation, and normal auth/quota orphan cleanup remains.

## Interactive management deadlines

Storage-management counts/list/delete requests have a 10-second request bound.
Manual and background cleanup share a 60-second total deadline including gate
waits, pagination, pacing and retries. Timeout reports possible partial work
honestly and releases the cleanup/listing guards so another attempt can run.
Transcription and upload request deadlines are unchanged. Malformed count
responses fail visibly instead of fabricating zero records. Final validation:
1,539 Rust tests passed (16 ignored), including stalled management and
repeat-after-timeout regressions; Clippy workspace/all-targets and formatting
pass.

Soniox documents `total` as a required integer on its [file-count endpoint](https://soniox.com/docs/api-reference/stt/files/get_files_count) and [transcription-count endpoint](https://soniox.com/docs/api-reference/stt/transcriptions/get_transcriptions_count).

## Initial source and post-timeout usage

The lifted navigation filter starts unset, so ordinary Sources mounting derives
the saved active source. An explicit Soniox Cloud override still wins. Stateful
coverage checks both paths. Cleanup now reloads usage after failures/timeouts as
well as success; failed usage refresh clears stale counts and surfaces the read
error, and the button becomes usable again. The full frontend suite passes 715
tests; typecheck, lint, build and independent review pass. Rust is unchanged
from the 1,539-test validated revision.
