# Plan 026: Wave 5 (F5) — actionable errors + feedback

> **STATUS: IN PROGRESS — claimed Main 2026-06-16.** Wave 5 of `plans/021-v2-feature-track.md`.

## Status
- **Priority**: P2 · **Effort**: S-M · **Risk**: LOW
- **Depends on**: none
- **Parent**: plans/021 (Wave 5)

## Scope
Make recording/permission failures tell the user **what happened AND how to fix it**, surfaced in the existing pill **feedback overlay** (the user's explicit focus). Add a structured `suggestion` (remediation) to the pill toast payload + render it; attach concise remediation to the key user-facing failures. Lightly surface the already-defined `ErrorEventPayload.suggestion` in the main-window Sonner toasts. **Out of scope** (deferred): clickable action buttons in the overlay — it is `pointer-events-none` / a non-focused panel (oracle's L item); W5 is remediation TEXT.

## Ground truth
- Pill toast: `pill_toast`/`pill_toast_with_variant`/`pill_toast_persistent` → `emit_pill_toast` (commands/audio.rs:158) → `app.emit("toast", PillToastEventPayload { id, message, duration_ms, action?, variant?, persistent })`. No remediation field.
- `FeedbackToast.tsx`: `PillToastPayload { id, message, duration_ms, action?, variant?, persistent? }`; severity inferred from message words or `variant`; renders a single message.
- `AppContainer.tsx`: `ErrorEventPayload` already declares `actions/details/hotkey/suggestion` but handlers ignore them; domain errors (remote-server-error, license-required, no-models-error, tray-action-error, parakeet-unavailable) shown via Sonner.

## Approach
**Backend** (`commands/audio.rs`, `commands/text.rs`):
- Add `suggestion: Option<String>` to `PillToastEventPayload` (serde skip-if-None). Add a `suggestion: Option<&str>` param to `emit_pill_toast`; existing wrappers pass `None`; add `pill_toast_with_suggestion(app, message, suggestion, duration_ms, variant?)`.
- Attach a concise remediation `suggestion` at the key user-facing failure sites (keep the `message` short, move/duplicate the "how to fix" into `suggestion`): mic permission denied (→ "Enable Microphone access in System Settings ▸ Privacy & Security"), no microphone found (→ "Connect a microphone and try again"), microphone busy (→ "Close other apps using the mic"), recording interrupted / "No audio captured" (→ "Try recording again"), no speech detected (existing "speak closer" → suggestion), auto-paste permission (text.rs: → "Grant Accessibility permission to enable auto-paste"). Use the project's exact existing wording where present; just make it structured.

**Frontend** (`src/components/FeedbackToast.tsx`, `src/components/AppContainer.tsx`):
- `FeedbackToast`: add `suggestion?: string` to `PillToastPayload` + `ActiveToast`; render it as a second, smaller muted line under the message (respect the existing severity treatment + 2-line clamp → allow the suggestion line).
- `AppContainer`: where domain errors show Sonner toasts, pass `payload.suggestion` as the Sonner `description` when present (and `payload.actions?.[0]` as a Sonner action button only if trivially available — else skip). Minimal, additive.

## Tests
- Backend: `emit_pill_toast`/payload includes `suggestion` when provided, omits when None (serde).
- Frontend: `FeedbackToast` renders the suggestion line when present and not when absent; severity still derived correctly.

## Acceptance
- Pill feedback overlay shows a remediation line for the key failures; `suggestion` flows backend→payload→render; main-window domain errors show their `suggestion` as the Sonner description when present; no interactive-button requirement; all gates green.

## Smoke (append to SMOKE.md)
- **026-S1**: trigger mic-permission-denied (deny mic) → pill overlay shows the failure + a "how to fix" line; trigger auto-paste without Accessibility → overlay shows the paste remediation. A normal success toast is unchanged.

## STOP
- If adding the suggestion line breaks the toast window sizing/clamp badly, keep the line but cap length; do not add interactive buttons (deferred).
