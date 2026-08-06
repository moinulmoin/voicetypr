# Plan 045: Privacy-safe PostHog product analytics

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: Plan 031 GlitchTip observability
- **Claimed**: Main, 2026-08-06

## Goal

Add consented, anonymous product analytics through PostHog while keeping GlitchTip as the only error, crash, log, trace, and symbolication system.

## Non-negotiable boundary

- PostHog receives only typed events from a closed Rust API. No frontend SDK, autocapture, session replay, feature flags, identify calls, person profiles, error tracking, or free-form properties.
- Never send audio, transcripts, clipboard contents, prompts, API/license keys, email addresses, paths, hostnames, target apps/windows, hotkey values, provider error strings, or arbitrary logs.
- GlitchTip remains the sole destination for errors, crashes, curated operational logs, sampled traces, and native symbols. No duplicate events.
- Product analytics has its own independently revocable consent and installation ID. New users choose during onboarding. Existing users must acknowledge the new choice before analytics can send.
- Debug builds and release builds without a PostHog project token are inert.

## Deliverables

1. A release-only PostHog Rust client with error tracking disabled, EU ingestion, GeoIP disabled, person profiles disabled, a bounded queue, generation-aware consent revocation, and a final event/property allowlist.
2. Typed events for app start and the recording → transcription → formatting → delivery journey, using only closed enums and bounded numeric buckets.
3. Separate backend consent commands and persistent keys.
4. Separate Crash & error reporting and Usage analytics controls in onboarding and Advanced settings.
5. A one-time consent dialog for existing users; no analytics before Continue.
6. Release workflow wiring for the public PostHog project token.
7. Contract tests, frontend interaction tests, full quality gate, and manual desktop smoke entries.

## Verification

- Focused Rust privacy/consent/event-schema tests.
- Focused frontend consent/onboarding tests.
- `pnpm quality-gate`.
- Independent adversarial privacy and integration review.
- Manual release-build network smoke in `plans/SMOKE.md` before Beta publication.
