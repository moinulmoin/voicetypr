# Plan 048: Expanded Local Agent CLIs

**Status:** IN PROGRESS — claimed Main 2026-08-10
**Priority:** P1
**Size:** L
**Depends on:** 044, 047

## Scope

Add best-effort Polish adapters for Codex, Droid, Amp, Grok, OpenCode, Cline, Kilo Code, and Hermes. Preserve the existing cold-spawn timeout, bounded output, temporary working directory, process-group cleanup, raw-transcript fallback, and sanitized user-facing failures. Apply each CLI's strongest available controls for tools, project context, plugins, memory, and session persistence without claiming identical guarantees across third-party CLIs.

## Acceptance

- All eight providers appear under Local Agents and can be probed independently.
- Each adapter uses the installed CLI's documented non-interactive mode and provider-specific output parser.
- Model selection uses CLI default unless a compatible explicit model is selected.
- Reasoning controls appear only where the adapter has a verified per-run setting.
- Missing, incompatible, unauthenticated, timed-out, and malformed CLI runs preserve the raw transcript and surface actionable errors.
- Focused Rust/frontend checks pass; installed adapters receive exact-command smoke coverage.
