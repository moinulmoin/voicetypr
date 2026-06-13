# Plan 014: Shared transcription contract — Stage 1 (contract DTOs + executor)

> **Executor instructions**: This is **Stage 1** of the migration map in
> `docs/plans/2026-06-10-shared-transcription-contract-design.md` (lines
> 290-301). Stage 1 is **purely additive**: create a new `transcription/`
> submodule (request/error/capability DTOs + one executor that *delegates to
> existing helpers*) and wire it into `lib.rs`. **Do NOT** port any existing
> callsite, change the remote wire protocol, touch `/api/v1/*`, or delete any
> legacy type/path. Old desktop/upload/bytes/remote/CLI paths keep working
> unchanged. Stages 2-6 (porting callsites, multipart cutover, deletion) are
> separate future plans. Run every verification command before yielding.
>
> **Baseline commit**: `e018daa` (branch `plan/v2-roadmap`).
> **Design drift reconciled**: the design predates the `cloud_stt` module
> (plan 019, 2026-06-12). The executor routes Whisper / Parakeet /
> **CloudStt(provider)** / Remote, where cloud is the multi-provider
> `cloud_stt::CloudProvider` seam, NOT a Soniox-only branch.

## Status

- **Priority**: P1 (foundation every later contract stage + 015/016 executor
  seam depends on)
- **Effort**: L
- **Risk**: LOW — additive only; no prod callsite is rewired, so runtime
  behavior is unchanged. The executor is reachable (public API, exercised by
  tests) but not yet wired into recording/upload/remote in Stage 1.
- **Depends on**: 012 (design, DONE)
- **Category**: backend architecture / contract seam
- **Planned at**: 2026-06-13

## Decision

Build `src-tauri/src/transcription/` as new children of the existing
`transcription.rs` seed (Rust 2018 lets `transcription.rs` declare
`pub mod request;` with files under `transcription/`). No file move; the seed
DTOs (`TranscriptionSource`, `TranscriptionTask`, `TranscriptionJob`,
`TranscriptionResult`, segments/timings) stay put and are reused unchanged.

Strangler-fig: the executor delegates engine execution to the **existing**
reusable helpers rather than reimplementing inference/HTTP. Delegation is clean
for Whisper (`transcribe_whisper_with_acceleration`), Cloud
(`cloud_stt::CloudProvider`), and Remote (`remote::client` transcribe); only
Parakeet is inline-wrapped at each current site, so the executor's Parakeet arm
gets a thin local wrapper over the existing manager calls (full extraction is a
Stage 2 concern).

YAGNI deviations from the design sketch (documented intentionally):
- Ship the `transcribe_with_app` **free function**; **skip** the
  `TranscriptionExecutor` trait until Stage 4 introduces the second
  (remote-server `HostDefault`) implementation. One impl, no trait.
- `capabilities.rs` **re-exports/wraps** `provider_capabilities` (it already is
  the single source of truth, only 2 callers); it does **not** move the matrix.

## Module layout (all new unless noted)

```
src-tauri/src/transcription.rs        (seed; add `pub mod` declarations + re-exports)
src-tauri/src/transcription/request.rs
src-tauri/src/transcription/error.rs
src-tauri/src/transcription/capabilities.rs
src-tauri/src/transcription/executor.rs
```

### request.rs

```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use crate::provider_capabilities::ProviderEngine;
use crate::transcription::{TranscriptionSource, TranscriptionTask};

pub enum TranscriptionAudio {
    Path { path: PathBuf, format_hint: Option<AudioFormatHint>, cleanup: CleanupPolicy },
    Bytes { bytes: Vec<u8>, format_hint: Option<AudioFormatHint> },
}

pub enum AudioFormatHint { Wav, Other(String) }
pub enum CleanupPolicy { CallerOwns, DeleteAfterAttempt, PreserveOnRetryableFailure }

pub enum EngineSelection {
    Explicit { engine: ProviderEngine, model: String },
    /// Remote server inbound (Stage 4) snapshots its own shared engine/model.
    HostDefault,
}

pub enum TimeoutPolicy { Interactive, Upload, Explicit(Duration), None }

pub struct RequestTerm { pub text: String, pub aliases: Vec<String> }
pub struct RequestCorrection { pub from: String, pub to: String }

#[derive(Default)]
pub struct RequestContext {
    pub terms: Vec<RequestTerm>,
    pub corrections: Vec<RequestCorrection>,
    pub max_bytes: u32,
}

/// Typed cancellation (replaces today's string-matched "Transcription cancelled").
/// Backed by Arc<AtomicBool> to match the existing `cancel_flag` closures.
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn new() -> Self;
    pub fn cancel(&self);
    pub fn is_cancelled(&self) -> bool;
}

pub struct TranscriptionRequest {
    pub source: TranscriptionSource,
    pub audio: TranscriptionAudio,
    pub engine: EngineSelection,
    pub spoken_language: Option<String>,
    pub task: TranscriptionTask,
    pub context: RequestContext,
    pub timeout: TimeoutPolicy,
    pub cancellation: CancellationToken,
}
```

- `RequestContext` gets a deterministic `prune_to(max_bytes)` helper (drop
  lowest-value terms/corrections until serialized size fits, else clear),
  unit-tested. This is the cap-negotiation primitive Stage 4/5 use; it is pure
  and testable now.

### error.rs

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionErrorCode {
    ModelUnavailable, EngineUnavailable, EngineFailed, AudioInvalid,
    Cancelled, Timeout, TransportFailed, Unauthorized,
    UnsupportedMediaType, ResponseInvalid, Internal,
}

#[derive(Debug, Clone)]
pub struct TranscriptionError {
    pub code: TranscriptionErrorCode,
    pub retryable: bool,
    pub user_message: String,
    /// Log/history-only. NEVER serialize to remote clients.
    pub detail: Option<String>,
    pub source: TranscriptionSource,
}

#[derive(serde::Serialize)]
pub struct PublicTranscriptionError<'a> {
    pub code: TranscriptionErrorCode,
    pub retryable: bool,
    pub user_message: &'a str,
}

impl TranscriptionError {
    pub fn new(code, source, user_message) -> Self;   // retryable defaults per code
    pub fn with_detail(self, detail: impl Into<String>) -> Self;
    pub fn public(&self) -> PublicTranscriptionError<'_>;  // drops detail
}
```

Mappers (each typed, body/low-level strings → `detail` only):
- `from_remote_client_error(&RemoteClientError, source)` — AuthFailed→Unauthorized(false);
  Timeout→Timeout(true); ConnectFailed/HttpStatus→TransportFailed(true);
  ResponseDecode/ResponseSchema→ResponseInvalid(true); RequestBuild/JoinFailed→Internal(false).
  `server_error_body()` → `detail`, never `user_message`.
- `from_stt_error(&cloud_stt::common::SttError, source)` — Auth→Unauthorized(false);
  ModelUnavailable→ModelUnavailable(false); RateLimited→TransportFailed(true);
  Timeout→Timeout(true); Network→TransportFailed(true); Server→EngineFailed(true);
  BadResponse→ResponseInvalid(false). Retryable cross-checked with `is_transient()`.
- `from_local_engine_string(&str, source)` — best-effort marker detection on the
  legacy local String errors: contains "cancelled"→Cancelled(false);
  "too short"→AudioInvalid(false); "timed out"→Timeout(true);
  "No speech recognition models"/"model"→ModelUnavailable(false); else EngineFailed(true).
  Raw string → `detail`; stable `user_message` per code.

### capabilities.rs

```rust
pub use crate::provider_capabilities::{ProviderEngine, ProviderCapabilities, capabilities_for_engine};

pub fn may_translate(engine: ProviderEngine) -> bool;        // supports_translate_task
pub fn accepts_request_context(engine: ProviderEngine) -> bool; // prompt|structured|vocab
pub fn is_shareable_remote(engine: ProviderEngine) -> bool;  // shareable_remote
```

### executor.rs

```rust
pub async fn transcribe_with_app(
    app: &tauri::AppHandle,
    request: TranscriptionRequest,
) -> Result<TranscriptionResult, TranscriptionError>;
```

Behavior (snapshot at start; one route; return existing `TranscriptionResult`):
1. Resolve `EngineSelection` → `ActiveEngineSelection` (reuse the existing
   `resolve_engine_for_model`, widened to `pub(crate)`): `Explicit` passes
   `engine.as_str()` + `model`; `HostDefault` resolves from current config
   (`current_model_engine`/`current_model`).
2. Build the `TranscriptionJob` (source/engine/model/spoken_language/task) for
   the result.
3. Normalize audio to WAV only when the resolved engine requires it (Whisper/
   Parakeet yes; Cloud/Remote skip), reusing `crate::ffmpeg::normalize_streaming`.
   Respect `CleanupPolicy` for temp files.
4. Select timeout from `TimeoutPolicy` (Interactive/Upload map to the existing
   live-recording / upload budgets) and wrap the engine call.
5. Route ONCE, delegating:
   - Whisper → `transcribe_whisper_with_acceleration(... , move || token.is_cancelled())`.
   - Parakeet → thin wrapper over existing manager load+transcribe calls.
   - Cloud → `cloud_stt::CloudProvider::transcribe_typed(app, path, lang)` (new
     `pub(crate)` typed seam) → `from_stt_error` on failure.
   - Remote → reuse remote request build + `remote::client` transcribe →
     `from_remote_client_error` on failure.
6. Map every failure through the error mappers; success → `TranscriptionResult`.

## Supporting changes (additive, behavior-preserving)

- `commands/audio.rs`: widen to `pub(crate)`: `resolve_engine_for_model`,
  `ActiveEngineSelection` (+ its methods), `transcribe_whisper_with_acceleration`,
  the Parakeet manager helper(s) the executor calls, and the normalize helper.
  **Visibility only — no logic change.** Do NOT modify the existing callsites.
- `cloud_stt/mod.rs`: add `pub(crate) async fn transcribe_typed(self, app, path,
  language) -> Result<String, common::SttError>`; the existing public
  `transcribe` becomes a thin wrapper `transcribe_typed(...).await.map_err(|e|
  e.message(self.display_name()))`. No behavior change to existing callers.
- `transcription.rs`: add `pub mod request; pub mod error; pub mod capabilities;
  pub mod executor;` and convenience re-exports. `lib.rs` already exports
  `pub mod transcription;`.

## Scope boundaries (Stage 1 does NOT)

- Port desktop recording / upload / audio-bytes / CLI / remote-server callsites
  (Stages 2-5).
- Change `/api/v1/transcribe` to multipart or touch `StatusResponse` /
  `RemoteCapabilities` / `ErrorResponse` wire shapes (Stages 4-5).
- Delete `TranscriptionFailure`, `remote::client::TranscriptionRequest`, base64
  `X-VoiceTypr-Context`, or any duplicate engine-routing branch (Stage 6).
- Add the `TranscriptionExecutor` trait (Stage 4).

## Tests (focused, no real keys/engines)

- request.rs: DTO construction; `RequestContext::prune_to` boundary cases
  (fits / over-cap drops lowest-value / hard-over clears); `CancellationToken`
  cancel/is_cancelled.
- error.rs: each mapper → correct `(code, retryable)`;
  `PublicTranscriptionError` (via serde_json) **never** contains `detail`/body;
  `from_local_engine_string` marker detection table; auth body lands in
  `detail` not `user_message`.
- capabilities.rs: `may_translate`/`accepts_request_context`/
  `is_shareable_remote` match the matrix for all 8 engines.
- executor.rs: pure-logic coverage that needs no live engine — timeout-policy →
  duration selection; normalization-required decision per engine; engine
  resolution mapping for `Explicit`/`HostDefault`. (Live engine dispatch is
  validated when Stage 2 wires it + manual smoke.)

## Verification (all must pass before yielding)

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
pnpm typecheck && pnpm lint   # no FE change expected; confirm untouched
```

## Rollback

Remove the four new files, the `pub mod` lines in `transcription.rs`, the
`transcribe_typed` seam, and the `pub(crate)` visibility widenings. No prod
callsite changed, so reverting the Stage-1 commit fully restores prior behavior.

## STOP conditions

- If delegating to an existing helper would require changing its behavior or an
  existing callsite, STOP and reduce scope to a thin local wrapper — Stage 1 must
  not alter running paths.
- If a clean typed cloud-error seam can't be added without touching cloud_stt
  behavior, fall back to `from_local_engine_string` on the public `transcribe`
  String and leave a follow-up note.
