# 041 — Paid-license validation resilience

**Status: IN PROGRESS — claimed Main 2026-07-24**

## Problem

A paid user successfully activates Voicetypr, then later sees `Trial expired` and must enter the same license key again. A transient network or server failure must not erase or downgrade a paid entitlement, and must never be presented as a definitive trial/license expiration.

## Locked behavior

1. Only a definitive entitlement response from the licensing service may mark a paid license invalid or expired.
2. Transport errors, timeouts, and server-unavailable responses keep the last verified paid entitlement through its existing offline-grace contract.
3. Validation uses bounded retries and records consecutive scheduled verification failures without retrying indefinitely.
4. Repeated transient failures escalate to a verification warning, not `Trial expired`; the UI offers an explicit revalidation action.
5. Activation and validation never delete a stored paid key solely because the service was unreachable.

## Acceptance

- A paid cached license remains usable through transient validation failures.
- A fresh paid activation survives restart and a simulated unavailable licensing service.
- Three consecutive scheduled verification failures produce a truthful verification warning while preserving entitlement.
- A definitive invalid/expired server response still revokes paid status immediately.
- Focused Rust and frontend behavioral tests cover the state transitions.
- No license key, device identifier, or raw server response is added to logs or telemetry.
