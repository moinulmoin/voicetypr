# Plan: Report Bug dialog with latest log context

## Overview
Add a bottom-left Report Bug action that opens an in-app dialog. The dialog collects an optional name, optional email, and required message, then builds a support report containing clear system information and the latest app log excerpt. Because this repo currently has no private bug-report API endpoint and the existing GitHub issue path is public, this PR uses an email addressed to Voicetypr Support plus a copy-report fallback; a direct-to-maintainer submit flow should be a follow-up web/API PR.

---

## Scope Boundaries
- Add a manual bug-report flow from the main sidebar.
- Include necessary system info and latest app log context by default, with clear copy explaining what is included.
- Sanitize and size-bound the latest log before exposing it to the frontend.
- Preserve the existing crash-report GitHub flow unless a small shared utility extraction is needed.
- Do not add a public GitHub issue flow for manual reports, because reports should come privately to Voicetypr support.
- Do not add a server endpoint in this PR; no private endpoint exists in the desktop repo or sibling web API routes.

---

## Context & Research
### Relevant Code and Patterns
- `src/components/Sidebar.tsx` owns bottom-left navigation/actions and can host local dialog state.
- `src/components/CrashReportDialog.tsx` shows existing shadcn dialog, report, copy, and toast patterns.
- `src/utils/crashReport.ts` gathers app version, OS, architecture, current model, timestamp, and device ID for crash reports.
- `src/components/sections/HelpSection.tsx` has current email support and diagnostics patterns.
- `src-tauri/src/commands/logs.rs` has log directory/open commands but no latest-log read command.
- `src-tauri/src/lib.rs` configures daily log files named `voicetypr-YYYY-MM-DD` and registers Tauri commands.

### Research Findings
- Existing manual support is email/manual-log only; existing crash reporting opens public GitHub issues.
- No private `bug-report`, `support`, or `feedback` API route exists in `../voicetypr-web/app/api/**/route.ts`.
- Logs are useful diagnostics and are included by default; the report copy should be direct about that instead of making the log attachment sound exceptional or scary.

### Institutional Learnings
- `memory://root/memory_summary.md` notes the desktop/web split; current repo inspection confirmed no web bug-report endpoint currently exists.

---

## Key Technical Decisions
- **Manual Report Bug is a dialog action, not a tab.** It belongs in the bottom-left group and should not change `activeSection`.
- **Email to support is the submission path for this PR.** It avoids public GitHub exposure and avoids inventing an unavailable backend. The dialog labels this as emailing Voicetypr Support so the user understands where the report goes.
- **Backend returns a bounded latest-log excerpt, not an arbitrary file read.** The command should select the newest `voicetypr-*.log`, bound bytes/lines, redact defensive common secrets/paths, and return metadata so the UI can explain what is included.
- **Frontend report builder centralizes system info.** Reuse the crash-report style data gathering without requiring an `Error` object.

---

## Implementation Units
- U1. **Backend latest-log attachment command**

**Goal:** Expose a safe, frontend-callable command for report diagnostics.

**Requirements:** latest log included by default; privacy-aware bounds/redaction.

**Dependencies:** None.

**Files:**
- Modify: `src-tauri/src/commands/logs.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/tests/logging_performance_tests.rs` or a new focused logs test under `src-tauri/src/tests/`

**Approach:**
- Add a serializable struct for log attachment metadata/content.
- Select newest matching `voicetypr-*.log` file in `app_log_dir`.
- Read only a bounded tail of the file.
- Redact common API-key/token/license/email-like values and user-home paths where practical.
- Return a clear empty/no-log state if no log exists or read fails.

**Patterns to follow:**
- `src-tauri/src/commands/logs.rs` for app log dir access.
- `src-tauri/src/lib.rs` command registration.

**Test scenarios:**
- Happy path: newest matching log is selected and content returned with metadata.
- Edge case: no log dir or no matching file returns no content without error.
- Privacy/error path: secret-like values and home paths are redacted.
- Edge case: oversized log is truncated to the configured tail bound.

**Verification:**
- Focused Rust tests pass.

---

- U2. **Frontend report data utility**

**Goal:** Build a manual bug-report payload with system info and latest log data.

**Requirements:** optional name/email, required message, system info, latest log included, clear report body formatting.

**Dependencies:** U1 command shape.

**Files:**
- Modify: `src/utils/crashReport.ts` or create a small sibling utility if reuse becomes clearer.
- Test: `src/utils/*report*.test.ts` if practical.

**Approach:**
- Add manual report types and gatherer that calls app/OS APIs and the new latest-log command.
- Generate an email body/report body with user fields, environment table, and a fenced latest-log section including truncation/redaction notice.
- Keep crash report behavior intact.

**Patterns to follow:**
- Existing `gatherCrashReportData` and `generateGitHubIssueUrl` formatting.
- `HelpSection` diagnostics copy.

**Test scenarios:**
- Happy path: report body includes message, optional contact fields, system info, and log metadata/content.
- Edge case: missing name/email are omitted or marked not provided without breaking formatting.
- Error path: failed latest-log command still generates a usable report with a no-log note.

**Verification:**
- Focused frontend utility/component tests pass.

---

- U3. **Report Bug dialog and sidebar action**

**Goal:** Add the bottom-left action and modal UX.

**Requirements:** name optional, email optional, message required; clear copy that latest log and system info are included; submit via email addressed to support; copy fallback.

**Dependencies:** U2 utility.

**Files:**
- Modify: `src/components/Sidebar.tsx`
- Create or modify: `src/components/ReportBugDialog.tsx`
- Test: `src/components/ReportBugDialog.test.tsx` and/or sidebar test.

**Approach:**
- Add a `Report Bug` bottom action with `Bug` icon that opens a local dialog.
- Dialog validates required message, gathers report data on submit/copy, opens an email compose URL to `support@voicetypr.com`, and offers Copy Report.
- Include concise copy: “Voicetypr will include your system info and the latest app log excerpt, then open an email addressed to Voicetypr Support so you can send it directly to us.”
- Avoid marking a nav section active.

**Patterns to follow:**
- `CrashReportDialog.tsx` dialog/toast/buttons.
- `HelpSection.tsx` Gmail/default email flow.
- `Sidebar.tsx` bottom group styling.

**Test scenarios:**
- Happy path: clicking Report Bug opens dialog.
- Validation: submit/copy with blank message shows required-message feedback and does not open email/copy report.
- Happy path: valid message opens an email draft containing report body.
- Edge case: log unavailable still permits report.

**Verification:**
- Focused Vitest tests pass.

---

## System-Wide Impact
- **Interaction graph:** Sidebar action -> dialog -> frontend report utility -> Tauri latest-log command -> email compose/copy.
- **Error propagation:** Backend log read failures return no-log/empty state where possible; frontend shows a usable report rather than failing the whole flow.
- **State lifecycle risks:** No persistent state changes. No automatic external post without user action.
- **API surface parity:** Existing crash report path remains unchanged; Help email support remains available.
- **Integration coverage:** Component tests should exercise dialog validation and email/copy behavior; Rust tests should exercise log selection/redaction/truncation.

---

## Risks & Mitigations
| Risk | Mitigation |
|------|------------|
| Report delivery depends on user's mail client | Keep Copy Report fallback in this PR; follow up with a private web API endpoint for direct submission. |
| Email URL body may be too long | Bound log excerpt and provide Copy Report fallback. |
| Public GitHub issue leaks contact/log data | Do not use GitHub issue URL for manual bug reports. |
| Backend command can become arbitrary file read | Only read newest matching file from Tauri `app_log_dir`; no frontend-supplied path. |

---

## Direct-to-Maintainer Follow-up Plan
- Add a `voicetypr-web` server-owned endpoint, for example `app/api/v1/bug-reports/route.ts`, rather than putting any provider secret in the desktop app.
- Validate a bounded payload: message, optional name/email, app version, OS/architecture/model, device hash or device ID, latest log excerpt, and timestamp.
- Add abuse controls on the web side using the existing Redis pattern: rate-limit by IP and device identifier, reject oversized bodies before email delivery, and log minimal failure metadata.
- Deliver privately with a server-side email provider secret, e.g. `BUG_REPORT_TO_EMAIL` plus Resend/Postmark/SMTP credentials stored only in web deployment env.
- Desktop follow-up: replace the email button with `Submit Report`, POST to the endpoint, show truthful success/failure, and keep Copy Report/email fallback for endpoint outages.

## Documentation / Operational Notes
- This PR should mention that reports include the latest log excerpt and are addressed to Voicetypr Support.
