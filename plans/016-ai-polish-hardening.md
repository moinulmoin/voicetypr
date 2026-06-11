# Plan 016: AI polish Rust-native provider layer — replace Pi sidecar with VoiceTypr-owned contract

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan in
> `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat b1a66bf..HEAD -- src-tauri/Cargo.toml src-tauri/build.rs src-tauri/src/commands/ai.rs src-tauri/src/formatting src-tauri/src/lib.rs src-tauri/src/writing.rs src/components/sections/EnhancementsSection.tsx src/utils/keyring.ts sidecar/formatting-engine package.json pnpm-lock.yaml`
>
> On drift, compare the "Current state" excerpts to live code first. If the
> sidecar command protocol, settings keys, or AI settings UI has changed,
> update this plan before writing code.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED-HIGH (changes the provider runtime for every AI-polished
  dictation and removes a packaged sidecar)
- **Depends on**: none hard. Prefer after plan 015's no-transcript-loss path if
  both are ready, but this plan must still preserve raw transcript on polish
  failure itself.
- **Category**: reliability / architecture / launch polish
- **Planned at**: commit `b1a66bf`, 2026-06-11
- **Replaces**: the previous Plan 016 draft that hardened the Pi sidecar in
  place. The product direction is now Rust-native provider execution; do not
  invest further in the Node/Pi sidecar except as a temporary spike fallback.

## Decision

Use a **VoiceTypr-owned AI polish contract** and move normal AI polish execution
into Rust:

```txt
Enhancements UI / settings / CLI later
        |
        v
VoiceTypr AI provider contract
  - AiProvider
  - AiModel
  - AiPolishRequest
  - AiPolishResult
  - AiProviderError
        |
        +--> generated catalog from models.dev / AISDK-style metadata
        |
        +--> genai runtime where supported
        |
        +--> small direct reqwest adapter for gaps such as custom OpenAI-compatible
```

The UI must not know whether `genai`, `aisdk`, direct `reqwest`, or a future SDK
executes a provider. SDK types must not leak across Tauri command boundaries,
settings, history records, or frontend TypeScript types.

## Evidence

| Evidence | Meaning for VoiceTypr |
|---|---|
| `sidecar/formatting-engine/package.json` depends on `@earendil-works/pi-ai` and Node `>=22.19.0`. | Current AI polish has a Node runtime/process/package seam. This is exactly the class of fragility we are avoiding for V2. |
| `src-tauri/Cargo.toml` already ships `reqwest = 0.12.22`. | Rust-native provider calls do not require a new process boundary. |
| `genai` API reference documents `AuthResolver`, `ServiceTargetResolver`, `with_reqwest`, `WebConfig`, typed errors, `ChatOptions`, and adapters for OpenAI, Anthropic, Gemini, Groq, xAI, OpenRouter, Together, Fireworks, Cohere, DeepSeek, etc. | Best runtime fit: runtime-selected providers, key lookup from VoiceTypr storage, custom endpoints, timeout control, typed errors. |
| `aisdk` `Cargo.toml` is MIT, `0.5.2`, `edition = "2024"`, uses `reqwest = 0.12`, and exposes a very broad feature list generated around many providers. | Best catalog/reference candidate and possible fallback runtime, but younger and broader than we need for the hot path. Spike before committing it as runtime. |
| `models.dev` is an open database of AI model specs, providers, context, output, reasoning/tool/structured flags, prices, release/update dates. | Use generated metadata instead of hand-maintaining a stale provider/model list. |
| `genai` stable docs list `genai = "0.6"`; `custom` adapter is noted as newer/beta in the API reference. | Pin stable first. If custom OpenAI-compatible needs beta, implement that one path with direct `reqwest` rather than dragging a beta runtime into the app. |

## Product invariant

AI polish must feel like a native VoiceTypr feature, not an external agent SDK
bolted on:

1. **Setup fails early**: invalid key/model/base URL is rejected before save.
2. **Dictation never gets trapped**: if polish fails, the raw transcript is
   still delivered/saved and the UI explains that only polish failed.
3. **One runtime budget**: VoiceTypr owns timeout, cancellation, and retry.
4. **Broad but not chaotic**: many API-key providers are searchable, but the
   default picker is grouped by reliability/fit for dictation cleanup.
5. **No sidecar for normal polish**: no Node process, no Pi runtime, no SEA
   packaging in the final done state.

## Current state

### A. Pi sidecar is the runtime

`sidecar/formatting-engine/package.json`:

```json
"dependencies": {
  "@earendil-works/pi-ai": "^0.79.1"
},
"engines": {
  "node": ">=22.19.0"
}
```

The Rust side talks to this process through `src-tauri/src/formatting/sidecar.rs`.
Normal polish pays process lifecycle, protocol, timeout, build, and packaging
complexity.

### B. Existing Rust backend already has HTTP/client foundation

`src-tauri/Cargo.toml` already includes:

```toml
reqwest = { version = "0.12.22", features = ["json", "stream", "multipart", "blocking"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1.0"
tokio = { version = "1.46.0", features = ["full"] }
```

So a Rust-native polish layer can reuse existing async/runtime dependencies.

### C. Existing Plan 016 facts still matter, but the transport changes

Carry these requirements forward from the old draft:

- validate keys before persisting them;
- no write of custom base URL/model/no-auth settings before validation;
- retry only retryable provider errors within one bounded budget;
- remember model per provider;
- report clear user-facing errors;
- do not lose dictated text if polish fails.

Do **not** carry forward the old assumption that Pi/Node remains the final
transport.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Rust compile | `cd src-tauri && cargo check` | exit 0 |
| Rust tests | `cd src-tauri && cargo test` | all pass |
| Rust lint/format | `cd src-tauri && cargo clippy -- -D warnings && cargo fmt --check` | exit 0 |
| Frontend gates | `pnpm typecheck && pnpm lint && pnpm test --run` | all pass |
| Build | `pnpm build` | exit 0 |
| Diff hygiene | `git diff --check` | clean |

Do not run broad gates until the plan reaches its verification step. During
implementation, run the narrow command named by each step.

## Scope

**In scope**:

- Rust AI provider module/contract under `src-tauri/src/ai/` or an equivalent
  backend-local module name.
- Tauri commands in `src-tauri/src/commands/ai.rs` that currently list models,
  validate/cache keys, and format text.
- `src-tauri/src/lib.rs` command registration/startup wiring.
- `src/utils/keyring.ts` and `src/components/sections/EnhancementsSection.tsx`
  save-order and provider/model UX.
- `src-tauri/build.rs`, root/sidecar package config, and lockfiles needed to
  remove the formatting sidecar from normal builds after parity.
- Tests for provider mapping, catalog filtering, validation ordering, retry
  policy, timeout/cancel behavior, and no-transcript-loss on polish failure.

**Out of scope**:

- Local LLM inference with Burn or another model runtime. Burn is not a cloud
  provider/key/model catalog layer.
- STT providers such as OpenAI STT, Groq Whisper, Deepgram, Soniox. This plan is
  text polish after transcription.
- Writing rule engine boundary/performance fixes from the old plan. Those are
  valid but separate; do not let them block the provider runtime cutover.
- Plan 012 shared transcription contract implementation.
- UI redesign beyond the provider/model setup flow needed for this contract.

## Architecture target

### Rust types

Create explicit serializable contract types. Names can vary, but the boundaries
must be this clear:

```rust
pub struct AiProvider {
    pub id: String,
    pub label: String,
    pub auth: AiProviderAuth,
    pub runtime: AiProviderRuntime,
    pub status: AiProviderStatus,
    pub notes: Vec<String>,
}

pub struct AiModel {
    pub provider_id: String,
    pub model_id: String,
    pub label: String,
    pub context_window: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub supports_reasoning: bool,
    pub supports_structured_output: bool,
    pub recommended_for_dictation: bool,
}

pub struct AiPolishRequest {
    pub provider_id: String,
    pub model_id: String,
    pub input_text: String,
    pub prompt: String,
    pub timeout_ms: u64,
    pub reasoning_effort: Option<AiReasoningEffort>,
}

pub struct AiPolishResult {
    pub output_text: String,
    pub provider_id: String,
    pub model_id: String,
}

pub enum AiProviderError {
    MissingApiKey,
    InvalidApiKey,
    InvalidModel,
    UnsupportedProvider,
    Timeout,
    Canceled,
    RateLimited,
    ServiceUnavailable,
    Network,
    BadResponse,
    Internal,
}
```

Rules:

- No SDK-specific enum/type crosses this boundary.
- Error variants map to stable user-facing strings and CLI-ready codes later.
- Provider/model IDs are stable strings stored in settings/history.
- The executor accepts borrowed strings where practical; avoid cloning the
  transcript except where the provider SDK requires an owned body.

### Catalog

Generate a static catalog file from `models.dev`/AISDK-style metadata and commit
that generated artifact. The app should not fetch the global catalog at runtime
for normal settings UI.

Required catalog fields:

- provider id, provider label;
- auth type: API key, no-auth local, OAuth/agent/unsupported;
- runtime mapping: `genai` adapter kind, OpenAI-compatible endpoint template, or
  `unsupported`;
- model id, display label;
- context/output limits when available;
- reasoning/structured/tool flags when available;
- recommended-for-dictation boolean computed by VoiceTypr rules;
- source metadata version/date.

UI behavior:

- Show API-key providers by default.
- Group: Recommended, Popular, All API-key providers.
- Search across all API-key providers/models.
- Hide OAuth/agent-only/provider-proxy entries unless an explicit Advanced
  toggle is added. This is not curation-by-hand; it is a capability filter.

### Runtime

Preferred runtime order:

1. `genai = "0.6"` for providers/adapters it supports cleanly.
2. Direct `reqwest` adapter for custom OpenAI-compatible endpoints and any
   provider where `genai` stable cannot express the required request shape.
3. AISDK runtime only if the spike proves it gives materially better provider
   coverage without worse timeout/cancel/error control.

VoiceTypr owns:

- `reqwest::Client` construction and timeouts;
- API-key lookup via secure store/cache, not env vars;
- retry policy;
- cancellation;
- error mapping;
- no-transcript-loss behavior.

## Steps

### Step 1: SDK and catalog spike, then choose runtime in code comments

Do this as the first implementation commit/chunk. Do not remove Pi yet.

1. Add the minimum dependency candidate behind a tiny module:
   - preferred first: `genai = "0.6"`;
   - do not use a beta `genai` release unless stable cannot support a MUST
     provider and direct `reqwest` is worse;
   - do not add AISDK as a runtime dependency unless the comparison below proves
     it is the better executor.
2. Create a backend-only spike test/module that can build requests for:
   - OpenAI;
   - Anthropic;
   - Gemini;
   - OpenRouter;
   - Groq;
   - xAI;
   - Custom OpenAI-compatible.
3. For each provider, prove these can be represented without env vars:
   - provider id;
   - model id;
   - API key from a resolver or direct headers;
   - timeout from VoiceTypr;
   - optional reasoning effort where supported;
   - user-facing error mapping.
4. Compare AISDK only for the same matrix if `genai` cannot satisfy the matrix.
   The comparison is not theoretical: compile it or inspect exact source for
   timeout/retry/dynamic model behavior and record the finding in a code comment
   or the implementation report.

**Verify**: `cd src-tauri && cargo check`.

**Proceed only if** one runtime path satisfies OpenAI, Anthropic, Gemini,
OpenRouter, Groq, xAI, and custom OpenAI-compatible with VoiceTypr-owned
errors/timeouts. If not, hit STOP condition 1.

### Step 2: Add VoiceTypr AI contract module

1. Add `src-tauri/src/ai/` with focused modules, for example:

```txt
src-tauri/src/ai/
  mod.rs
  catalog.rs
  contract.rs
  error.rs
  executor.rs
  genai_runtime.rs
  openai_compatible.rs
```

2. Define the public Rust contract types from the Architecture target. Keep
   derives explicit: `Debug`, `Clone` only where needed, `Serialize`/
   `Deserialize` where crossing Tauri.
3. Add `From`/mapping code from runtime errors into `AiProviderError`.
4. Add unit tests for error-to-user-message mapping. The test should assert
   categories, not exact provider debug strings.

**Verify**: `cd src-tauri && cargo test ai::`.

### Step 3: Generate and wire provider/model catalog

1. Add a deterministic generator script or Rust build helper that reads a pinned
   source snapshot from `models.dev`/AISDK-style metadata and writes a static
   catalog artifact. Prefer a checked-in JSON artifact over runtime network
   fetch.
2. The generated file must be small enough to ship. If the full catalog is too
   large/noisy, filter at generation time to API-key text-generation providers
   plus the launch priority set.
3. Catalog generation must produce stable sorting so diffs are reviewable.
4. Add catalog tests:
   - provider IDs unique;
   - model IDs unique per provider;
   - every runtime-supported provider has at least one model;
   - every default/recommended model maps to a runtime adapter;
   - no OAuth/agent-only provider appears in the default API-key list.
5. Replace the old curated/fallback model list in `commands/ai.rs` with this
   catalog response shape. If live provider model listing is kept, it must be
   an optional refresh/status signal, not the only source of truth.

**Verify**: `cd src-tauri && cargo test ai::catalog` and `pnpm typecheck`.

### Step 4: Validate keys before save, then persist

1. Backend: expose a pure validation command such as `validate_ai_provider`.
   It must not write settings and must not cache keys.
2. Provider validation policy:
   - use the runtime SDK's model-list or minimal request when reliable;
   - use direct provider validation endpoints when the SDK cannot distinguish
     invalid auth from other failures;
   - custom OpenAI-compatible validates base URL + model + auth/no-auth in one
     probe.
3. Frontend `src/utils/keyring.ts`: save order becomes:

```txt
validate provider/base/model/key
→ write key to secure store when required
→ cache key in backend memory
→ persist provider/base/model settings
```

4. `EnhancementsSection.tsx`: custom base URL, no-auth, and selected model must
   not be written before validation succeeds.
5. Keep startup cache-fill offline tolerant: a previously saved key should still
   load when offline; only user-initiated save requires live validation.

**Verify**: `cd src-tauri && cargo test ai::validation`; `pnpm typecheck`.

Manual smoke before final DONE:

- invalid Gemini/Anthropic/OpenAI key rejected before save;
- valid key saves and model picker can select a model;
- custom base URL with bad endpoint does not persist.

### Step 5: Implement Rust-native polish executor

1. Route `format_text_with_ai`/existing AI polish call path through the new
   Rust executor.
2. Runtime policy:
   - total budget defaults to existing polish timeout unless product changes it;
   - retry at most once for 429, network reset, and 5xx/service unavailable;
   - respect provider `Retry-After` only inside remaining budget;
   - never retry invalid auth, invalid model, unsupported provider, prompt
     validation, or cancellation;
   - cancellation must abort waiting on the provider future.
3. Prompt/request policy:
   - preserve existing deterministic pre/post writing rules;
   - pass only the text needed for polish;
   - do not log transcript content or provider response content;
   - map reasoning effort only when model/provider supports it, otherwise omit
     it instead of sending fake defaults.
4. Result policy:
   - empty/whitespace provider output is `BadResponse`;
   - if polish fails after transcription, deliver/save raw transcript with a
     polish-failed warning instead of discarding the dictation;
   - include provider/model/error category in diagnostics without transcript
     content.

**Verify**: Rust unit tests with mocked HTTP for success, invalid key, invalid
model, 429 retry once, 5xx retry once, timeout, cancellation, empty response,
and no-transcript-loss wrapper behavior.

### Step 6: Cut UI/settings over to catalog + Rust runtime

1. Provider picker:
   - default list = API-key providers from catalog;
   - Recommended group computed from catalog flags, not hard-coded UI arrays;
   - All group searchable;
   - Advanced-only/unsupported providers clearly hidden or disabled.
2. Model picker:
   - uses generated catalog metadata;
   - shows dictation-friendly hints: fast/low-cost/reasoning support where data
     exists;
   - never falls back silently to stale hard-coded models without a visible
     status.
3. Per-provider model memory:
   - store `ai_models_by_provider` map;
   - preserve existing `ai_model` active value for current readers until all
     readers are migrated;
   - provider switch restores previous model when still valid.
4. Reasoning UI:
   - default = provider/model default;
   - optional setting only when `supports_reasoning` is true;
   - copy: "Lower reasoning usually feels faster for dictation cleanup. Use
     stronger reasoning only for heavier rewriting."

**Verify**: `pnpm typecheck && pnpm test --run`.

Manual smoke before final DONE:

- configure OpenAI, select model, polish dictation;
- switch OpenAI -> Gemini/Anthropic -> OpenAI and model restores;
- search finds a non-recommended API-key provider from catalog;
- unsupported/no-auth/agent-only provider is not shown in default flow.

### Step 7: Remove Pi sidecar from normal build

This step is part of DONE, not optional cleanup.

1. Use Search/Find first to identify all references to:
   - `formatting-engine`;
   - `FormattingClient` / `FormattingCommand`;
   - `@earendil-works/pi-ai`;
   - sidecar packaging paths in `build.rs` and Tauri config.
2. Remove the Node formatting sidecar from normal AI polish:
   - delete unused Rust sidecar client code if no longer referenced;
   - remove sidecar build invocation from `src-tauri/build.rs`;
   - remove `sidecar/formatting-engine` package from workspace/build inputs if
     it has no other purpose;
   - remove `@earendil-works/pi-ai` from lockfile via package manager, not by
     hand-editing lockfile text.
3. Do not leave a user-selectable "legacy Pi" runtime or hidden compatibility
   shim in the shipped path. Temporary local spike code must be deleted before
   DONE.
4. If deleting the whole sidecar directory would remove useful history/tests,
   remove it in the same commit after verifying no build scripts reference it.
   Ask only if the operator wants to keep the directory for archival reasons;
   otherwise clean cutover is the default.

**Verify**: `pnpm build`; `cd src-tauri && cargo check`.

### Step 8: Full verification gates

Run, in order:

```bash
cd src-tauri && cargo test
cd src-tauri && cargo clippy -- -D warnings && cargo fmt --check
pnpm typecheck
pnpm lint
pnpm test --run
pnpm build
git diff --check
```

Manual smoke required before marking DONE:

1. Invalid key rejected before persistence for at least OpenAI and one non-OpenAI
   provider.
2. Valid key end-to-end polish for at least two provider families. If real keys
   are unavailable, mark plan `NEEDS-SMOKE`, not `DONE`.
3. Provider timeout leaves raw transcript delivered/saved.
4. Provider switch restores per-provider model.
5. Fresh app build launches without the formatting sidecar process/package.

## Done criteria

ALL must hold:

- [ ] VoiceTypr-owned `AiProvider`/`AiModel`/`AiPolishRequest`/
      `AiPolishResult`/`AiProviderError` types exist and no SDK type crosses
      Tauri/frontend/settings boundaries.
- [ ] Runtime choice is documented in code/report with evidence from the spike:
      `genai` stable, `genai` + direct custom adapter, or AISDK if it wins.
- [ ] Provider/model catalog is generated from external metadata, committed,
      deterministic, and tested.
- [ ] API-key save path validates before secure-store persistence and before
      settings writes.
- [ ] Custom OpenAI-compatible base URL/model/no-auth settings persist only
      after successful validation.
- [ ] Rust-native polish supports launch priority providers: OpenAI,
      Anthropic, Gemini, OpenRouter, Groq, xAI, and Custom OpenAI-compatible,
      or the report identifies exactly which provider hit a STOP condition.
- [ ] Retry/cancel/timeout behavior is owned by VoiceTypr and covered by tests.
- [ ] Polish failure cannot discard the raw transcript.
- [ ] Provider/model UI reads from the catalog, supports broad searchable
      API-key providers, and remembers model per provider.
- [ ] Pi/Node formatting sidecar is removed from normal build/runtime before
      DONE.
- [ ] Full verification gates pass.
- [ ] Manual smoke is performed or the row is marked `NEEDS-SMOKE` with exact
      missing provider credentials/scenario.
- [ ] `plans/README.md` status row updated.

## STOP conditions

Stop and report back if:

1. Neither `genai` stable nor a small direct `reqwest` adapter can support the
   launch priority provider matrix without env-var auth, unbounded timeout, or
   untyped errors. Report the exact failing provider and why AISDK is or is not
   a better runtime.
2. AISDK is required for runtime coverage but materially weakens timeout,
   cancellation, or error control. Do not trade runtime robustness for catalog
   breadth.
3. Adding the chosen SDK introduces a dependency/build problem on Windows or
   macOS that cannot be explained by local environment.
4. The generated catalog is too large/noisy to ship cleanly. Stop and propose a
   deterministic filtered catalog rule rather than hand-curating one-off lists.
5. Provider validation endpoints reject documented valid credentials or cannot
   validate without making a paid text generation call. Do not ship validation
   that rejects good keys.
6. Removing the sidecar reveals another production feature still depends on it.
   Report the dependency and split only if necessary.
7. No-transcript-loss requires touching Plan 015's recording/transcription path
   in a way that conflicts with current in-flight work. Coordinate before
   editing that path.

## Maintenance notes

- The old rule-engine boundary/performance fixes are still worthwhile but are
  not part of this provider-runtime plan. Put them in a separate writing-engine
  plan if they remain launch blockers.
- Future CLI/agent work should reuse these exact Rust contract types and error
  codes. Do not invent a separate CLI AI schema.
- Future STT cloud providers should follow the same shape but live under the
  transcription contract, not this text-polish module.
- Catalog updates should be explicit and reviewable: regenerate, inspect diff,
  run catalog tests, then commit.
