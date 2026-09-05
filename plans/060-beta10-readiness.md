# Plan 060 — Beta10 release remediation (silent-failures work migrated from PR 047)

Renumbered from PR 140's `plans/047-silent-failures-soniox-alerts-diagnostics.md`
to free 047 for main's landed `polish-provider-ux-audio-fast-path.md` (via #136).
This file is now the beta10 remediation umbrella: the migrated silent-failures
plan below, plus the six concurrent beta10 remediation slices reviewed against
main `4f9e497d` + PR 140 `eace981b`. Upstream plans 044–059 are untouched.

Baseline: working tree 2026-08-20 (post v2.0.5). Trigger: paid-user bug report
(Windows, v2.0.5, 2026-08-19) — Soniox transcription permanently failing with
HTTP 429 `limit_exceeded` ("Total file count limit has been exceeded") and local
Whisper large-v3-turbo failing on CPU-only fallback; user saw only
"Transcription failed" / "Could not reach the transcription service", and
GlitchTip raised nothing because handled failures are curated *logs*, never
issues.

Research (2026-08-20, source-verified):

- Soniox: `DELETE /v1/transcriptions/{id}` cascades to its file; transcriptions
  have their OWN org cap (2,000 total / 100 pending) in addition to files
  (1,000 / 10 GB); `GET /v1/files/count` + `GET /v1/transcriptions/count`
  exist; file-management RPM limit exists (numeric value undocumented); no
  non-storing REST transcribe path (WebSocket is a separate architecture).
- Seam map: verified call sites listed per work item below.
- Dependency audit rider: `env_logger` (Cargo), `@radix-ui/react-collapsible`,
  `@radix-ui/react-slot`, `@tauri-apps/plugin-updater` (JS),
  `tauri-plugin-macos-permissions-api` (JS), `conventional-changelog-cli` are
  dead (verified; knip false positives on 8/12 flags were excluded).

## A — Soniox storage lifecycle (P0, customer-blocked)

1. After terminal poll status in both flows (`soniox.rs` typed + diarized),
   fire-and-forget `DELETE {BASE}/transcriptions/{id}` (best-effort; ignore
   404/429/network; log at warn). Covers all exits: completed, error, timeout.
2. Parse `error_type=limit_exceeded` in `common.rs::log_http_body` → new
   `SttError::LimitExceeded` (non-transient: excluded from `is_transient` so
   `with_retry` stops retrying terminal limit 429s) with honest message:
   storage/quota limit reached — clean up stored files (see 3) or raise cap in
   Soniox console. No raw provider bodies in user-facing text (019 bar).
3. Map `LimitExceeded` through `transcription/error.rs` to a distinct
   `TranscriptionErrorCode` + actionable `user_message_for_code` text.
4. Settings UI (Soniox selected + key validated): stored-files count +
   transcriptions count (`GET /files/count`, `/transcriptions/count`) and a
   "Clean up stored files" action → backend command listing + deleting
   transcription records (paced sequential deletes, back off on 429; progress
   state; skip while any transcription is processing → 409).
5. Tests: wiremock limit-429 → LimitExceeded (no retry); delete-called-on-
   terminal-status (typed + diarized); error-mapping contract test update
   (`error.rs` tests pin RateLimited→TransportFailed today).

## B — Telemetry: alertable failure events, funnel removal (P0)

Replaces 031's stage "curated logs + sampled traces" (031 owner approved this
pivot 2026-08-20): delete `log_transcription`/`TranscriptionPhase` funnel,
`TelemetryTransaction`/`TelemetrySpan`, sentry `logs` cargo feature,
`before_send_log`/`scrub_log` + their tests. Keep: panics, scrubbed frontend
error events, consent gates.

Add `capture_failure_event(message, tags)` (fixed strings only):

- `flow.transcription.failed` — sink `audio.rs:5416-5426` decode failure (incl.
  timeout via executor) with tags: engine, model, backend, failure_class,
  duration_ms.
- `flow.paste.failed` — `audio.rs:5865-5875` / `5916-5921` (engine captured
  pre-spawn at `5237-5238`).
- `flow.model_load.failed` — `whisper/cache.rs:99-102`.
- `flow.remote.failed` — `audio.rs:6023-6055`.
- backend tag: global set at whisper init (`transcriber.rs:62-168`,
  `gpu_sidecar.rs` effective_backend) — `ActiveEngineSelection::Whisper`
  carries no backend field today.

## C — Bug-report system specs swallow (P1)

`crashReport.ts:21-23` `.catch(() => undefined)` → surface "System specs
collection failed: <reason>" row + log. Verified NOT a missing-registration
issue (registered 2026-06-13, present in v2.0.5) — on the reporter's machine
the invoke rejected; today's code erases the evidence.

## D — DEBUG ring buffer (P1)

Always-on in-memory ring (~1000 entries, capped bytes) capturing `log::debug!`
in release (tauri-plugin-log filters file sink at Info) via a custom
Debug-level target if tauri-plugin-log v2 supports per-target levels, else a
wrapper logger at init. Ring dump redacted through `redact_log_content`
(`commands/logs.rs:275`) and auto-attached to every bug report. No user-facing
toggle. Also: fix rotated `.log.N` files never being cleaned
(`clear_old_logs`/`find_newest_log` only match bare names).

## E — Dependency cleanup rider (P2)

Remove the six dead deps listed above (manifest removals only; transitive
availability unchanged where relevant). Out of scope: cpal 0.16→0.18 upgrade
(realtime RT-priority callbacks) — separate plan + device smoke; whisper-rs
already latest (0.16.0).

## Beta10 remediation slices (concurrent; Main gates after barrier)

Integration branch `fix/060-beta10-readiness` (worktree
`/tmp/voicetypr-beta10-integration`) = PR 140 `eace981b` merged with main
`4f9e497d`. Workers own disjoint files on their own branches; IntegrationOwner
cherry-picks only on Main's instruction. No validation mid-flight (Main runs the
consolidated gate); no remote pushes; user WIP (`agent/`, `videos/`,
`plans/032-*.md`) untouched.

| Slice | Owner / base | Scope (review-proven defects) | Acceptance |
|-------|--------------|-------------------------------|------------|
| 060.1 recorder/audio integration | IntegrationOwner, main after merge; `src-tauri/src/audio/recorder.rs`, `src-tauri/src/commands/audio.rs` | Final WAV drain branch bypasses speech-evidence observe (dropped/unwritten audio must not fabricate evidence; no second scan, no RT-callback allocations); no-speech gate must not delete recordings with only uncertain absence evidence; `stop_recording()` error must restore media via `MEDIA_CONTROLLER.resume_if_we_paused` before propagation (ESC/custom + normal-stop paths); Windows sidecar backend tag must record the actual attempt (pre-await) and CPU on fallback, attempt-local, no global stale attribution | Focused regressions per transition in owned test files; commit `fix: preserve final speech and restore media on cancellation errors` |
| 060.2 Soniox | SonioxFixes, PR HEAD `eace981b`; soniox.rs seam + its tests | Remaining Soniox silent-failure defects from the beta10 review (storage lifecycle / alertable-failure seam) | Focused fixes + regressions on their branch; validated at Main's consolidated gate |
| 060.3 Report diagnostics | DiagnosticsFixes, PR HEAD `eace981b`; ReportProblemSection + crash-report consumers | Remaining report/diagnostics defects from the beta10 review (specs swallow seam, DEBUG ring attachment) preserving extracted components | Focused fixes + regressions on their branch |
| 060.4 Release tooling | ReleaseToolingFixes, main `4f9e497d`; workflow + packaging files | Release-workflow/tooling defects from the beta10 review (42 fast-path, pins, caches) | Focused fixes + regressions on their branch |
| 060.5 Polish workflow | PolishFixes, main `4f9e497d`; polish seam files | Polish-workflow defects from the beta10 review; `classify_polish_outcome`, model selection, prefetch preserved | Focused fixes + regressions on their branch |
| 060.6 Frontend state | FrontendFixes, main `4f9e497d`; extracted hooks/state files | Frontend-state defects from the beta10 review; extracted hooks, PostHog journeys/consent preserved | Focused fixes + regressions on their branch |

Smoke for 060.1 additions rides the renumbered section below (`060-S1..S5`) and
`SMOKE.md`; hardware smoke (Windows sidecar tag, macOS media restore) batches
after the premerge gate per Main.

## Verification

`cargo fmt --check`, `cargo clippy -- -D warnings`, `cd src-tauri && cargo
test`, `pnpm typecheck`, `pnpm lint`, `pnpm exec vitest run`, `pnpm
quality-gate`.

## NEEDS-SMOKE (after code freeze)

- Real Soniox key: dictation → transcription succeeds AND stored counts stay
  flat (delete fired); limit-429 shows honest message; cleanup button drains
  counts. macOS + Windows.
- GlitchTip: failure event arrives as issue (Discord alert fires) with correct
  tags; no logs/traces after removal; opt-out still fully inert.
- Bug report from release build contains System specs (or visible failure
  reason) and redacted DEBUG ring section.
