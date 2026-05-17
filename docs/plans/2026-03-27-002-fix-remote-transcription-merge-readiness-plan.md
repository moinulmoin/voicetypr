---
title: "fix: Make Remote Transcription Branch Merge-Ready"
type: fix
status: completed
date: 2026-03-27
---

# fix: Make Remote Transcription Branch Merge-Ready

## Enhancement Summary

**Deepened on:** 2026-03-27  
**Sections enhanced:** 9  
**Research contributors used:** `RepoResearch`, `LearningsResearch`, `ArchitectureReview`, `SecurityReview`, `TestingReview`, `OracleStateMachine`, `DesignerOnboardingUX`, `LibrarianFrameworkDocs`  
**Matched skills applied:** `systematic-debugging`, `test-driven-development`, `frontend-design`, `document-review`

### Key Improvements
1. Reframed the license issue as a **split source-of-truth bug**, not just a permissive branch.
2. Strengthened startup/restore tasks to re-read **full canonical remote settings**, not only `active_id`.
3. Expanded onboarding/readiness work so it fixes **recording-path truth**, not just `AppContainer` visibility.
4. Strengthened the test strategy with deterministic timing seams, auth-failed remote cases, and platform-matrix assertions.
5. Tightened the plan so startup retry cleanup clears the **actual persisted startup gate**, not just one flag.

### New Considerations Discovered
- `AppState.license_cache` is consulted before recording, but current code clears it more often than it seeds it. Activation/restore transitions need cache warming too.
- Startup auto-start currently snapshots more than `active_id`; stale `port` and `password` can also leak through the delayed task.
- An offline or auth-failed remote can currently affect both onboarding and recording-path readiness because selection state is over-trusted.
- Soniox should be exposed in re-transcription as a **cloud** source, not silently folded into a generic local-model bucket.

### Section Manifest
- **Overview / Problem Statement** — validate that the branch is close but not mergeable yet, and explain why.
- **Local Research Summary** — ground the plan in real code boundaries and local institutional artifacts.
- **Key Decisions** — lock Soniox scope, platform support truth, and offline-remote behavior.
- **Technical Approach / Architecture** — align enforcement boundaries and shared invariants.
- **Implementation Phases** — deepen each task with state-machine, UX, and verification details.
- **System-Wide Impact** — trace where stale state leaks across startup, tray, frontend, and recording paths.
- **Acceptance / Verification** — make tests concrete, deterministic, and cross-layer.
- **Documentation / Messaging** — ensure comments, PR summary, and UI copy tell the truth.

## Overview

This plan covers the remaining work needed to make `feature/network-sharing-remote-transcription` truthfully mergeable into `main` after the original review findings were fixed. A post-fix review found that the branch is close, but still has **two merge-blocking P1 issues** and **several P2 correctness/state-machine gaps** around licensing, retranscription source coverage, startup restore, tray restore, and onboarding/readiness truthfulness.

This plan is intentionally scoped to **merge-readiness**, not feature expansion. The goal is to land the smallest coherent set of changes that makes the system honest for users, operators, and future maintainers.

### Research Insights

**Best Practices:**
- Treat state-machine fixes as **source-of-truth alignment** work, not line-local patches. The same invariant should not exist in three parallel implementations.
- Keep frontend readiness as a projection of backend truth rather than a parallel authority. React guidance specifically warns against redundant derived state managed via effects.
- In Tauri, background startup tasks should use managed state and re-read canonical state at commit time before side effects.

**References:**
- React: *You Might Not Need an Effect* — https://github.com/reactjs/react.dev/blob/main/src/content/learn/you-might-not-need-an-effect.md
- Tauri state management — https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/develop/state-management.mdx
- Tauri startup/setup tasks — https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/learn/splashscreen.mdx

## Problem Statement

The branch currently passes build/test/type/lint gates, but correctness is not yet complete:

1. **Cold-start license enforcement fail-opens** in the recording path when the in-memory cache is empty.
2. **Soniox is supported by the backend but omitted from the re-transcription UI**, creating a product-surface inconsistency.
3. **Startup and restore flows still have duplicated state-machine logic**, allowing stale snapshots or premature state mutation.
4. **Onboarding/readiness can lie** when a remote server is selected but unreachable.
5. **Platform engine support messaging still overstates behavior** in a few places, especially around Intel macOS and Parakeet.

The branch should not merge until the runtime behavior and the product surface say the same thing.

### Research Insights

**Root-cause framing:**
- The licensing bug is not merely “`None => allow`.” It is a split-cache bug: `validate_recording_requirements()` reads `AppState.license_cache`, while broader license lifecycle logic uses cached status elsewhere and invalidates the in-memory cache on transitions. `None` currently means “uninitialized,” not “licensed.”
- The startup bugs are time-of-check/time-of-use bugs. The delayed task captures mutable settings, sleeps, then acts on stale assumptions.
- The onboarding bug is a truthiness bug: selected remote state is currently treated as if it were proven available capability.

**Edge cases that must shape implementation:**
- cold start after app launch
- freshly activated or restored license
- selected remote that is offline
- selected remote that is reachable but auth-failed
- tray-driven restore failure after remote→local switch

## Local Research Summary

### Repo patterns and architecture

- **Backend enforcement boundary:** `src-tauri/src/commands/audio.rs`
  - `validate_recording_requirements()` is the real gate before recording/transcription work.
  - It is reused by `start_recording`, `transcribe_audio_file`, and `transcribe_audio`.
- **Canonical remote/local switching state machine:** `src-tauri/src/commands/remote.rs`
  - `resolve_shareable_model_config`
  - `set_active_remote_server`
  - `transcribe_remote`
- **Tray-specific parallel switching path:** `src-tauri/src/commands/settings.rs`
  - Duplicates part of the remote/local restore logic
  - This is the highest-risk drift point for invariants
- **Startup restore orchestration:** `src-tauri/src/lib.rs`
  - Delayed sharing auto-start
  - Startup cleanup and retry semantics
- **Frontend retranscription source construction:** `src/components/sections/RecentRecordings.tsx`
- **Readiness/onboarding truth model:**
  - backend: `src-tauri/src/recognition/model_selection.rs`
  - frontend: `src/components/AppContainer.tsx`, `src/hooks/useAppReadiness.ts`

### Institutional learnings available locally

There is **no `docs/solutions/` directory** in this repo snapshot. The most relevant institutional guidance is instead in:

- `docs/reviews/2026-03-26-network-sharing-pr-review.md`
- `docs/plans/2026-03-26-001-fix-network-sharing-pr-review-findings-plan.md`
- `docs/plans/2026-03-27-001-fix-post-review-remote-transcription-blockers-plan.md`
- `todos/001` through `todos/007`

### Research decision

**Skipping external research.**

Reason:
- This is a repo-specific merge-readiness/state-machine task, not a novel framework or external API design problem.
- The codebase already has the necessary truth sources and patterns.
- The remaining questions are about aligning product intent with current implementation, not discovering outside best practices.

### Research Insights

**Repo-specific patterns worth reusing:**
- `set_active_remote_server(None)` in `src-tauri/src/commands/remote.rs` already encodes the safest restore invariant: clear `sharing_was_active` only after successful restore.
- `load_remote_settings` / `save_remote_settings` are the canonical persistence boundary for remote state. Startup should not mutate ad hoc store keys directly.
- `resolve_engine_for_model` and `get_model_status()` already prove Soniox is a first-class source in backend/model status; the remaining inconsistency is UI filtering.
- `recognition_availability_snapshot()` is where readiness semantics should be corrected first, because recording and onboarding both depend on it.

## Key Decisions

### Decision 1: Soniox should be supported in re-transcription

**Chosen approach:** Treat Soniox omission as a real bug and fix it.

**Why:**
- `src-tauri/src/commands/audio.rs` already supports Soniox in file/audio transcription paths.
- `src-tauri/src/commands/model.rs` exposes Soniox as a first-class selectable cloud model when configured.
- The re-transcription UI currently filters it out manually in `RecentRecordings.tsx`.
- This is product drift, not intentional scope control.

**Rejected alternative:** Document Soniox as intentionally excluded from re-transcription.

**Why rejected:** The backend and product surface already treat Soniox as supported. Narrowing the docs would preserve a user-visible inconsistency instead of fixing it.

### Decision 2: Intel Macs should not expose Parakeet

**Chosen approach:** Keep runtime behavior as-is and align comments/docs/UI wording.

**Actual support matrix:**
- **Windows x64 / ARM64:** Whisper only
- **macOS Apple Silicon:** Whisper + Parakeet
- **macOS Intel:** Whisper only

**Evidence:**
- `src-tauri/src/parakeet/models.rs:35-50`
- `src-tauri/src/parakeet/manager.rs:64-69`
- `src-tauri/src/whisper/transcriber.rs:51-68,146-176`

**Rejected alternative:** Try to make Intel Macs show Parakeet models “for consistency.”

**Why rejected:** That would make the UI lie about actual engine capability. The hardware/software dependency is real.

### Decision 3: Offline remote selection may remain selectable, but must not satisfy readiness by itself

**Chosen approach:** Separate `remote_selected` from `remote_available` semantics.

**Why:**
- Keeping an offline server selected is a reasonable UX choice.
- Using that stale selection to suppress onboarding/readiness is not.
- Readiness must reflect what can actually transcribe now.
- Security review also identified that **auth-failed** remotes must be treated as unavailable, not merely “reachable.”

**Rejected alternative:** Forbid selecting offline remotes entirely.

**Why rejected:** Bigger product/UX change than needed for merge-readiness.

### Research Insights

**UI/UX implications:**
- Soniox should appear as a **cloud** source in re-transcription, not implicitly as a generic “local” source.
- Selected remote and available remote should be distinct UI states. Suggested status vocabulary: `Selected`, `Online`, `Offline`, `Auth Failed`.
- User-facing copy should prefer truth-preserving language such as `No transcription sources available` instead of `No models available` when remote/cloud sources are in play.

## Technical Approach

### Architecture

The safest plan is to align all remaining behavior with three boundaries:

1. **Recording/transcription enforcement happens in backend command paths**
   - The backend must never fail-open on cache absence or stale state.
2. **There should be one authoritative remote/local restore invariant**
   - Startup path, command path, and tray path should not each invent their own rules.
3. **Frontend onboarding/readiness is advisory, not authoritative**
   - It should reflect backend truth, not over-infer from selected IDs.

### Research Insights

**Framework guidance:**
- React recommends computing derived UI state during render or in a dedicated selector/hook instead of mirroring it into `useState` with effects. For this plan, `showOnboarding` should be downstream of a truthful readiness model, not a separate authority.
- Tauri guidance supports background startup work via `setup` + managed state, but the mutable decision must still be re-checked against current state before binding or persisting side effects.

**Implementation detail to preserve:**
- Keep the backend as the authority for “can transcribe now,” and let UI consume that via a narrower readiness model.

## Execution Rule: Red-Green-Refactor

For every task in this plan, use explicit red-first TDD:

- write the smallest targeted failing regression test first,
- run it until it fails for the intended reason,
- implement the minimum production change to make it pass,
- then refactor while keeping the targeted regression green.

This is mandatory for merge-readiness work because these bugs are state-machine and truth-boundary issues that are easy to "fix" cosmetically without proving the real failure mode.

### Research Insights

**TDD guidance:**
- The plan should not rely on "tests added" as a trailing activity. Each blocker should be pinned to a witnessed failing regression before implementation.
- Prefer command-boundary and hook-boundary regressions over helper-only tests so the real public behavior is proven.
- For timing-sensitive startup logic, extract deterministic seams before implementation so the red step does not depend on wall-clock sleeps.

## Implementation Phases

### Phase 1: Close merge blockers

#### Task 1: Fix cold-start license enforcement

**Primary files:**
- `src-tauri/src/commands/audio.rs`
- `src-tauri/src/commands/license.rs`
- possibly `src-tauri/src/lib.rs` if cache initialization needs sequencing support

**Problem:**
`validate_recording_requirements()` currently allows recording when `license_cache` is empty.

**Recommended implementation:**
- Remove the permissive `None => allow` branch.
- Replace `None` semantics with an explicit runtime state such as `Unknown/Loading`, which blocks recording until the backend knows the license snapshot.
- Seed or warm the backend in-memory cache at startup.
- Also seed or refresh the backend cache immediately after **license activation** and **license restore** so a newly licensed user is not stranded in a false `Loading` state.
- Keep network dependency out of the hotkey path; prefer cache warming over live validation during `start_recording()`.

### Research Insights

**Best Practices:**
- Treat this as a source-of-truth repair: the same cache consulted for enforcement must be reliably populated whenever license state changes.
- A fail-closed gate is only safe if `Licensed` is explicit and absence is never interpreted as authorization.

**Implementation Details:**
```rust
// Desired semantic shape, not final code
match app_state.license_cache.read().await.as_ref() {
    Some(cached) if cached.status.status == LicenseState::Licensed => allow,
    Some(cached) if matches!(cached.status.status, LicenseState::Expired | LicenseState::None) => block,
    None => block_with_loading_or_unknown_state,
}
```

**Edge Cases:**
- cold start after app launch
- restored license immediately followed by recording
- activated license immediately followed by recording
- offline grace-period scenarios should remain explicit, not inferred from cache absence

**Acceptance criteria:**
- [ ] Cold-start expired/missing-license state cannot start recording
- [ ] Valid licensed users still avoid network latency on steady-state recording start
- [ ] Activation/restore transitions repopulate or warm the backend cache
- [ ] Tests cover empty-cache, valid-cache, expired-cache, none-license, restore, and activation branches

**Estimated effort:** Medium

---

#### Task 2: Add Soniox to re-transcription sources

**Primary files:**
- `src/components/sections/RecentRecordings.tsx`
- related frontend tests for retranscription source lists

**Problem:**
The UI includes only Whisper and Parakeet in retranscription source discovery, even though Soniox is already supported by the backend.

**Recommended implementation:**
- Expand the source list to include Soniox when configured/available.
- Make the source taxonomy explicit end-to-end: `local`, `cloud`, and `remote` should be distinguishable in both menu construction and parser/build logic.
- If the existing `sourceId` shape is preserved, ensure the parser and display layer still treat Soniox as a **cloud** source rather than a generic local-model entry.
- Review empty-state and error copy in retranscription flows to use `sources` rather than `models` where appropriate.

### Research Insights

**UX guidance:**
- Keep source categories truthful: local models, cloud source, remote servers.
- If the menu remains flat, the display label should still make the source type obvious, e.g. `Soniox (Cloud)`.

**Acceptance criteria:**
- [ ] Soniox appears in the re-transcription menu when configured
- [ ] Cloud-only users can re-transcribe saved recordings
- [ ] Source taxonomy and parser/build logic consistently distinguish local, cloud, and remote options
- [ ] Source labeling distinguishes local, cloud, and remote options clearly
- [ ] Whisper, Parakeet, and remote re-transcription remain unchanged

**Estimated effort:** Small to Medium

### Phase 2: Align state-machine correctness before merge

#### Task 3: Re-check full remote state before delayed sharing auto-start

**Primary files:**
- `src-tauri/src/lib.rs`
- possibly a shared helper extracted with `src-tauri/src/commands/remote.rs`

**Problem:**
Startup auto-start snapshots state, waits 2 seconds, then starts sharing without re-reading current remote configuration.

**Recommended implementation:**
- Immediately before binding/starting sharing inside the delayed startup task, reload the full canonical `RemoteSettings` snapshot.
- Re-check:
  - whether an active remote is now selected,
  - whether sharing is still enabled,
  - current server config such as port/password.
- Abort local sharing auto-start if the refreshed state no longer permits sharing.
- Prefer extracting a helper so the delayed task tests a pure decision function instead of wall-clock behavior.

### Research Insights

**Architecture review additions:**
- The stale snapshot risk is broader than `active_id`; stale `port`, `password`, and enablement can also leak through the delayed task.
- A deterministic seam is required here; otherwise tests will depend on a real 2-second sleep and become flaky.

**Acceptance criteria:**
- [ ] Selecting a remote during startup delay prevents local sharing auto-start
- [ ] Disabling sharing or changing sharing config during the delay is honored
- [ ] Normal startup restore still works when refreshed state permits it
- [ ] Tests cover the race without relying on wall-clock sleep

**Estimated effort:** Small to Medium

---

#### Task 4: Keep `sharing_was_active` until tray restore succeeds

**Primary files:**
- `src-tauri/src/commands/settings.rs`
- shared restore helper extracted with `src-tauri/src/commands/remote.rs`

**Problem:**
Tray remote→local switching clears `sharing_was_active` before restore succeeds, unlike the safer command path.

**Recommended implementation:**
- Match the invariant already used in `set_active_remote_server(None)`.
- Clear and persist `sharing_was_active` only after `resolve_shareable_model_config()` and `manager.start()` succeed.
- Extract a shared restore helper and use it from tray and command flows so the invariant cannot drift again.

### Research Insights

**Root-cause guidance:**
- Do not patch the early clear in place and leave two restore flows diverging. The bug exists because the invariant lives in parallel implementations.
- The real objective is to eliminate duplicated sequencing, not merely shuffle one line.

**Acceptance criteria:**
- [ ] Failed tray restore preserves restore intent
- [ ] Successful tray restore clears and persists the flag
- [ ] Tray and command restore paths both delegate to the same restore helper/invariant
- [ ] Tests exercise both success and failure cases through the tray path

**Estimated effort:** Medium

---

#### Task 5: Fix startup restore failure persistence and clear the actual startup retry gate

**Primary files:**
- `src-tauri/src/lib.rs`
- `src-tauri/src/commands/remote.rs`
- `src-tauri/src/remote/settings.rs`

**Problem:**
Startup invalid-config cleanup writes remote state through the wrong path and does not save. The plan also needs to clear the real startup retry gate, not just one flag.

**Recommended implementation:**
- Stop writing directly to `store("remote_settings")` from the startup task.
- Mutate and persist the real `RemoteSettings` aggregate through the canonical save path already used elsewhere.
- Reconcile all startup retry gating fields, especially `server_config.enabled` and related restore-intent state, so relaunch cannot repeatedly retry a known-bad config.

### Research Insights

**Architecture review additions:**
- The actual startup retry gate is not only `sharing_was_active`; startup currently keys off persisted enablement as well.
- Narrowly clearing one field risks a “fixed cleanup” that still retries forever.

**Acceptance criteria:**
- [ ] Invalid startup sharing config clears the real persisted retry gate(s)
- [ ] Relaunch does not repeat a known-bad startup share configuration forever
- [ ] Startup cleanup uses the same persistence path as runtime remote settings logic
- [ ] Tests verify persisted behavior across relaunch simulation

**Estimated effort:** Small to Medium

---

#### Task 6: Make onboarding/readiness truthful when remote is offline or auth-failed

**Primary files:**
- `src-tauri/src/recognition/model_selection.rs`
- `src-tauri/src/commands/audio.rs`
- `src/components/AppContainer.tsx`
- `src/hooks/useAppReadiness.ts`
- `src/hooks/useModelAvailability.ts`
- `src/components/sections/AudioUploadSection.tsx`
- possibly `src-tauri/src/commands/remote.rs`

**Problem:**
Any active remote ID can currently suppress onboarding and satisfy readiness, even if the remote is offline or auth-failed.

**Recommended implementation:**
- Decouple `remote_selected` from `remote_available`.
- Define one backend-owned readiness contract or snapshot that treats a remote as available only when it is verified and authenticated, not merely selected or merely reachable.
- Update onboarding visibility logic, readiness hooks, upload-path checks, and recording-path validation to consume that same truthful availability model.
- Add an explicit refresh/event path so readiness recomputes when remote selection or remote status changes.
- Preserve offline selection UX if desired, but display it as `selected but unavailable` rather than `ready`.

### Research Insights

**Frontend-design guidance:**
- Make `selected` and `available` separate UI states.
- Show explicit status pills such as `Online`, `Offline`, `Auth Failed`.
- Use truth-preserving copy: `Selected remote unavailable` and `No transcription sources available`.

**Architecture review additions:**
- `AppContainer` is not the only consumer; readiness also leaks through `useAppReadiness`, `useModelAvailability`, and upload-path guards.
- The plan must fix recording-path truth, not just onboarding visibility.

**Security review additions:**
- Auth-failed remotes must be treated as unavailable, not merely as online.

**Acceptance criteria:**
- [ ] Offline remote selections do not suppress onboarding by themselves
- [ ] Auth-failed remotes do not satisfy readiness or recording eligibility
- [ ] Readiness reflects reachable, authenticated engines, not just selected engine IDs
- [ ] The backend readiness contract is shared by recording-path validation and frontend readiness consumers
- [ ] The UI distinguishes between selected and available remote sources
- [ ] Remote selection/status changes trigger explicit readiness refresh

**Estimated effort:** Medium

### Phase 3: Truthful platform and product messaging

#### Task 7: Clarify platform engine support and mode claims, and add executable assertions

**Primary files:**
- `src-tauri/src/parakeet/manager.rs`
- `src-tauri/src/commands/model.rs`
- any branch docs/PR descriptions updated during merge prep, after Tasks 1-6 are verified
- related tests for model enumeration/platform behavior

**Problem:**
Some wording still implies broader support than runtime actually provides, and there are not enough explicit assertions to guard the support matrix.

**Recommended implementation:**
- Update comments/docs/UI wording to match reality exactly:
  - Windows = Whisper only
  - macOS Apple Silicon = Whisper + Parakeet
  - macOS Intel = Whisper only
- Clarify that active remote usage and local sharing are currently mutually exclusive modes.
- Add executable assertions or targeted tests that lock in the platform matrix behavior instead of relying only on text cleanup.

### Research Insights

**Testing review additions:**
- Docs-only cleanup is not enough if code comments and runtime gates can drift again.
- Platform-matrix tests should explicitly cover Apple Silicon, Intel macOS, and non-macOS behavior where feasible.

**Acceptance criteria:**
- [ ] No comments/docs imply Intel Parakeet support
- [ ] No comments/docs imply simultaneous active remote usage and local sharing when runtime forbids it
- [ ] Targeted tests/assertions guard platform engine enumeration behavior

**Estimated effort:** Small to Medium

## Alternative Approaches Considered

### Alternative A: Merge now, fix follow-ups later

**Rejected.**

Reason:
- The branch still contains two real merge blockers.
- The startup/restore/onboarding issues are exactly the kind of bugs that become expensive after merge because they depend on timing and persisted state.

### Alternative B: Shrink scope by removing Soniox from branch claims

**Rejected.**

Reason:
- Soniox support is already in the backend and model surface.
- The real inconsistency is the UI filter, not the product concept.

### Alternative C: Rewrite remote/local switching architecture before merge

**Rejected for now.**

Reason:
- The branch can be made truthful without a large architectural rewrite.
- The immediate need is merge readiness, not redesign.

## System-Wide Impact

### Interaction Graph

**Recording start path**
- Hotkey/UI start triggers `start_recording`
- `start_recording` calls `validate_recording_requirements`
- `validate_recording_requirements` reads recognition availability + `AppState.license_cache`
- On success, recording starts and later transcribes via local/remote engine resolution

**Remote/local readiness path**
- Remote selection persists in `RemoteSettings.active_connection_id`
- `recognition_availability_snapshot` currently treats selected remote as available
- `auto_select_model_if_needed` may mark onboarding complete
- `AppContainer`, readiness hooks, and upload guards can all consume stale remote truth differently

**Sharing restore path**
- Startup task in `lib.rs` decides whether to auto-start sharing after delay
- Tray switching in `commands/settings.rs` can also restore sharing
- `commands/remote.rs` contains the safer runtime restore invariant

### Error & Failure Propagation

- License enforcement errors currently terminate recording start with user-facing errors, except for the empty-cache branch.
- Activation/restore transitions can invalidate runtime cache and need explicit repopulation to avoid false `Loading` states.
- Startup sharing restore failures log warnings but can leave retry state inconsistent due to wrong persistence path.
- Tray restore failures can silently lose restore intent because state is mutated before success.
- Offline or auth-failed remote readiness errors currently propagate poorly because selection state is treated as availability.

### State Lifecycle Risks

- `license_cache` can be absent during cold start or after license-state changes while recording is invokable.
- `sharing_was_active` currently has multiple mutation points with inconsistent ordering.
- `server_config.enabled` and related persisted remote settings can continue to trigger startup retries unless reconciled correctly.
- `active_connection_id` is durable state, but onboarding/readiness currently over-trust it.
- Startup tasks use delayed async work, creating stale snapshot risk.

### API Surface Parity

Equivalent functionality that must stay aligned:
- Recording/transcription model availability:
  - main model selection UI
  - retranscription source UI
  - backend `resolve_engine_for_model`
  - readiness hooks and upload-path checks
- Remote/local switching:
  - `set_active_remote_server`
  - tray-driven `set_model_from_tray`
  - startup restore in `lib.rs`

### Integration Test Scenarios

1. **Cold-start license gate**
   - App starts with empty license cache and expired/missing entitlement.
   - Recording must not begin.

2. **License activation/restore warm path**
   - License is restored or activated.
   - Backend cache becomes usable immediately for recording-path enforcement.

3. **Soniox-only retranscription**
   - Soniox configured, no local model downloaded.
   - Saved recording can be re-transcribed successfully.

4. **Startup race with remote selection**
   - Sharing auto-start pending after delay.
   - User selects remote before delay completes.
   - Local sharing must not start.

5. **Startup config change during delay**
   - Port/password or sharing enablement changes during startup delay.
   - Delayed auto-start must honor refreshed config.

6. **Tray restore failure preserves intent**
   - Switch from remote to a local model that cannot be shared.
   - `sharing_was_active` must remain true for future retry.

7. **Offline remote does not satisfy recording path**
   - No local models, active remote exists but is offline.
   - `validate_recording_requirements()` must still block recording.

8. **Auth-failed remote does not satisfy onboarding/readiness**
   - Active remote is reachable but password is wrong.
   - Onboarding/readiness must still reflect lack of usable engine.

9. **Platform matrix assertions**
   - Windows: Whisper only
   - macOS Apple Silicon: Whisper + Parakeet
   - macOS Intel: Whisper only

## Acceptance Criteria

### Functional Requirements
- [ ] Cold-start recording cannot bypass license enforcement
- [ ] Activation and restore transitions repopulate or warm backend license cache
- [ ] Soniox is available for re-transcription when configured
- [ ] Startup sharing auto-start re-checks full current remote state before binding
- [ ] Tray restore preserves `sharing_was_active` until success
- [ ] Startup restore failure clears the real persisted retry gate(s)
- [ ] Offline remote selection does not falsely satisfy onboarding/readiness or recording eligibility
- [ ] Auth-failed remote selection does not falsely satisfy onboarding/readiness or recording eligibility
- [ ] Platform support messaging matches runtime reality exactly

### Non-Functional Requirements
- [ ] No new synchronous network check in the hotkey-start path
- [ ] No new cross-path state-machine drift between startup, command, and tray flows
- [ ] User-facing error messages remain actionable and specific
- [ ] Startup race tests avoid wall-clock sleeps and use deterministic seams where possible

### Quality Gates
- [ ] `cargo check`
- [ ] `cargo test --lib`
- [ ] `pnpm typecheck`
- [ ] `pnpm lint`
- [ ] `pnpm test`
- [ ] Targeted tests added for cold-start cache state, activation/restore cache warm, Soniox retranscription, startup timing races, tray restore failure, offline/auth-failed remote truth, and platform matrix assertions

## Success Metrics

- Branch can be honestly called **merge-ready** with no known P1 blockers.
- Runtime behavior matches UI and docs for supported engines by platform.
- Startup/restore/onboarding state transitions no longer depend on stale or over-trusted state.
- Cloud-only Soniox users retain parity with local-engine users for re-transcription.
- Newly activated/restored users are not trapped in a false licensing bootstrap state.

## Dependencies & Risks

### Dependencies
- Rust/backend work and frontend work can happen in parallel after the decision to support Soniox in retranscription.
- Startup/restore fixes should be coordinated because they touch related invariants.
- Readiness/onboarding work must coordinate frontend selectors with backend availability semantics.

### Risks
- Over-fixing onboarding could accidentally break the intentional UX of keeping offline remotes selected.
- Fixing license enforcement incorrectly could reintroduce network latency or false negatives.
- Tray and command restore flows could drift again if not aligned intentionally.
- Docs-only cleanup of platform support could mask future behavior drift if no tests back it.

### Mitigations
- Reuse `commands/remote.rs` as the canonical restore reference.
- Keep backend enforcement and frontend readiness concerns separate.
- Add targeted tests for each state transition bug, not just broad regression suites.
- Prefer pure helpers and deterministic seams for timing-sensitive startup logic.

## Documentation Plan

Update as part of this work:
- runtime comments describing platform engine availability
- any merge summary / PR description text that overstates support
- QA notes for platform matrix, Soniox retranscription, and selected-vs-available remote behavior if maintained locally
- user-facing copy where `models` is too narrow and `sources` is more truthful

## Sources & References

### Internal References
- `docs/reviews/2026-03-26-network-sharing-pr-review.md`
- `docs/plans/2026-03-26-001-fix-network-sharing-pr-review-findings-plan.md`
- `docs/plans/2026-03-27-001-fix-post-review-remote-transcription-blockers-plan.md`
- `src-tauri/src/commands/audio.rs`
- `src-tauri/src/commands/license.rs`
- `src-tauri/src/commands/remote.rs`
- `src-tauri/src/commands/settings.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/recognition/model_selection.rs`
- `src-tauri/src/parakeet/models.rs`
- `src-tauri/src/parakeet/manager.rs`
- `src-tauri/src/whisper/transcriber.rs`
- `src/components/sections/RecentRecordings.tsx`
- `src/components/AppContainer.tsx`
- `src/hooks/useAppReadiness.ts`
- `src/hooks/useModelAvailability.ts`
- `src/components/sections/AudioUploadSection.tsx`
- `todos/001-pending-p1-license-cache-bypass-on-cold-start.md`
- `todos/002-pending-p1-soniox-missing-from-retranscription-sources.md`
- `todos/003-pending-p2-startup-sharing-races-with-remote-selection.md`
- `todos/004-pending-p2-tray-restore-clears-sharing-flag-too-early.md`
- `todos/005-pending-p2-startup-restore-fails-to-clear-persisted-flag.md`
- `todos/006-pending-p3-clarify-platform-engine-support-and-mode-claims.md`
- `todos/007-pending-p2-offline-remote-selection-skips-onboarding-readiness.md`

### External References
- React derived-state guidance: https://github.com/reactjs/react.dev/blob/main/src/content/learn/you-might-not-need-an-effect.md
- React effect cleanup guidance: https://github.com/reactjs/react.dev/blob/main/src/content/reference/eslint-plugin-react-hooks/lints/set-state-in-effect.md
- Tauri state management: https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/develop/state-management.mdx
- Tauri setup/background task pattern: https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/learn/splashscreen.mdx

### Related Work
- PR #57

## Final Recommendation

Do not merge the branch yet.

The best path is:
1. fix the two P1 blockers first,
2. land the startup/restore/onboarding P2 correctness fixes in the same pass,
3. add the targeted deterministic regressions that prove the branch is now truthful,
4. then re-run the full quality gate and manual QA flows before merge.
