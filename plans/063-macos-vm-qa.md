# Plan 063 — Packaged macOS VM QA

Status: IN PROGRESS — claimed Codex 2026-09-06.
Baseline: PR #140, `37eea0229e3b2c391315c4235068af1212d80f36`.

## Scope

Run the packaged macOS ARM app in an isolated Tart guest, preserving the host
desktop and user profile. Exercise onboarding, recording/insertion, recovery,
settings and the primary pages. Fix only reproduced defects in the release scope.
Windows ARM builds and broad redesigns are deferred. No scheduled polling.

## Environment and limits

VM workspace: `/Volumes/1tb-drive/qa/voicetypr-vm`.
Guest-only CuaDriver/VNC control, virtual audio input, synthetic speech and
disposable settings. Candidate is an ad hoc signed release build, not a notarized
public installer. VM results do not establish physical microphone/GPU parity or
Windows runtime correctness. Existing unperformed hardware checks remain open.

## Reproduced finding

Fresh onboarding: select a local model, grant permissions, save the hotkey,
turn both reporting options off, then finish setup. The app immediately opens
another privacy dialog with both switches on. Determine whether the stale state
is visual or persisted, fix the handoff, and prove off choices persist after
relaunch. Evidence is in `evidence/01-onboarding-consent-off.png` and
`evidence/02-duplicate-consent-after-onboarding.png` under the VM workspace.

## Acceptance

Record exact artifact identity and observed runtime outcomes. Test confirmed
fixes through the real flow after repackaging. Preserve consent and license
fixtures; no customer data resets, provider deletion, support submissions or
release publication as a side effect of QA. Keep release smoke limitations
explicit in the final report.

## Confirmed fixes — awaiting rebuilt VM verification

- Onboarding persisted OFF/OFF correctly, but published completion before the
  writes finished, so the main app opened a stale default-on dialog. Persist
  consent first and publish onboarding completion only after settings save.
  Failed saves leave the same step recoverable with the user's selections.
- Fresh startup without Accessibility failed to start the shortcut engine.
  Later successful permission polling did not emit the event that retries it.
  Authorized checks now notify the existing retry listener; concurrent starts
  remain idempotent. The old build started its engine only after relaunch;
  the same guest VNC shortcut then recorded and inserted successfully.
- Empty Overview displayed “Busiest · 1”. Display the actual zero maximum and
  keep a separate safe denominator for chart geometry.

Integrated local validation: 727 frontend tests, typecheck, lint and production
build pass; 1,548 Rust workspace tests pass with 16 ignored, and Clippy across
all targets passes with warnings denied. Independent review cleared the final
patch after correcting settings-save publication and zero chart geometry.

Old-candidate VM evidence: startup, permission gating, Base English download and
checksum verification, selection, consent persistence, and post-relaunch
record→transcribe→insert completed. Clipboard sentinel was restored. The first
diagnostic run contained prolonged silence and extra recognized words; a short
controlled run also had a word-level recognition mismatch. Do not treat either
as exact transcription-accuracy proof or infer a recorder defect without audio
comparison. Both paths returned to Idle and saved History entries.
