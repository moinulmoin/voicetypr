# Plan: Direct bug and crash report submission

## Overview

Replace the current user-mediated bug/crash reporting paths with a private submit flow. Voicetypr desktop will collect manual bug reports and React crash reports, include latest app log context and system information, then POST to a `voicetypr-web` API endpoint. The web API will validate, rate-limit, and forward the report privately to Discord using a server-side webhook secret.

The desktop UI should say **Submit**, not "Email" or "Open in GitHub", only after the API endpoint exists and the button truly sends the report.

---

## Scope Boundaries

- Add a private report intake endpoint in `voicetypr-web`.
- Deliver reports to the maintainer via Discord webhook from the web server.
- Update desktop manual Report Bug flow to submit directly.
- Update desktop React crash report flow to submit directly.
- Include latest app log excerpt by default for both manual and crash reports.
- Keep Copy Report fallback for endpoint/network failures.
- Do not send Discord webhooks directly from the desktop app; that would expose the webhook URL.
- Do not create public GitHub issues for reports containing logs/contact/system context.
- Native/process crash detection on next launch is a follow-up unless explicitly pulled into this scope; React error-boundary crashes are in scope.

---

## Context & Research

### Relevant Code and Patterns

Desktop repo:
- `src/components/ReportBugDialog.tsx` owns the manual report dialog.
- `src/components/CrashReportDialog.tsx` currently opens a public GitHub crash issue URL.
- `src/components/ErrorBoundary.tsx` shows `CrashReportDialog` after React error-boundary failures.
- `src/utils/crashReport.ts` gathers crash/manual system context and builds report bodies.
- `src-tauri/src/commands/logs.rs` exposes latest app log context for manual reports.

Web repo (`../voicetypr-web`):
- `app/api/v1/**/route.ts` is the existing public desktop API route pattern.
- `lib/types.ts` contains Zod request schemas.
- `lib/api-utils.ts` contains `createSuccessResponse`, validation/internal-error helpers, and CORS helpers.
- `lib/redis.ts` exposes Upstash Redis via `Redis.fromEnv()`.
- `.env.example` already documents server-side secrets; no current email/support provider exists.

### External References

- Discord webhook execution accepts JSON payloads with `content`, `embeds`, and optional file attachments. Message `content` is limited to 2000 chars; embeds are limited to 10 embeds and 6000 combined embed chars, with field values capped at 1024 chars. Use `allowed_mentions` to prevent user-generated text from pinging people. Source: Context7 `/discord/discord-api-docs` webhook/message docs.

### Institutional Learnings

- `memory://root/memory_summary.md` notes the desktop/web split; current repo inspection confirms server-owned secrets belong in `voicetypr-web`, not the desktop app.

---

## Key Technical Decisions

- **Web API is the trust boundary.** Desktop sends report payloads to `voicetypr-web`; Discord webhook URL remains server-side only.
- **One report endpoint handles manual and crash reports.** Use a discriminated `kind: "manual" | "crash"` payload so validation preserves domain differences without duplicating delivery plumbing.
- **Discord delivery should use summary embed plus attached/truncated text.** Discord content/embed limits are small; send a concise embed and include log/report details as bounded text where supported, or truncate log content server-side before embedding.
- **The UI says Submit only when it really submits.** Until direct API exists, email/copy wording must stay truthful. After API exists, manual and crash CTAs become **Submit**.
- **Failure must be honest and recoverable.** On endpoint failure, show a failed-submit toast/dialog state and keep Copy Report available.

---

## Implementation Units

- U1. **Web report schema and endpoint**

**Goal:** Add a public desktop-facing API endpoint that accepts bug/crash reports without exposing delivery secrets.

**Requirements:** Validate payload shape, size-limit logs/user text, preserve manual vs crash distinctions, return a small success response.

**Dependencies:** Discord webhook URL available as web env.

**Files:**
- Target repo: `../voicetypr-web`
- Modify: `lib/types.ts`
- Create: `app/api/v1/bug-reports/route.ts`
- Modify: `.env.example`
- Test: colocated route/schema tests if existing test setup supports route handlers.

**Approach:**
- Add `bugReportRequestSchema` with `kind`, contact fields, message/crash object, environment object, latestLog object.
- Require `message` for manual reports.
- Require `crash.errorMessage` for crash reports.
- Cap strings before delivery: message, stack, component stack, log excerpt, file name, model/version fields.
- Return CORS-wrapped API responses matching existing route style.

**Patterns to follow:**
- `app/api/v1/trial/check/route.ts`
- `lib/types.ts`
- `lib/api-utils.ts`

**Test scenarios:**
- Happy path: valid manual payload returns success and attempts Discord delivery.
- Happy path: valid crash payload returns success and attempts Discord delivery.
- Edge case: oversized log/message is rejected or bounded before Discord delivery.
- Error path: invalid manual report without message returns validation error.
- Error path: missing Discord webhook env returns internal error without leaking config.

**Verification:**
- Web typecheck/lint/tests for touched files pass.

---

- U2. **Web rate limiting and Discord delivery**

**Goal:** Prevent endpoint abuse and privately notify the maintainer.

**Requirements:** Rate-limit public POSTs; send report to Discord without allowing user-generated mentions.

**Dependencies:** U1 endpoint shape.

**Files:**
- Target repo: `../voicetypr-web`
- Modify/Create: endpoint helper in `app/api/v1/bug-reports/route.ts` or a small `lib/bug-reports.ts` if reuse is justified.
- Modify: `.env.example`

**Approach:**
- Rate-limit by IP and device hash/device ID when present using Upstash Redis.
- Use atomic Redis increment/expiry or a small fixed-window key.
- Send Discord webhook with `allowed_mentions: { parse: [] }`.
- Use `wait=true` when calling Discord so the endpoint knows delivery actually succeeded.
- Format Discord report with clear title, kind, app version, OS, model, contact, timestamp, and bounded detail/log sections.

**Patterns to follow:**
- `lib/redis.ts`
- Existing webhook dedupe/rate-adjacent Redis usage discovered under `app/api/webhooks/polar/route.ts` and server actions.

**Test scenarios:**
- Happy path: report under limit delivers to Discord.
- Edge case: user message containing `@everyone` does not create mentions.
- Error path: rate-limited caller receives 429.
- Error path: Discord non-2xx returns a truthful failure to desktop.

**Verification:**
- Tests or focused mocked fetch validation prove Discord payload shape and rate-limit behavior.

---

- U3. **Desktop shared report submit client**

**Goal:** Add a desktop utility that POSTs manual/crash report payloads to the web endpoint.

**Requirements:** No desktop secrets; bounded request body; structured success/failure result; copy fallback remains possible.

**Dependencies:** U1 endpoint contract.

**Files:**
- Target repo: current desktop repo
- Modify: `src/utils/crashReport.ts` or create `src/utils/reportSubmission.ts` if separation is clearer.
- Test: `src/utils/crashReport.test.ts` and/or new submit utility test.

**Approach:**
- Add a single submit function used by both manual and crash flows.
- Convert existing manual/crash data into the endpoint contract.
- Treat non-2xx or `{ success: false }` as failure.
- Do not rely on inconsistent API error fields; use status plus `message` when present.

**Patterns to follow:**
- Existing `gatherManualReportData` / `gatherCrashReportData` in `src/utils/crashReport.ts`.
- Existing frontend tests mocking Tauri APIs and network boundaries.

**Test scenarios:**
- Happy path: valid manual data POSTs expected payload and resolves success.
- Happy path: crash data POSTs expected payload and resolves success.
- Error path: failed fetch returns a failure result used by UI.
- Edge case: no log content still submits with status note.

**Verification:**
- Focused Vitest utility tests pass.

---

- U4. **Desktop manual Report Bug submit UX**

**Goal:** Change manual Report Bug from email compose to true direct submit.

**Requirements:** CTA label is **Submit**; on click report goes to the API; include latest log; keep Copy Report fallback.

**Dependencies:** U3 submit client.

**Files:**
- Modify: `src/components/ReportBugDialog.tsx`
- Test: `src/components/ReportBugDialog.test.tsx`

**Approach:**
- Replace email-opening path with direct submit.
- Keep required message validation.
- Show submitting state and disable duplicate submits.
- On success, toast success and close dialog.
- On failure, keep dialog open and show Copy Report fallback guidance.

**Test scenarios:**
- Happy path: clicking Submit sends report and closes dialog.
- Validation: blank message does not submit.
- Error path: submit failure shows error and keeps Copy Report available.
- Edge case: close during async gather/submit does not show stale success.

**Verification:**
- Focused component tests pass.

---

- U5. **Desktop crash report submit UX**

**Goal:** Send React crash reports privately through the same endpoint instead of opening GitHub.

**Requirements:** CTA label is **Submit**; include crash error/stack/system info/latest log; keep Copy Details/Copy Report fallback.

**Dependencies:** U3 submit client.

**Files:**
- Modify: `src/components/CrashReportDialog.tsx`
- Modify: `src/utils/crashReport.ts`
- Test: add/update crash report dialog tests.

**Approach:**
- Extend crash report data gathering to include latest log attachment, or compose crash endpoint payload by combining `gatherCrashReportData` with latest log command.
- Replace `generateGitHubIssueUrl` usage in the dialog.
- Keep GitHub issue URL generator only if still used elsewhere; otherwise remove it with tests.
- Button text becomes **Submit**.
- On successful submit, close or show sent state; on failure, keep copy fallback.

**Test scenarios:**
- Happy path: React crash report submits crash payload with latest log.
- Error path: submit failure does not dismiss the dialog and surfaces copy fallback.
- Edge case: log unavailable still submits crash report with no-log note.
- Regression: Try Again still resets error boundary.

**Verification:**
- Focused crash component tests pass.

---

- U6. **Optional next-launch native crash prompt**

**Goal:** Handle true process/native crashes that cannot show a dialog at crash time.

**Requirements:** Detect prior abnormal shutdown and ask on next launch whether to submit diagnostics.

**Dependencies:** U1-U3 report submission path.

**Files:**
- Target repo: current desktop repo
- Likely backend state/logging files after deeper investigation.

**Approach:**
- Mark clean startup/shutdown state in a small app-state flag.
- If previous session ended uncleanly, show a startup prompt: “Voicetypr closed unexpectedly. Submit crash report?”
- Include latest log and system info; do not claim exact crash cause unless captured.

**Test scenarios:**
- Deferred until scoped.

**Verification:**
- Deferred; not part of the first direct-submit PR unless explicitly requested.

---

## System-Wide Impact

- **Interaction graph:** Desktop dialog/error boundary -> shared submit client -> `voicetypr-web` API -> Redis rate limiter -> Discord webhook.
- **Error propagation:** Web validation/429/Discord failure must return truthful status; desktop must display failure and keep copy fallback.
- **State lifecycle risks:** Duplicate submits from double-clicks; stale async completion after modal close; Discord/webhook outage; rate-limit false positives.
- **API surface parity:** Manual and crash reports share delivery but keep distinct payload fields.
- **Security/privacy boundary:** Desktop contains only public endpoint URL; Discord secret remains in web env.
- **Operational dependency:** Discord webhook env must be configured before switching desktop CTA to Submit in production.

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Public endpoint spam | Redis rate limit by IP and device identifier; reject oversized bodies. |
| Discord secret leakage | Store webhook URL only in `voicetypr-web` env; never return it or ship it in desktop. |
| Discord message truncation | Use concise embed fields and bounded details/log content; optionally attach text if implementation supports multipart cleanly. |
| User-generated mentions | Send `allowed_mentions: { parse: [] }` and avoid raw content pings. |
| Endpoint outage blocks reports | Desktop keeps Copy Report fallback and shows truthful failure. |
| Crash report loses native crashes | React error-boundary crashes are covered first; true process crash prompt is next-launch follow-up. |

---

## Rollout Plan

1. Build and merge `voicetypr-web` endpoint with Discord env configured.
2. Verify endpoint manually with a synthetic report in preview/production and confirm Discord delivery.
3. Update desktop manual Report Bug and Crash Report dialogs to use **Submit**.
4. Keep Copy Report fallback in desktop.
5. After direct submit is stable, decide whether next-launch native crash detection is worth adding.

---

## Sources & References

- Current desktop report PR: `src/components/ReportBugDialog.tsx`, `src/components/CrashReportDialog.tsx`, `src/utils/crashReport.ts`.
- Web API patterns: `../voicetypr-web/app/api/v1/trial/check/route.ts`, `../voicetypr-web/lib/types.ts`, `../voicetypr-web/lib/api-utils.ts`, `../voicetypr-web/lib/redis.ts`.
- Memory artifact: `memory://root/memory_summary.md` for desktop/web split; verified against current repo and sibling web repo files.
- Discord docs via Context7 `/discord/discord-api-docs`: webhook execute endpoint, content/embed limits, `allowed_mentions`.
