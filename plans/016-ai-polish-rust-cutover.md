# Plan 016: AI polish Rust-native cutover — current providers, remove Pi sidecar

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan in
> `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat b1a66bf..HEAD -- src-tauri/Cargo.toml src-tauri/build.rs src-tauri/src/commands/ai.rs src-tauri/src/formatting src-tauri/src/lib.rs src-tauri/src/writing.rs src-tauri/src/commands/audio.rs src/components/sections/EnhancementsSection.tsx src/utils/keyring.ts src/hooks/useProviderModels.ts src/types/providers.ts sidecar/formatting-engine package.json pnpm-workspace.yaml pnpm-lock.yaml`
>
> On drift, compare the "Current state" excerpts to live code first. If the
> sidecar command protocol, settings keys, or AI settings UI has changed,
> update this plan before writing code.

## Status

- **Priority**: P1
- **Effort**: M-L
- **Risk**: MED (runtime cutover for every AI-polished dictation, but scope is
  limited to the providers users already have)
- **Depends on**:
  - Plan 015 manual smoke is a **precondition for shipping** the executor
    cutover (the no-transcript-loss recovery path is shared). Spike, contract,
    validation, and migration work proceed before 015 smoke completes — only
    release/DONE is gated.
  - Does NOT depend on 014. If 014 lands first, apply the executor at the
    shared transcription seam instead.
- **Category**: reliability / architecture
- **Planned at**: commit `b1a66bf`, 2026-06-11; revised same day after
  independent strategy + SDK source review; amended after final pre-execution
  validation (citation audit + executability review).
- **Replaces**: prior 016 drafts. Catalog breadth → Plan 017; extra providers →
  Plan 018.

## Decision

Ship reliability before breadth:

```txt
016 (this plan)  Rust executor for OpenAI / Anthropic / Gemini / Custom,
                 existing UI shape, validate-before-save, settings migration,
                 raw-transcript fallback, Pi sidecar removed from build.
017              Generated provider/model catalog + searchable breadth UI.
018              Graduate OpenRouter / Groq / xAI to production per-provider.
```

Architecture:

```txt
Enhancements UI / settings / CLI later
        |
        v
Voicetypr AI provider contract
  - AiProvider / AiModel
  - AiPolishRequest / AiPolishResult
  - AiProviderError
        |
        +--> genai =0.6.0 (exact pin): OpenAI, Anthropic, Gemini
        |
        +--> direct reqwest adapter: Custom OpenAI-compatible
```

The UI must not know which runtime executes a provider. No SDK type crosses
Tauri command boundaries, settings, history records, or frontend TypeScript
types.

## Evidence (source-verified 2026-06-11)

| Finding | Consequence |
|---|---|
| genai 0.6 `AuthResolver`/`AuthData::Key` resolves the API key per call from a `'static` closure — no env vars. | Key lookup from Voicetypr's secure store works. Closure must capture an `Arc` key-store handle, not borrowed UI state. |
| genai has **no per-call timeout**; `WebConfig::with_timeout` is reqwest client-level only. | Wrap every polish call in `tokio::time::timeout` + cancellation token. Mandatory. |
| genai errors are **not semantically typed**: provider HTTP failures surface as `webc::Error::ResponseFailedStatus { status, body, headers }` / `Reqwest(reqwest::Error)` nested in `genai::Error`. | Voicetypr owns an error mapper: 401/403 → `InvalidApiKey`, 429 → `RateLimited`, 5xx → `ServiceUnavailable`, reqwest timeout/connect → `Timeout`/`Network`. Never show raw SDK errors. |
| `AdapterKind::Custom` does **not exist in genai 0.6 stable** (only 0.7-beta, env-var driven). | Custom OpenAI-compatible uses the direct reqwest adapter. Do not adopt the beta. |
| genai emits `reasoning_effort` only for the **native OpenAI adapter**; Anthropic/Gemini have native mappings; OpenAI-compatible gateways silently drop it. | Runtime maps reasoning effort for OpenAI/Anthropic/Gemini native adapters only, omitting it elsewhere. **No reasoning UI in this plan** (see Step 6). |
| genai 0.5→0.6 was a large breaking release; single dominant maintainer. | Pin **exactly** `genai = "=0.6.0"`. The contract is the swap seam. |
| aisdk 0.5.2: heavy churn, 2-person bus factor; its codegen just fetches models.dev. | Not a runtime here. Codegen reference for Plan 017 only. |

## Reliability bar

1. **No big-bang cutover**: the Pi sidecar path stays buildable locally until
   the Rust path passes the provider matrix, mocked failure tests, and manual
   smoke items 1-4. Removal happens in Step 7 only after that parity gate.
2. **No silent downgrade**: if Rust polish fails, the user gets the raw
   transcript plus a polish-failed notice. Never drop text, never paste an
   empty or partial provider response, never pretend polish succeeded.
3. **No unbounded provider calls**: one total budget per request, cancellation,
   bounded retry — all owned by Voicetypr.
4. **No dependency lock-in**: UI/settings/history store only Voicetypr contract
   fields.
5. **Non-streaming only**: partial output is never pasted; discarded on
   cancel/failure.
6. **No launch without real smoke**: at least two real provider families
   end-to-end with valid keys before `DONE`.

## Current state (citation-audited 2026-06-11)

### A. Pi sidecar is the runtime

- `sidecar/formatting-engine/package.json:11-13,21-22` — `@earendil-works/pi-ai`
  `^0.79.1`, Node `>=22.19.0`.
- `src-tauri/build.rs:44-55,80-107,225-232` — builds/ensures the SEA sidecar,
  reruns on sidecar inputs.
- Sidecar bundled in **four** configs: `tauri.conf.json:49-54`,
  `tauri.dev.conf.json:50-53`, `tauri.macos.conf.json:7-13`,
  `tauri.windows.conf.json:8-14`.
- `src-tauri/src/lib.rs:388` manages `FormattingClient`; `lib.rs:525` calls
  `warm_formatting_sidecar_if_ai_enabled` (`lib.rs:1410-1463`); imports
  `FormattingCommand`/`PROTOCOL_VERSION` at `lib.rs:15-21`.
- `enhance_transcription_internal` sends `FormattingCommand::Format`
  (`src-tauri/src/commands/ai.rs:1165-1181`). `list_ai_providers`
  (`ai.rs:1394-1434`) and `list_provider_models` (`ai.rs:1460-1527`) also call
  `FormattingClient`.
- Sidecar timeout is an outer process-hang bound; Node owns the 30s provider
  budget (`src-tauri/src/formatting/sidecar.rs:15-17,151-157`).
- Capabilities allow formatting-sidecar spawn/stdin on macOS
  (`capabilities/macos.json:32-45`) and Windows
  (`capabilities/windows.json:32-40`); not in `default.json`.

### B. Product surface is already four providers

- `src/types/providers.ts:21-47` — OpenAI, Gemini, Anthropic, Custom (that
  order; order is irrelevant to this plan). Imported as `AI_PROVIDERS` by
  `EnhancementsSection.tsx:33`; drives startup key/model loops at
  `EnhancementsSection.tsx:122-157`.
- Backend validation: OpenAI/custom (`ai.rs:509-562`), Gemini (`563-592`),
  Anthropic (`593-623`), `_ => Err("Unsupported provider")` (`624`). Command is
  `validate_ai_api_key` (`ai.rs:489-626`), registered at `lib.rs:1311-1315`.
- Key warm-up is hardcoded in **two** places: frontend
  `src/utils/keyring.ts:87-100` (`['gemini','openai','anthropic','custom']`)
  and backend `AI_PROVIDER_KEYS` (`ai.rs:35-41`), warmed at `lib.rs:520-523`.
- Pi-era provider ID mapping: `sidecar_provider_for("gemini") => "google"` and
  inverse (`ai.rs:1354-1370`).
- Model loading hook: `src/hooks/useProviderModels.ts:37-45,88-96` calls
  `list_provider_models`; custom is skipped at `23-27,74-78`.
- Settings keys: `ai_enabled`, `ai_provider`, `ai_model`,
  `ai_models_by_provider` (`ai.rs:776-779`); `ai_custom_base_url`,
  `ai_custom_no_auth`, legacy `ai_openai_base_url`, `ai_openai_no_auth`
  (`ai.rs:29-33,1251-1292`). Secure-store keys: `ai_api_key_{provider}`
  (`keyring.ts:51-55`, `ai.rs:457-458`). There is **no** legacy
  `ai_api_key_google` — do not invent one.

### C. Fallback + events today (the gap Step 4 must close)

- On formatting failure during desktop dictation, raw text is copied to
  clipboard and saved to history, but `should_deliver=false` **prevents
  insertion** (`src-tauri/src/commands/audio.rs:3775-3872,3892-3895`). That
  does not meet this plan's "delivered + saved + notice" invariant.
- `writing::process_transcription` silently swallows many AI failures and
  returns deterministic library text with only a log warning
  (`src-tauri/src/writing.rs:1857-1876`).
- Polish progress events already exist and the pill consumes them:
  `enhancing-started` (`audio.rs:3733-3735`), `enhancing-completed` /
  `enhancing-failed` (`audio.rs:3756-3780`), listener in
  `src/components/pill/usePillController.ts:9-13,65-85`. **These event names
  must be preserved.**
- Upload/test transcription paths also call enhancement
  (`audio.rs:4680-4684,4953-4957`) and need the same fallback invariant.

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Rust compile | `cd src-tauri && cargo check` | exit 0 |
| Rust tests | `cd src-tauri && cargo test` | all pass |
| Rust lint/format | `cd src-tauri && cargo clippy -- -D warnings && cargo fmt --check` | exit 0 |
| Frontend gates | `pnpm typecheck && pnpm lint && pnpm test --run` | all pass |
| Build | `pnpm build` | exit 0 |
| Diff hygiene | `git diff --check` | clean |

## Scope

**In scope**: `src-tauri/src/ai/*` (extend — module already exists with
`mod.rs`, `openai.rs`, `prompts.rs`, `tests.rs`; preserve those),
`src-tauri/src/commands/ai.rs`, `src-tauri/src/lib.rs` wiring,
`src-tauri/src/commands/audio.rs` + `src-tauri/src/writing.rs` fallback
invariant, `src/utils/keyring.ts`, `src/components/sections/EnhancementsSection.tsx`,
`src/hooks/useProviderModels.ts`, `src/types/providers.ts` (frontend consumes
backend DTO), settings migration, sidecar removal (build.rs, 4 Tauri configs,
2 capability files, package script, workspace, lockfiles), tests.

**Out of scope**: generated catalog + breadth UI (017); new providers (018);
reasoning-effort **UI** (deferred — runtime omits unsupported fields; no new
control in this plan); local LLM inference; cloud STT; writing rule-engine
boundary fixes beyond the fallback invariant; Plan 012/014 implementation.

## Contract (full definitions — the single source of truth)

Serialization MUST match the serde convention already used by the existing AI
command DTOs in `commands/ai.rs` (check the existing `list_provider_models`
response shape at implementation time and follow it consistently).

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProvider {
    pub id: String,            // "openai" | "anthropic" | "gemini" | "custom"
    pub label: String,         // display name
    pub requires_api_key: bool,// custom may be no-auth
    pub supports_base_url: bool, // true only for custom
    pub supports_reasoning: bool, // native adapter mapping exists
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModel {
    pub provider_id: String,
    pub model_id: String,
    pub label: String,
    pub recommended: bool,
}

#[derive(Debug, Clone)]
pub struct AiPolishRequest {
    pub provider_id: String,
    pub model_id: String,
    pub input_text: String,
    pub prompt: String,
    pub timeout_ms: u64,
    pub reasoning_effort: Option<AiReasoningEffort>, // runtime-only; no UI yet
}

#[derive(Debug, Clone, Serialize)]
pub struct AiPolishResult {
    pub output_text: String,
    pub provider_id: String,
    pub model_id: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiReasoningEffort { Low, Medium, High }

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AiProviderError {
    #[error("missing API key")]        MissingApiKey,
    #[error("invalid API key")]        InvalidApiKey,
    #[error("invalid model")]          InvalidModel,
    #[error("unsupported provider")]   UnsupportedProvider,
    #[error("timed out")]              Timeout,
    #[error("canceled")]               Canceled,
    #[error("rate limited")]           RateLimited,
    #[error("service unavailable")]    ServiceUnavailable,
    #[error("network error")]          Network,
    #[error("bad response")]           BadResponse,
    #[error("internal error")]         Internal,
}
```

Rules: stable string IDs in settings/history; no SDK types cross this boundary;
error variants map to fixed user-facing strings; `AiProvider`/`AiModel` are the
DTOs returned by `list_ai_providers`/`list_provider_models` — the frontend
consumes these instead of its own static table (Plan 017 swaps only the
backend source behind the same DTOs).

## Runtime policy

- One `reqwest::Client` built by Voicetypr, injected via
  `ClientBuilder::with_reqwest` — same TLS/proxy stack on both runtime paths.
- Total budget enforced by `tokio::time::timeout` around the whole polish
  future; cancellation token aborts the wait.
- Retry at most once for 429 / connection-reset / 5xx, within remaining budget,
  honoring `Retry-After` if it fits. Never retry auth/model/validation errors
  or cancellations.
- Non-streaming `exec_chat` only. Empty/whitespace output → `BadResponse`.
- No transcript or provider response content in logs.

### Latency budget

Target p50 ≤ 2.5s, p95 ≤ 8s on fast models. Default total budget stays at the
current product timeout. The existing `enhancing-started/completed/failed`
events keep driving the pill's progress state. Record duration + outcome
category in diagnostics (no content).

### Shutdown semantics

On app quit / window destroy with polish in flight: cancel the request, do not
emit to destroyed windows, do not write partial history/settings, do not leak a
task holding `AppState`.

## Steps

### Step 1: Runtime spike (genai =0.6.0 + direct reqwest)

Do not remove Pi yet. The spike is **committed test code**, not throwaway:
mocked-HTTP integration tests living under `src-tauri/src/ai/` (extend the
existing `tests.rs` or add `runtime_tests.rs`), using a local mock HTTP server
(reuse whatever the repo's existing Rust tests use; add `wiremock` as a
dev-dependency only if nothing exists).

1. Add `genai = "=0.6.0"` to `src-tauri/Cargo.toml` (exact pin).
2. Prove with mocked HTTP, for OpenAI/Anthropic/Gemini (genai) and Custom
   (direct reqwest):
   - API key from an `Arc`-captured resolver, no env vars;
   - Voicetypr-owned total timeout + cancel (verify the request aborts);
   - error mapping to `AiProviderError` from 401/403/429/5xx/network/timeout;
   - reasoning effort emitted for the three native adapters, omitted for
     custom.
3. **Pass criteria are mocked-HTTP criteria.** Real-key proof belongs to Step 8
   smoke, not this gate.

**Verify**: `cd src-tauri && cargo check && cargo test ai`.

**Proceed only if** all four launch providers pass the mocked matrix.
Groq/xAI/OpenRouter are explicitly NOT gates (Plan 018).

### Step 2: Contract module

Extend the existing `src-tauri/src/ai/` module — preserve `openai.rs`,
`prompts.rs`, `tests.rs`, and existing `EnhancementOptions` export — adding
`contract.rs`, `error.rs`, `executor.rs`, `genai_runtime.rs`,
`openai_compatible.rs`, `providers.rs` (static four-entry table) to `mod.rs`.
Use the full contract definitions above verbatim (field names included). Unit
tests assert error categories, not provider debug strings.

**Verify**: `cd src-tauri && cargo test ai`.

### Step 3: Validation before save

1. **Replace `validate_ai_api_key` in place** (`ai.rs:489-626`). Keep the
   command name — no new `validate_ai_provider`, no alias, no shim. New
   semantics: pure validation through the contract runtime; no settings writes,
   no key caching inside validation.
2. Per-provider policy: model-list or minimal probe; custom validates base URL
   + model + auth/no-auth in one probe. UI copy distinguishes "invalid key"
   from "valid key, model unavailable".
3. `src/utils/keyring.ts:46-61` save order: validate → keyring write → backend
   cache → settings persist, for ALL providers.
4. `EnhancementsSection.tsx:734-754`: keep the existing validate-before-
   `set_openai_config` ordering for custom; extend the same invariant to
   no-auth and model changes.
5. Startup cache-fill stays offline-tolerant (saved keys load without live
   validation).

**Verify**: `cd src-tauri && cargo test ai`; `pnpm typecheck`.

### Step 4: Rust executor cutover + fallback invariant

1. Route `enhance_transcription_internal` (`ai.rs:1165-1181`) through the Rust
   executor. **Also** reimplement `list_ai_providers` (`ai.rs:1394-1434`) and
   `list_provider_models` (`ai.rs:1460-1527`) over the contract table/runtime
   in this step — after it, no command path touches `FormattingClient`.
2. Remove `sidecar_provider_for`/`voice_provider_for_sidecar` usage from the
   request path (migration in Step 5 owns the `google`→`gemini` settings
   rewrite).
3. **Fallback invariant** (this closes the audited gap): on any polish
   failure/timeout/cancel/empty-response, the raw transcript is **delivered to
   the paste/insertion path**, saved to history, AND a visible notice is shown.
   Amend `audio.rs:3775-3895` (currently `should_deliver=false`) and the silent
   fallback in `writing.rs:1857-1876` so failures surface as the typed category
   + notice instead of being swallowed. Apply the same invariant to the
   upload/test paths (`audio.rs:4680-4684,4953-4957`).
4. Preserve event names `enhancing-started` / `enhancing-completed` /
   `enhancing-failed` exactly (pill: `usePillController.ts:9-13,65-85`).

**Verify**: mocked-HTTP tests — success, invalid key, invalid model, 429
retry-once, 5xx retry-once, timeout, cancellation, empty response — plus one
invariant test per entry path: polish failure ⇒ raw transcript delivered +
history-saved + failure event emitted.

### Step 5: Settings + migration

1. Migration runs at startup **before** `warm_ai_key_cache_from_secure_store`
   (`lib.rs:520-523`). One-way, idempotent, logged without content:
   - `ai_provider == "google"` → `"gemini"`;
   - `ai_models_by_provider["google"]` → merged into `["gemini"]` (existing
     `gemini` entry wins);
   - `ai_model` preserved iff valid for the migrated provider per the contract
     table; otherwise cleared and surfaced in UI (no silent provider/model
     substitution);
   - custom base/no-auth keys: keep `ai_custom_*`; migrate legacy
     `ai_openai_base_url`/`ai_openai_no_auth` per existing rules
     (`ai.rs:1251-1292`);
   - no legacy `ai_api_key_google` secure key exists — do not create or read
     one.
2. Replace **both** hardcoded warm-up lists — backend `AI_PROVIDER_KEYS`
   (`ai.rs:35-41`) and frontend `keyring.ts:87-100` — with lists derived from
   the contract provider table (frontend gets it via `list_ai_providers`).
3. If the saved provider is invalid after migration, surface it in the UI —
   never silently fall back to a different provider.

**Verify**: Rust unit tests for the migration table; `pnpm test --run` for
settings hooks.

### Step 6: Existing-UI pass (no new picker UX, no reasoning UI)

1. `EnhancementsSection.tsx` provider cards and startup key/model loops
   (`:33,122-157`) consume `list_ai_providers` instead of the static
   `AI_PROVIDERS` import; remove the TS table from `src/types/providers.ts`
   once no consumer remains (clean cutover — no re-export).
2. `useProviderModels.ts` keeps calling `list_provider_models` (now
   contract-backed); custom stays free-text.
3. Per-provider model memory restores on switch (existing
   `ai_models_by_provider` behavior, now migration-safe).
4. **No reasoning control is added.** The runtime omits unsupported reasoning
   fields; UI work for reasoning is deferred past 017.

**Verify**: `pnpm typecheck && pnpm test --run`.

### Step 7: Remove Pi sidecar from the build

Parity gate first: Steps 1-6 verified AND manual smoke items 1-4 pass.

Cleanup checklist (audited; complete all):

1. `src-tauri/build.rs:25-107,225-232` — remove formatting-sidecar build/ensure
   functions, call, and rerun lines.
2. All four Tauri configs' `externalBin` entries (`tauri.conf.json:49-54`,
   `tauri.dev.conf.json:50-53`, `tauri.macos.conf.json:7-13`,
   `tauri.windows.conf.json:8-14`).
3. Capabilities: formatting-sidecar spawn/stdin in `macos.json:32-45` and
   `windows.json:32-40`. Keep ffmpeg/parakeet/whisper-vulkan permissions.
   **Do not remove `tauri-plugin-shell`** — other sidecars use it.
4. `lib.rs`: `FormattingClient` manage (`388`), warmup call+fn
   (`525`, `1410-1463`), `FormattingCommand`/`PROTOCOL_VERSION` imports
   (`15-21`), `formatting` module decl.
5. `commands/ai.rs`: remaining `FormattingCommand`/`FormattingResponse`/
   `FormattingClient`/request-counter/`sidecar_provider_for`/
   `voice_provider_for_sidecar` code; `src-tauri/src/formatting/` module.
6. Root `package.json:11-14` `sidecar:build-formatting` script;
   `pnpm-workspace.yaml:1-4` entry; delete `sidecar/formatting-engine`;
   regenerate lockfile via the package manager.
7. Reference sweep (must return zero hits in shipped code):
   `formatting-engine`, `formatting-sidecar`, `FormattingClient`,
   `FormattingCommand`, `FormattingResponse`, `PROTOCOL_VERSION` (formatting
   one), `@earendil-works/pi-ai`, `sidecar:build-formatting`.

**Verify**: `pnpm build`; `cd src-tauri && cargo check`; built app contains no
formatting sidecar binary.

### Step 8: Full gates + smoke

```bash
cd src-tauri && cargo test
cd src-tauri && cargo clippy -- -D warnings && cargo fmt --check
pnpm typecheck && pnpm lint && pnpm test --run
pnpm build
git diff --check
```

Manual smoke (required before DONE; otherwise `NEEDS-SMOKE` with the exact
missing scenario):

1. Invalid key rejected before persistence: OpenAI + one non-OpenAI provider.
2. Valid-key end-to-end polish: two real provider families.
3. Provider timeout → raw transcript delivered/saved + notice; app responsive.
4. Custom base URL with bad endpoint does not persist.
5. Provider switch restores per-provider model.
6. Quit app mid-polish: no crash, no half-written history/settings.
7. Fresh build launches and polishes with no formatting sidecar present.
8. Windows build smoke (one real polish call exercising TLS/proxy). If no
   Windows machine/runner is available, mark `NEEDS-SMOKE` naming this item.

## Done criteria

ALL must hold:

- [ ] Contract types exactly as specified; no SDK type crosses boundaries.
- [ ] `genai = "=0.6.0"`; custom on direct reqwest; spike evidence recorded.
- [ ] Voicetypr-owned timeout/cancel/retry/error mapping covered by the mocked
      failure matrix.
- [ ] `validate_ai_api_key` replaced in place (pure validation); save order
      validate → keyring → cache → settings for all four providers.
- [ ] Frontend consumes `list_ai_providers`/`list_provider_models` DTOs; static
      TS provider table removed.
- [ ] Migration implemented per Step 5 rules; both warm-up lists derived from
      the provider table.
- [ ] Fallback invariant holds on all entry paths: failure ⇒ raw transcript
      **delivered** + saved + notice; events preserved; non-streaming only.
- [ ] Shutdown semantics implemented.
- [ ] Pi/Node sidecar fully removed per the Step 7 checklist; sweep clean.
- [ ] Plan 015 smoke completed before the cutover ships.
- [ ] Full gates pass; manual smoke done or `NEEDS-SMOKE`.
- [ ] `plans/README.md` updated.

## STOP conditions

1. Any of the four launch providers cannot pass the Step 1 mocked matrix with
   Voicetypr-owned auth/timeout/errors. (Groq/xAI/OpenRouter never stop this
   plan.)
2. `genai =0.6.0` introduces a Windows or macOS build/TLS problem not explained
   by local environment.
3. Provider validation cannot distinguish invalid auth without a paid
   generation call, and the minimal-probe cost is unacceptable — report rather
   than ship validation that rejects good keys.
4. Sidecar removal reveals a production dependency outside AI polish.
5. The fallback invariant conflicts with in-flight Plan 015 work in
   `commands/audio.rs` — coordinate before editing shared regions.

## Maintenance notes

- Plan 017 swaps the static provider table for the generated catalog behind the
  same DTOs — nothing outside `providers.rs` may assume "exactly four".
- Plan 018 graduates additional providers with its own per-provider acceptance.
- Future CLI/agent work reuses these contract types and error codes verbatim.
- Residual debt to carry into 017/018 (tracked, intentional):
  - `src-tauri/src/ai/contract.rs` has `#[allow(dead_code)]` on
    `AiReasoningEffort` — reasoning UI is deferred; the runtime already omits
    unsupported reasoning fields.
  - Tauri wire DTOs are compatibility shapes `{id,name}` /
    `{id,name,recommended}`; the full `AiProvider`/`AiModel` contract lives
    backend-side. Keep the wire shapes stable when 017 swaps the source.
  - `set_openai_config`/`get_openai_config` now configure the custom
    OpenAI-compatible endpoint (legacy wire naming) — document, don't rename
    mid-flight.
  - `src/types/providers.ts` `PROVIDER_UI_METADATA` is four-provider UI
    metadata (colors/key URLs), not a provider source of truth; 017 replaces it
    with catalog/overlay-driven metadata or generic fallbacks.
