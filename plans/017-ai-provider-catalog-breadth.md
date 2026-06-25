# Plan 017: AI provider catalog + searchable breadth UI

> **Executor instructions**: Follow this plan step by step. Run every
> verification command before moving on. Stop on any STOP condition. Update the
> status row in `plans/README.md` when done.
>
> **Prerequisite**: Plan 016 (Rust-native cutover) is DONE or NEEDS-SMOKE with
> the executor merged. This plan must not touch the runtime/executor — it only
> replaces the provider/model *source* and the picker UI.
>
> **Drift check (run first)**:
> `git diff --stat <016-merge-commit>..HEAD -- src-tauri/src/ai src-tauri/src/commands/ai.rs src/components/sections/EnhancementsSection.tsx src/utils/keyring.ts`

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW-MED (metadata + UI; runtime untouched)
- **Depends on**: 016
- **Category**: feature breadth / UX
- **Planned at**: 2026-06-11

## Decision

Replace Plan 016's static four-entry provider table with a **generated,
committed catalog** and a broad, searchable provider/model picker — behind the
same Voicetypr contract types. No runtime changes.

Catalog source: pinned snapshot of `models.dev/api.json` (MIT, hourly-synced
upstream) + a Voicetypr **overlay** file. Source-verified findings that shape
this design:

- models.dev provider schema has `env` (auth env vars), `npm`, optional `api`
  base URL, and per-model `reasoning`, `reasoning_options`, `tool_call`,
  `structured_output`, `limit` (context/output), `cost`, dates.
- The `env` field signals "needs a key" but does NOT distinguish pure API-key
  providers from OAuth/subscription/proxy providers. The overlay must classify
  auth explicitly.
- `lazy-hq/aisdk`'s `scripts/provider-codegen.py` proves the metadata is
  consumable standalone; we write our own small generator. aisdk itself is NOT
  a dependency.

## Deliverables

### 1. Generator + committed artifact

- Deterministic generator (script or `cargo xtask`-style helper) reading a
  **pinned snapshot** (committed `api.json` copy with source date) + overlay.
- Output: one committed catalog artifact consumed by `src-tauri/src/ai/`
  (replacing `providers.rs`'s static table behind the same types).
- Overlay fields per provider: auth classification
  (`api_key` / `oauth_or_proxy` / `local_no_auth` / `unsupported`), runtime
  mapping (`genai_adapter` / `openai_compatible` / `unsupported`), status
  (`production` / `experimental` / `hidden`), disabled reason.
- Generation-time filter: API-key text-output providers only; stable sort;
  reviewable diffs. No runtime network fetch of the global catalog.
- The four Plan 016 providers are `production`; everything else enters as
  `experimental` or `hidden` (graduation = Plan 018).

### 1b. Runtime mapping generalization (required seam change)

Breadth is impossible without this, and the original "no executor changes"
rule was wrong. Today `src-tauri/src/ai/genai_runtime.rs::adapter_kind_for_provider`
hardcodes only `openai`/`anthropic`/`gemini`, and `src-tauri/src/ai/executor.rs`
dispatches on `is_native_provider() || PROVIDER_CUSTOM`. To run any catalog
provider these must become **data-driven**: map each provider's overlay
`runtime` field (`genai_adapter:<kind>` / `openai_compatible` / `unsupported`)
to the genai `AdapterKind` or the direct-reqwest path, and dispatch by that
mapping rather than a hardcoded list.

- ALLOWED in this plan: generalizing the adapter-kind lookup and the executor
  dispatch to read the catalog runtime field; adding per-provider base-URL /
  namespace metadata needed to route OpenAI-compatible gateways.
- FORBIDDEN in this plan: changing the runtime **reliability policy** — total
  timeout, retry classification, cancellation, error mapping. Those stay
  exactly as 016 shipped. A provider that needs different reliability handling
  graduates via Plan 018, not here.

### 2. Catalog tests

- provider IDs unique; model IDs unique per provider;
- every `production`/`experimental` provider maps to a runtime adapter;
- no `oauth_or_proxy`/`unsupported` provider in the default API-key list;
- recommended models resolve to existing catalog entries;
- snapshot regeneration is a no-op when inputs unchanged (determinism test).

### 3. Picker UI

- Default list: API-key providers, grouped Recommended / Popular / All.
- Search across providers and models.
- `experimental` providers visibly badged; `hidden` not shown without an
  Advanced toggle.
- Model rows show dictation-relevant hints where data exists (speed/cost/
  reasoning support).
- Per-provider model memory continues to work for all catalog providers.

### 4. Catalog refresh procedure (docs)

Documented manual flow: update snapshot → regenerate → review diff → run
catalog tests → commit. Never part of app startup.

## Verification

- `cd src-tauri && cargo test ai`
- `pnpm typecheck && pnpm lint && pnpm test --run`
- `pnpm build`
- `git diff --check`
- Manual smoke: search finds a non-recommended API-key provider; selecting an
  `experimental` provider shows the badge and works or fails with a clear
  typed error; the four production providers behave exactly as in Plan 016.

## Done criteria

- [ ] Committed generated catalog + overlay + generator, deterministic, tested.
- [ ] Static four-provider table replaced; contract types unchanged.
- [ ] Picker UI grouped/searchable; experimental badged; hidden gated.
- [ ] Runtime mapping/dispatch is data-driven (deliverable 1b); NO change to
      executor timeout/retry/cancel/error-mapping policy in the diff.
- [ ] Full gates pass; manual smoke done or `NEEDS-SMOKE`.
- [ ] `plans/README.md` updated.

## STOP conditions

1. The filtered catalog is still too large/noisy to review — propose a tighter
   deterministic filter rule; do not hand-curate one-off lists.
2. models.dev schema drift breaks the generator — pin to the committed snapshot
   and report; do not chase upstream mid-plan.
3. A provider needs changes to the runtime **reliability policy** (timeout,
   retry, cancellation, error mapping) — that belongs to 016/018; stop and
   report. (Generalizing adapter mapping/dispatch per deliverable 1b is
   expected and allowed.)
