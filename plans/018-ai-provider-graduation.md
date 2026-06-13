# Plan 018: AI provider graduation — OpenRouter, Groq, xAI

> **Executor instructions**: Follow per-provider. A provider graduates
> independently; do not batch-graduate. `plans/README.md` keeps ONE row for
> 018; record per-provider state inside that row's Status cell, e.g.
> `OpenRouter DONE / Groq TODO / xAI TODO`.
>
> **Prerequisite**: Plans 016 and 017 merged (contract + executor + catalog).
>
> **DROPPED (2026-06-13, per user)**: this plan is cancelled. xAI, then Groq and
> OpenRouter (and the briefly-added DeepSeek/Cohere), were all removed from the
> catalog. Rationale: they are OpenAI-compatible endpoints already reachable via
> the **Custom** provider (017), so there is nothing distinct to graduate. The
> AI-polish surface is OpenAI/Anthropic/Gemini + Custom. Kept for history only.

## Status

- **Priority**: P2 (OpenRouter), P3 (Groq, xAI)
- **Effort**: S-M per provider
- **Risk**: LOW (additive; production set untouched until a provider passes)
- **Depends on**: 016, 017
- **Category**: feature breadth
- **Planned at**: 2026-06-11

## Decision

New providers ship `experimental` (Plan 017 catalog status) and move to
`production` only after passing the per-provider acceptance below. Order:

1. **OpenRouter** first — one key unlocks broad model access; highest user
   value per unit of work.
2. **Groq**, **xAI** after.

Source-verified constraints that shape the work:

- genai 0.6 routes Groq/xAI/OpenRouter through the shared OpenAI-compatible
  adapter and **silently drops `reasoning_effort`** for them (native-OpenAI
  only). Reasoning control for these providers requires `ChatOptions.extra_body`
  with provider-specific payloads — or stays unsupported. Never send fake
  defaults.
- genai 0.6 requires namespaced model addressing for some adapters (e.g.
  `groq::model`, `open_router::` namespace). The catalog runtime mapping must
  encode this; the contract's `provider_id`/`model_id` strings stay clean.
- Provider `/models` listings do not prove a key can use a given model
  (entitlement vs auth). Validation copy must distinguish the two.

## Per-provider acceptance (all required to graduate)

- [ ] Validation policy implemented: invalid key vs valid-key-model-unavailable
      distinguished; no paid generation call needed for routine validation, or
      the minimal probe cost is documented and accepted.
- [ ] Error mapping verified against real responses: 401/403, 404 model,
      429 (+ `Retry-After`), 5xx, network — each lands in the right
      `AiProviderError` category with correct user copy.
- [ ] Timeout/cancel verified under the VoiceTypr budget (no hangs on slow
      gateway responses).
- [ ] Retry classification verified (429/5xx once within budget; never on
      auth/model errors).
- [ ] Reasoning effort: wired via verified `extra_body` payload, or explicitly
      omitted with the control hidden for that provider.
- [ ] Model mapping/namespacing covered by unit tests.
- [ ] Real-key end-to-end smoke: polish round-trip, raw-transcript fallback on
      forced failure.
- [ ] Catalog overlay flipped `experimental` → `production`; picker badge
      removed.

## Verification (per provider)

- `cd src-tauri && cargo test ai`
- `pnpm typecheck && pnpm test --run`
- Manual smoke with a real key (the provider does not graduate without it; if
  no key is available, it stays `experimental` — that is an acceptable end
  state, not a failure).

## STOP conditions

1. A provider's gateway behavior (streaming-only quirks, nonstandard errors,
   entitlement opacity) cannot meet the acceptance list — leave it
   `experimental` with a documented reason; do not weaken the acceptance bar
   to graduate it.
2. Any change leaks provider-specific types/behavior across the VoiceTypr
   contract boundary — stop and redesign within the adapter.
