# 034 — Compact report problem page

Status: IN PROGRESS — claimed Main 2026-07-23

## Problem

The sidebar opens a modal with name, email, and a single message field. The modal is cramped and asks for contact fields that are not required for triage. Voicetypr already gathers app version, OS, architecture, model, anonymous device ID, system specifications, and recent redacted logs.

## Decision

Replace the sidebar modal with a normal `Report a problem` page. Keep one required `Describe the issue` field. Gather diagnostics automatically. Direct submission remains primary; the prepared-report copy action appears only if submission fails.

The crash-specific `CrashReportDialog` remains unchanged.

## Acceptance

- Sidebar `Report a problem` opens a dedicated page and shows active navigation state.
- The page contains one required user-input field: `Describe the issue`.
- Submission includes the existing automatically gathered diagnostics and recent redacted log.
- Successful submission clears the form and confirms success.
- Failed submission preserves the prepared report and offers `Copy report`.
- Existing report validation, stale-request protection, and copy fallback remain covered.
- The obsolete manual-report modal and its sidebar state are removed.
