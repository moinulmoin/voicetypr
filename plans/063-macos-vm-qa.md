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
