# Plan 019: Cloud STT shortlist — Deepgram, OpenAI, Groq, Cohere Transcribe

> **Executor instructions**: This plan replaces the Soniox-only cloud-STT
> special case with a data-driven cloud-STT provider seam, then registers four
> new curated providers. Clean cutover: no `soniox`-specific commands, helpers,
> or snapshot fields survive. Run every verification command before yielding.
>
> **Baseline commit**: `fb09a61` (branch `plan/v2-roadmap`).
> **Supersedes session decision**: "replace xAI with Cohere Transcribe" — the
> curated cloud-STT shortlist is Soniox, Deepgram, OpenAI, Groq, Cohere
> Transcribe (xAI dropped).

## Status

- **Priority**: P1 (product breadth the user requested)
- **Effort**: L
- **Risk**: MED — touches recording engine resolution, model catalog, settings
  normalization, recognition availability snapshot (shared FE/BE shape), tray,
  and sharing rejection. Mitigated by: existing Soniox pattern, a single new
  `cloud_stt` module owning all provider behavior, and a snapshot field rename
  done atomically across BE+FE.
- **Depends on**: — (independent of AI-polish plans 016/017/018)
- **Category**: feature breadth / cloud transcription
- **Planned at**: 2026-06-12

## Decision

Today cloud STT is a Soniox-only special case duplicated across ~12 backend and
~12 frontend sites. Adding four providers Soniox-style would multiply that
hardcoding. Instead introduce one backend provider seam — `src-tauri/src/cloud_stt/`
— mirroring the existing frontend `CloudProviderDefinition`/`CLOUD_PROVIDERS`
registry (`src/lib/cloudProviders.ts`). Every cloud touchpoint becomes
data-driven over a `CloudProvider` enum.

Curated shortlist + default model per provider (one catalog entry per provider,
fixed curated model — matches the existing Soniox single-entry pattern; a
per-model picker is explicitly out of scope for this plan):

| id        | display   | default model                | auth   | body       | response text path                                  | lang     |
|-----------|-----------|------------------------------|--------|------------|-----------------------------------------------------|----------|
| soniox    | Soniox    | `stt-async-v3` (existing)    | Bearer | multipart (async files+poll flow) | `text` or join `tokens[].text`     | optional |
| openai    | OpenAI    | `gpt-4o-transcribe`          | Bearer | multipart `file`+`model` | top-level `text`                       | optional |
| groq      | Groq      | `whisper-large-v3-turbo`     | Bearer | multipart `file`+`model` | top-level `text`                       | optional |
| deepgram  | Deepgram  | `nova-3`                     | **Token** | **raw audio bytes**, `Content-Type: audio/wav` | `results.channels[0].alternatives[0].transcript` | optional (query) |
| cohere    | Cohere    | `cohere-transcribe-03-2026`  | Bearer | multipart `file`+`model`+`language` | top-level `text`            | **required** |

Verified endpoints (official docs, 2026-06-12):
- OpenAI: `POST https://api.openai.com/v1/audio/transcriptions`; validate `GET https://api.openai.com/v1/models`.
- Groq: `POST https://api.groq.com/openai/v1/audio/transcriptions`; validate `GET https://api.groq.com/openai/v1/models`.
- Deepgram: `POST https://api.deepgram.com/v1/listen?model=nova-3&smart_format=true`; validate `GET https://api.deepgram.com/v1/projects` with `Authorization: Token <key>`.
- Cohere: `POST https://api.cohere.com/v2/audio/transcriptions` (multipart `model`,`language`,`file`); validate `GET https://api.cohere.com/v1/models`.

Auth on success: 2xx → ok; 401/403 → "Invalid <Provider> API key"; other → `HTTP <code>: <snippet>`.

## Backend seam (new module `src-tauri/src/cloud_stt/`)

`mod.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudProvider { Soniox, Openai, Groq, Deepgram, Cohere }

impl CloudProvider {
    pub const ALL: &'static [CloudProvider] = &[Self::Soniox, Self::Openai, Self::Groq, Self::Deepgram, Self::Cohere];
    pub fn id(self) -> &'static str;                 // "soniox","openai","groq","deepgram","cohere"
    pub fn from_id(s: &str) -> Option<Self>;         // trim + lowercase
    pub fn display_name(self) -> &'static str;       // "Soniox","OpenAI",...
    pub fn cloud_label(self) -> String;              // "<display> (Cloud)"
    pub fn key_name(self) -> &'static str;           // "stt_api_key_<id>"
    pub fn default_model(self) -> &'static str;
    pub fn docs_url(self) -> &'static str;
    pub fn speed_score(self) -> u8;                  // catalog hint
    pub fn accuracy_score(self) -> u8;               // catalog hint
    pub fn requires_language(self) -> bool;          // cohere = true
    pub async fn validate_key(self, api_key: &str) -> Result<(), String>;
    pub async fn transcribe(self, app: &AppHandle, audio_path: &Path, language: Option<&str>) -> Result<String, String>;
}
```
- `validate_key` dispatches to the per-provider GET validation.
- `transcribe` reads the key from `secure_store::secure_get(app, key_name())` (error if missing), then dispatches to the per-provider file.
- Per-provider files (`soniox.rs`, `openai.rs`, `groq.rs`, `deepgram.rs`, `cohere.rs`) each expose `pub(super) async fn transcribe(app, key, audio_path, language) -> Result<String,String>` and `pub(super) async fn validate_key(key) -> Result<(),String>`.
- `common.rs`: shared helpers — `openai_compatible_transcribe(base_url, key, model, audio_path, language)` (multipart, Bearer, parse `{text}`; used by openai+groq), `get_validate(url, header_name, header_value)` (GET auth check with the 401/403 mapping), and an http-error snippet helper. Reuse the existing Soniox error/snippet style.
- Soniox migration: move `soniox_transcribe_async` + `build_soniox_create_payload` (currently `commands/audio.rs:5113-5307`) and their unit tests (`commands/audio.rs:942-1010` region) into `cloud_stt/soniox.rs`, unchanged behavior (still loads `compile_soniox_context` from `writing.rs`).

## Backend wiring (cutover — no `soniox`-only residue)

1. `commands/stt.rs`: replace `validate_and_cache_soniox_key` + `clear_soniox_key_cache` with generic `validate_stt_key(provider: String, api_key: String)` (dispatch via `CloudProvider::from_id`) and `clear_stt_key_cache(provider: String)` (no-op, kept for the FE removal flow). Reject unknown provider id.
2. `commands/audio.rs`: `ActiveEngineSelection::Soniox { model_name }` → `Cloud { provider: CloudProvider, model_name: String }`; `engine_name()` → `provider.id()`; `resolve_engine_for_model` recognizes any `CloudProvider::from_id(engine_hint or model_name)` (key present → `Cloud`, else "<Provider> key not configured. Please configure it in Models."); the three `ActiveEngineSelection::Soniox { .. } =>` transcribe arms (live ~3722, audio-file ~4720, audio-bytes ~5002) become `Cloud { provider, .. } => cloud_stt::CloudProvider::transcribe(...)`.
3. `commands/model.rs`: `collect_cloud_models` iterates `CloudProvider::ALL`, one `UnifiedModelInfo` each (`name`/`engine` = id, `display_name` = display_name, `kind:"cloud"`, `downloaded` = `secure_has(key_name)`, `requires_setup` = `!has_key`, scores from provider). `UnifiedModelInfo` struct unchanged.
4. `provider_capabilities.rs`: add `Openai, Groq, Deepgram, Cohere` to `ProviderEngine`; `from_engine_str`/`as_str`; capabilities = all flags `false` (no structured terms / prompt / vocab / translate / shareable_remote). Soniox keeps `supports_structured_terms: true`. Update `capabilities_match_static_truth_table` + `capability_invariants_are_pinned` (invariants still hold: only-Soniox structured terms, only-Whisper prompt, only-Whisper/Parakeet shareable, only-Parakeet vocab, Whisper/Remote translate).
5. `commands/settings.rs`: `normalize_speech_language_for_model` — add arms: `openai`/`groq`/`deepgram` keep a valid ISO-639-1 code else `"en"` (broad, Whisper-class); `cohere` restrict to its supported set else `"en"`. Replace `is_cloud_engine = engine == "soniox"` with `CloudProvider::from_id(&settings.current_model_engine).is_some()`. Both engine-resolution sites (`model_name == "soniox"`) → `CloudProvider::from_id(model_name).map(|p| p.id())`.
6. `recognition/model_selection.rs`: rename `soniox_selected`/`soniox_ready` → `cloud_selected`/`cloud_ready`; compute from `CloudProvider::from_id(engine)` + `secure_has(provider.key_name())`; `any_available` and auto-selection use cloud_*; auto-select returns `(engine_id, engine_id)`. Update tests.
7. `menu/tray.rs`: iterate `CloudProvider::ALL`, push each provider whose key exists as `(id, cloud_label(), u8::MAX, 0)`; current-model display uses `CloudProvider::from_id(current_model).map(|p| p.cloud_label())`.
8. `commands/remote.rs`: `ensure_sharing_engine_supported` rejects any `CloudProvider::from_id(engine).is_some()` with "Network sharing is not available for cloud transcription. Please select a Whisper or Parakeet model to share." Update tests (soniox + the new ids rejected; whisper/parakeet/remote/unknown allowed).
9. `lib.rs`: `mod cloud_stt;`; register `validate_stt_key`, `clear_stt_key_cache`; drop the two soniox commands.
10. `tests/settings_commands.rs`: extend valid engines to include the new cloud ids.

## Frontend wiring (cutover)

1. `src/types.ts`: `SpeechModelEngine` adds `'openai' | 'groq' | 'deepgram' | 'cohere'`; `AppSettings.current_model_engine` same; `RemoteShareableModel.engine` = `Extract<SpeechModelEngine, 'whisper' | 'parakeet'>` (only local engines shareable).
2. `src/utils/keyring.ts`: replace soniox-specific helpers with generic `saveSttApiKey(provider, key)` (calls `invoke('validate_stt_key', { provider, apiKey: key })` then `keyringSet('stt_api_key_'+provider, key)` then emit `stt-key-saved` `{provider}`), `removeSttApiKey(provider)` (delete + best-effort `invoke('clear_stt_key_cache', { provider })` + emit `stt-key-removed`), `hasSttApiKey(provider)`.
3. `src/lib/cloudProviders.ts`: widen `CloudProviderDefinition.engine` to `SpeechModelEngine`; add the four provider definitions (id/engine/modelName = provider id; displayName; providerName; description; docsUrl; setupCta); `addKey/removeKey/hasKey` delegate to the generic keyring helpers; `CLOUD_PROVIDERS` keyed by id.
4. `src/components/ApiKeyModal.tsx`: drop the soniox-specific display/url block; render purely from `providerName`/`docsUrl` props.
5. `src/components/sections/ModelsSection.tsx`: widen the `currentEngine` cast union; rendering is already data-driven via `CLOUD_PROVIDERS` + backend model list.
6. `src/components/LanguageSelection.tsx`: widen engine prop union; cloud language filtering — cohere → its supported set, others → broad (no filter).
7. Cloud detection in `AudioUploadSection.tsx`, `RecentRecordings.tsx`, `NetworkSharingCard.tsx`: replace `current_model_engine === 'soniox'` with a cloud-engine check (`CLOUD_PROVIDERS[engine] != null` or a small `isCloudEngine(engine)` helper in `lib/cloudProviders.ts`); replace hardcoded `'Soniox (Cloud)'` with `getModelDisplayName` / backend `display_name`.
8. `src/lib/model-display.ts`: `KNOWN_MODEL_DISPLAY_NAMES` add `openai/groq/deepgram/cohere → "<Display> (Cloud)"`.
9. `src/hooks/useModelAvailability.ts`: snapshot fields `soniox_selected/soniox_ready` → `cloud_selected/cloud_ready`; `hasLocalReadySource` derivation; keep `stt-key-saved`/`stt-key-removed` listeners.
10. Tests: update `AudioUploadSection.test.tsx`, `RecentRecordings.test.tsx`, `NetworkSharingCard.test.tsx`, `useModelAvailability.test.tsx`, `AppContainer.test.tsx` for renamed snapshot fields; add coverage that a non-Soniox cloud provider (e.g. Deepgram) is labeled/selected as cloud and rejected for sharing.

## Verification

- `cd src-tauri && cargo test`
- `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
- `cargo fmt --check` (run from `src-tauri`)
- `pnpm typecheck && pnpm lint && pnpm test --run`
- `pnpm build`
- `git diff --check`
- Manual smoke (NEEDS-SMOKE; real keys, deferred): add a key for each provider in
  Models → provider becomes selectable → record/upload → transcript returned;
  invalid key → clear typed error; cloud provider cannot be shared (sharing tab
  warning). Deepgram raw-body + Token auth path exercised specifically.

## Done criteria

- [ ] `cloud_stt` module owns all provider behavior; Soniox migrated; four new providers implemented to the verified contracts.
- [ ] No `soniox`-specific command, keyring helper, snapshot field, or hardcoded label remains; cutover is clean.
- [ ] Catalog, engine resolution, settings normalization, availability snapshot, tray, and sharing rejection are all data-driven over `CloudProvider`.
- [ ] All automated gates green; manual smoke `NEEDS-SMOKE` (no keys in CI).
- [ ] `plans/README.md` row added.

## STOP conditions

1. A provider's verified endpoint/response shape differs from this plan at
   implementation time → fix to the live contract and note it; do not invent.
2. The availability-snapshot rename leaks a half-renamed shape across BE/FE
   (some `soniox_*`, some `cloud_*`) → must be atomic; do not ship partial.
3. Any provider needs streaming/partials or a per-model picker → out of scope;
   leave a follow-up note, ship the single-default-model entry.
