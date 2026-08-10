# Plan 047: Polish Provider UX + No-Input Fast Path

**Status:** IN PROGRESS — claimed Main 2026-08-10
**Priority:** P0
**Size:** M
**Depends on:** 044, 046

## Scope

1. Keep Polish provider/model setup directly below the Polish summary when requested; keep per-app rules in Advanced.
2. Split provider setup into **Cloud API** and **Local Agents** tabs with compact horizontal rows, model selection, and supported thinking/effort selection.
3. Run Pi through plain one-shot print mode (`-p`) with tools, sessions, extensions, skills, prompt templates, and context files disabled; surface actionable CLI/provider errors.
4. Restore application toasts to the top center.
5. Skip the speech engine only for capture evidence classified as exact digital-zero no-input; uncertain audio must continue through transcription.
6. Correct the shortcut empty-state layout, cloud-model hierarchy, and AI Polish metric accounting.

## Acceptance

- Polish **Change** reveals provider/model setup next to the Polish controls instead of opening the distant Advanced section.
- Provider setup has Cloud API and Local Agents tabs; each local row exposes install/auth state, refresh, model, and supported thinking/effort controls.
- Pi uses plain `-p` output and reports the real non-zero error instead of a generic JSON-response failure.
- Empty digital-zero recordings return to idle without launching Whisper or Parakeet; uncertain and speech-positive recordings remain unchanged.
- Toasts render at top center.
- Empty shortcut actions align to the right of their descriptions on desktop and stack cleanly on narrow widths.
- Cloud transcription cards present the model as primary and provider as secondary.
- AI Polish analytics distinguish attempts from successful application so attempted use is not reported as zero.
- Focused frontend and backend checks pass, and changed UI paths are visually exercised.
