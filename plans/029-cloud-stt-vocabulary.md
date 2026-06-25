# 029 — Wire the dictionary into cloud STT (the real jargon fix)

Status: APPROVED — build AFTER 028 (shares `writing.rs`; must not run concurrently with 028 R2).
Why: STT mishears novel jargon/brand names ("shadcn ui", "Tauri", "Zustand"). The reliable fix is recognition-time vocab injection (the recognizer reconciles the term against the audio). Today only Whisper/Parakeet/Soniox get the dictionary. Cloud OpenAI/Groq/Deepgram receive NOTHING despite their APIs supporting it. This wires them up.

## Verified provider capabilities (source-checked 2026-06-23)
| Provider | Knob | Param | Limit |
|---|---|---|---|
| OpenAI (whisper-1, gpt-4o-transcribe, -mini) | prompt | `prompt` (multipart) | ≤224 tokens; NOT on `gpt-4o-transcribe-diarize` or realtime |
| Groq (whisper-large-v3*) | prompt | `prompt` (multipart) | ≤224 tokens |
| Deepgram (Nova-3) | keyterm | `keyterm` query param (repeatable) | ≤100 terms / ~500 tokens; Nova-2 uses `keywords` |
| Cohere Transcribe | none documented | — | leave as-is |

Refs: developers.openai.com/api/docs/guides/speech-to-text · console.groq.com/docs/speech-to-text · developers.deepgram.com/docs/keyterm

## Changes
### Capabilities (`src-tauri/src/provider_capabilities.rs`)
- OpenAI → `supports_initial_prompt: true`.
- Groq → `supports_initial_prompt: true`.
- Deepgram → `supports_vocabulary_terms: true`.
- Cohere → unchanged (all false).
Update `transcription/capabilities.rs` capability table test accordingly.

### Writing-context helpers (`src-tauri/src/writing.rs`) — AFTER 028 R2
- OpenAI/Groq prompt content: reuse the existing `WhisperInitialPrompt` target via `compile_remote_request_context` (preferred spellings + spoken forms, byte-budgeted). Keep the budget ≤ ~800 bytes to stay under 224 tokens.
- Deepgram: add `compile_deepgram_keyterms(settings, language) -> Vec<String>` — enabled, language-matching `phrase` values (+ spoken forms as separate keyterms), deduped, control-stripped, capped at 100.

### Request builders (`src-tauri/src/cloud_stt/`)
- `openai.rs` `transcribe_typed`: stop ignoring `_app`; load writing settings, compile the prompt string, add `prompt` to the multipart form when non-empty.
- `groq.rs`: same OpenAI-compatible path → add `prompt`.
- `deepgram.rs` `transcribe_at` + `transcribe_at_diarized`: append `keyterm` query params (one per term) from `compile_deepgram_keyterms`, but ONLY when the selected model is Nova-3 (gate on model id; Nova-2 path may use `keywords` or skip — pick one and document). Auth/body otherwise unchanged.
- `cohere`: no change.

## Tests (cover the matrix)
- Capability flags flip (table test).
- `compile_deepgram_keyterms`: enabled/lang-matching only; spoken forms included; deduped; ≤100; control-stripped; empty when no dictionary.
- OpenAI/Groq (wiremock, mirror existing deepgram test style): request carries `prompt` when dictionary present; omits it when empty.
- Deepgram: request carries `keyterm` params for Nova-3; none for a non-Nova-3 model; diarized path also carries them.
- Cohere request unchanged (no vocab params).

## Verification
`cd src-tauri && cargo test` → `cargo clippy -- -D warnings` → `cargo fmt --check`. (Frontend untouched; no pnpm gate needed unless a provider label changes.)

## Non-goals
No new providers. No streaming/realtime. No Cohere vocab (no API knob). No change to local engines (already wired).
