# Shared transcription contract design

## Scope and evidence

This document proposes one request/error/executor contract for desktop recording, upload, remote server, remote client, and CLI transcription. It is documentation only: no `src/` or `src-tauri/` source changes accompany it.

Existing contract seed:

- `TranscriptionSource` currently distinguishes `DesktopRecording`, `AudioFile`, `AudioBytes`, and `RemoteServer` (`src-tauri/src/transcription.rs:3`, `src-tauri/src/transcription.rs:5`).
- `TranscriptionTask` currently distinguishes `Transcribe` and `TranslateToEnglish` (`src-tauri/src/transcription.rs:12`, `src-tauri/src/transcription.rs:14`).
- `TranscriptionJob` carries only source, engine, model, spoken language, and task (`src-tauri/src/transcription.rs:36`, `src-tauri/src/transcription.rs:38`).
- `TranscriptionResult` carries raw text, engine/model/language/task, optional segments, and timings (`src-tauri/src/transcription.rs:78`, `src-tauri/src/transcription.rs:80`).

The V2 deferred plan says the remote protocol should become one multipart endpoint and explicitly says remote had not shipped, so no legacy `/transcribe` dual path should be preserved (`docs/plans/2026-06-07-001-v2-deferred-items-plan.md:82`, `docs/plans/2026-06-07-001-v2-deferred-items-plan.md:83`). I also checked release evidence: the current package version is `1.12.3` (`package.json:4`), the latest changelog section is `1.12.3` (`CHANGELOG.md:1`), and that section lists settings/support features and support/UI fixes rather than remote/network sharing (`CHANGELOG.md:3`, `CHANGELOG.md:8`). A read-only `git tag --contains 6af8d81` for the `feat: Network Sharing and Remote Transcription` commit returned no tags.

## Inventory

### Request shapes

| Surface | Audio input | Engine/model input | Language/task input | Context input | Settings read site | Contract gap |
|---|---|---|---|---|---|---|
| Desktop recording | Uses the captured/normalized `audio_path`; Soniox/Remote skip normalization while Whisper/Parakeet normalize to WAV (`src-tauri/src/commands/audio.rs:3026`, `src-tauri/src/commands/audio.rs:3040`). | Prefer active remote when online (`src-tauri/src/commands/audio.rs:2827`, `src-tauri/src/commands/audio.rs:2838`); otherwise route by `config.current_engine` and selected model (`src-tauri/src/commands/audio.rs:2855`, `src-tauri/src/commands/audio.rs:3018`). | Normalizes configured speech language by engine/model (`src-tauri/src/commands/audio.rs:3142`, `src-tauri/src/commands/audio.rs:3145`); resolves task and translates it to a bool (`src-tauri/src/commands/audio.rs:3151`, `src-tauri/src/commands/audio.rs:3157`). | Remote desktop requests compile request-local context from the active remote connection and spoken language (`src-tauri/src/commands/audio.rs:3386`, `src-tauri/src/commands/audio.rs:3390`). | A settings snapshot reads language/task/AI fields from the store (`src-tauri/src/commands/audio.rs:1316`, `src-tauri/src/commands/audio.rs:1335`). | Request state is reconstructed into `TranscriptionJob`, then execution still matches on `ActiveEngineSelection` (`src-tauri/src/commands/audio.rs:3174`, `src-tauri/src/commands/audio.rs:3216`). |
| Upload command / CLI local helper | `transcribe_audio_file_impl` accepts a file path string and uses that path directly (`src-tauri/src/commands/audio.rs:4185`, `src-tauri/src/commands/audio.rs:4202`). | `resolve_engine_for_model` uses the provided `model_name` plus optional engine hint (`src-tauri/src/commands/audio.rs:4224`, `src-tauri/src/commands/audio.rs:4226`). | Reads legacy and current language/task settings, then normalizes by engine/model (`src-tauri/src/commands/audio.rs:4232`, `src-tauri/src/commands/audio.rs:4261`). | Remote upload builds context with `resolve_remote_request_context` before creating the remote request (`src-tauri/src/commands/audio.rs:4415`, `src-tauri/src/commands/audio.rs:4422`). | Upload rereads `language`, `translate_to_english`, `speech_language`, `ai_enabled`, and `transcription_task` (`src-tauri/src/commands/audio.rs:4233`, `src-tauri/src/commands/audio.rs:4253`). | Upload has its own engine match and normalization branches (`src-tauri/src/commands/audio.rs:4282`, `src-tauri/src/commands/audio.rs:4284`). |
| Audio bytes / clipboard-style command | Writes incoming bytes to `recordings/temp_audio.wav` (`src-tauri/src/commands/audio.rs:4521`, `src-tauri/src/commands/audio.rs:4523`). | Resolves engine from model and optional engine hint (`src-tauri/src/commands/audio.rs:4525`, `src-tauri/src/commands/audio.rs:4526`). | Rereads and normalizes language/task from settings (`src-tauri/src/commands/audio.rs:4528`, `src-tauri/src/commands/audio.rs:4557`). | Remote bytes path compiles remote request context before `build_remote_upload_transcription_request` (`src-tauri/src/commands/audio.rs:4684`, `src-tauri/src/commands/audio.rs:4691`). | Reads the same language/task settings again (`src-tauri/src/commands/audio.rs:4529`, `src-tauri/src/commands/audio.rs:4549`). | Audio bytes has another full engine match and temp-file cleanup path (`src-tauri/src/commands/audio.rs:4578`, `src-tauri/src/commands/audio.rs:4719`). |
| Remote server inbound | HTTP accepts raw bytes as the request body with a 50 MB limit (`src-tauri/src/remote/http.rs:105`, `src-tauri/src/remote/http.rs:473`). | Server snapshots engine/model from shared state in `transcribe_inner` (`src-tauri/src/remote/transcription.rs:270`, `src-tauri/src/remote/transcription.rs:272`). | HTTP extracts `X-Voicetypr-Speech-Language` and `X-Voicetypr-Transcription-Task` headers (`src-tauri/src/remote/http.rs:98`, `src-tauri/src/remote/http.rs:102`) and the server converts the task to `translate_to_english` (`src-tauri/src/remote/transcription.rs:273`). | HTTP accepts `X-Voicetypr-Context` and decodes base64 with byte caps (`src-tauri/src/remote/http.rs:104`, `src-tauri/src/remote/http.rs:358`). | Remote server does not read the client settings store in the request path; it reads shared server state (`src-tauri/src/remote/transcription.rs:37`, `src-tauri/src/remote/transcription.rs:85`). | Server rebuilds `TranscriptionJob`, writes bytes to a temp WAV, and routes engines locally (`src-tauri/src/remote/transcription.rs:275`, `src-tauri/src/remote/transcription.rs:296`, `src-tauri/src/remote/transcription.rs:312`). |
| Remote client outbound | `remote::client::TranscriptionRequest` owns `Vec<u8>` audio data (`src-tauri/src/remote/client.rs:71`, `src-tauri/src/remote/client.rs:75`). | The client request itself has no model field; the server's `/status` response carries model/engine/capabilities (`src-tauri/src/remote/server.rs:28`, `src-tauri/src/remote/server.rs:38`). | Builder attaches optional spoken language and transcription task (`src-tauri/src/remote/client.rs:102`, `src-tauri/src/remote/client.rs:108`). | Builder carries optional string context and serializes it as base64 header (`src-tauri/src/remote/client.rs:112`, `src-tauri/src/remote/client.rs:321`). | Caller provides settings-derived values; the client type does not read settings (`src-tauri/src/remote/client.rs:87`, `src-tauri/src/remote/client.rs:112`). | Request timeout policy lives beside the remote client and depends on `TranscriptionSource` (`src-tauri/src/remote/client.rs:296`, `src-tauri/src/remote/client.rs:307`). |
| CLI transcribe | `--file` is required for `TranscribeArgs` (`src-tauri/src/cli.rs:59`, `src-tauri/src/cli.rs:61`). | `--model`, `--engine`, or settings choose local model/engine; `--server` switches to remote (`src-tauri/src/cli.rs:63`, `src-tauri/src/cli.rs:67`, `src-tauri/src/cli.rs:206`). | Local CLI transcribe uses settings current model/engine then calls `transcribe_audio_file_for_cli` (`src-tauri/src/cli.rs:209`, `src-tauri/src/cli.rs:221`). Remote CLI reads settings and sends speech language/task (`src-tauri/src/cli.rs:348`, `src-tauri/src/cli.rs:352`). | Remote CLI transcribe does not attach context; it only calls `with_language_and_task` (`src-tauri/src/cli.rs:352`, `src-tauri/src/cli.rs:356`). | `run_transcribe` reads settings for local defaults (`src-tauri/src/cli.rs:209`, `src-tauri/src/cli.rs:217`); `transcribe_via_remote` reads settings again (`src-tauri/src/cli.rs:348`). | Local CLI returns `{ text, model, engine }`, while remote CLI returns writing metadata plus model/duration (`src-tauri/src/cli.rs:228`, `src-tauri/src/cli.rs:383`). |
| CLI record | `RecordArgs` supports `--until-silence`; the implementation records to a timestamped WAV path (`src-tauri/src/cli.rs:75`, `src-tauri/src/cli.rs:246`). | Uses `--server` for remote; otherwise uses CLI model/engine args or settings defaults (`src-tauri/src/cli.rs:263`, `src-tauri/src/cli.rs:269`). | Remote record reuses `transcribe_via_remote`; local record calls `transcribe_audio_file_for_cli` (`src-tauri/src/cli.rs:264`, `src-tauri/src/cli.rs:280`). | Remote record currently inherits the no-context `transcribe_via_remote` shape (`src-tauri/src/cli.rs:263`, `src-tauri/src/cli.rs:342`). | Reads settings before recording for microphone, current model, and engine defaults (`src-tauri/src/cli.rs:245`, `src-tauri/src/cli.rs:258`). | Record and transcribe share output printing but construct payloads separately (`src-tauri/src/cli.rs:287`, `src-tauri/src/cli.rs:292`). |

### Error taxonomies

| Surface/type | Current shape | Retry signal | Internal detail exposure | Contract gap |
|---|---|---|---|---|
| `commands/audio.rs::TranscriptionFailure` | Local errors are raw `String`; remote errors wrap `RemoteClientError` (`src-tauri/src/commands/audio.rs:413`, `src-tauri/src/commands/audio.rs:416`). | `RemoteClientError` variants are mapped to string `error_kind` values (`src-tauri/src/commands/audio.rs:435`, `src-tauri/src/commands/audio.rs:445`). | UI payload `message` is `failure.message()`, which clones local strings or formats remote errors (`src-tauri/src/commands/audio.rs:419`, `src-tauri/src/commands/audio.rs:462`). | No stable cross-surface code enum; cancellation/too-short are string contains checks (`src-tauri/src/commands/audio.rs:3732`, `src-tauri/src/commands/audio.rs:3748`). |
| Failed remote history row | Stores `error_kind`, `error_detail`, and `error_body` for remote failures (`src-tauri/src/commands/audio.rs:468`, `src-tauri/src/commands/audio.rs:481`). | History retry capability is `can_retry_from_history: true` when a failed remote row is saved (`src-tauri/src/commands/audio.rs:473`, `src-tauri/src/commands/audio.rs:482`). | `error_body` can persist server response bodies from remote errors (`src-tauri/src/commands/audio.rs:479`, `src-tauri/src/commands/audio.rs:481`). | Retryability is tied to remote history preservation, not to the error itself (`src-tauri/src/commands/audio.rs:3782`, `src-tauri/src/commands/audio.rs:3795`). |
| Remote HTTP `ErrorResponse` | Wire error is a single string field (`src-tauri/src/remote/server.rs:53`, `src-tauri/src/remote/server.rs:56`). | HTTP status carries only coarse retry hints (`src-tauri/src/remote/http.rs:433`, `src-tauri/src/remote/http.rs:504`). | Server returns the internal `String` error verbatim to LAN clients on transcription failure (`src-tauri/src/remote/http.rs:497`, `src-tauri/src/remote/http.rs:503`). | Needs serialized `code`, `retryable`, and safe `user_message`; `detail` must stay local. |
| Remote server internals | `ServerContext::transcribe` returns `Result<TranscriptionResult, String>` (`src-tauri/src/remote/http.rs:35`, `src-tauri/src/remote/http.rs:40`). | Empty audio, temp-file, lock, model load, and engine failures are all strings (`src-tauri/src/remote/transcription.rs:291`, `src-tauri/src/remote/transcription.rs:382`). | Lock/model/path details are formatted into returned strings (`src-tauri/src/remote/transcription.rs:361`, `src-tauri/src/remote/transcription.rs:368`). | Cannot distinguish `audio_invalid`, `model_unavailable`, `engine_failed`, and `cancelled` without parsing text. |
| Remote client `RemoteClientError` | Structured variants cover auth, timeout, connect, HTTP status, decode/schema, request build, and join failures (`src-tauri/src/remote/client.rs:135`, `src-tauri/src/remote/client.rs:172`). | Auth and timeout helpers exist only partially (`src-tauri/src/remote/client.rs:219`, `src-tauri/src/remote/client.rs:224`). | Server bodies are retained for auth/status/decode/schema errors (`src-tauri/src/remote/client.rs:236`, `src-tauri/src/remote/client.rs:241`). | This is closest to the target taxonomy but transport-shaped, not transcription-shaped. |
| Upload/local/CLI errors | `transcribe_audio_file_impl` returns `Result<String, String>` (`src-tauri/src/commands/audio.rs:4185`, `src-tauri/src/commands/audio.rs:4191`); CLI commands return boxed errors (`src-tauri/src/cli.rs:199`, `src-tauri/src/cli.rs:202`). | No stable retryable field; upload maps remote errors to `e.to_string()` (`src-tauri/src/commands/audio.rs:4429`, `src-tauri/src/commands/audio.rs:4437`). | Local model, ffmpeg, Parakeet, and remote details flow into returned strings (`src-tauri/src/commands/audio.rs:4206`, `src-tauri/src/commands/audio.rs:4359`). | CLI output cannot reliably expose stable error codes until it consumes the shared error type. |

### Duplicated logic

| Logic | Duplicate sites | Evidence | Contract move |
|---|---|---|---|
| Engine routing | Desktop recording match, upload match, audio-bytes match, remote server match: four routing implementations. | Desktop match starts at `src-tauri/src/commands/audio.rs:3216`; upload at `src-tauri/src/commands/audio.rs:4282`; bytes at `src-tauri/src/commands/audio.rs:4578`; server at `src-tauri/src/remote/transcription.rs:312`. | One executor owns engine dispatch; callers only build `TranscriptionRequest`. |
| Engine/model resolution | Desktop inline selection differs from `resolve_engine_for_model`. | Desktop manually chooses active remote or config engine (`src-tauri/src/commands/audio.rs:2827`, `src-tauri/src/commands/audio.rs:2855`); upload/bytes call `resolve_engine_for_model` (`src-tauri/src/commands/audio.rs:4225`, `src-tauri/src/commands/audio.rs:4525`). | Introduce `EngineSelection::Explicit` and `EngineSelection::HostDefault` so resolution is a pre-executor step with one implementation. |
| Settings normalization | Settings reads and task normalization occur in the cached desktop config path, upload path, and bytes path. | Desktop settings snapshot reads language/task (`src-tauri/src/commands/audio.rs:1316`, `src-tauri/src/commands/audio.rs:1335`); upload rereads them (`src-tauri/src/commands/audio.rs:4233`, `src-tauri/src/commands/audio.rs:4253`); bytes rereads them (`src-tauri/src/commands/audio.rs:4529`, `src-tauri/src/commands/audio.rs:4549`). | Add one `TranscriptionRequest::from_settings_snapshot` helper used before all surfaces. |
| Language normalization | Desktop/upload/bytes each call `normalize_speech_language_for_model`. | Desktop call (`src-tauri/src/commands/audio.rs:3145`), upload call (`src-tauri/src/commands/audio.rs:4261`), bytes call (`src-tauri/src/commands/audio.rs:4557`). | Normalize while resolving `EngineSelection`, before executor. |
| Temp-file handling | Desktop normalized files, upload `NormalizedTempFile`, bytes fixed temp path, remote server `NamedTempFile`, CLI remote wrapper. | Desktop removes raw/normalized files (`src-tauri/src/commands/audio.rs:3064`, `src-tauri/src/commands/audio.rs:3448`); upload uses `NormalizedTempFile` (`src-tauri/src/commands/audio.rs:4287`); bytes writes/removes `temp_audio.wav` (`src-tauri/src/commands/audio.rs:4521`, `src-tauri/src/commands/audio.rs:4719`); server uses `NamedTempFile` (`src-tauri/src/remote/transcription.rs:296`, `src-tauri/src/remote/transcription.rs:308`); CLI remote has `RemoteNormalizedAudio` drop cleanup (`src-tauri/src/cli.rs:300`, `src-tauri/src/cli.rs:310`). | `TranscriptionAudio::{Path, Bytes}` plus executor-local normalization/temp policy. |
| Remote request wire construction | Desktop remote and upload/bytes remote each build remote requests. | Desktop creates `RemoteTranscriptionRequest::new(...LiveRecording)` with headers (`src-tauri/src/commands/audio.rs:3394`, `src-tauri/src/commands/audio.rs:3404`); upload/bytes use `build_remote_upload_transcription_request` (`src-tauri/src/commands/audio.rs:742`, `src-tauri/src/commands/audio.rs:755`). | Remote client serializes the shared request; callers do not hand-assemble headers/multipart. |
| Capability facts | `provider_capabilities` is a static source, but status projects it into a different DTO. | Matrix exposes `shareable_remote`, prompt/context, vocab, and translate flags (`src-tauri/src/provider_capabilities.rs:16`, `src-tauri/src/provider_capabilities.rs:22`); status maps engine caps into `RemoteCapabilities` (`src-tauri/src/remote/http.rs:337`, `src-tauri/src/remote/http.rs:349`). | Shared contract consumes provider capabilities directly and status advertises a wire-safe subset. |

## Contract proposal

### Module boundary

Add a `src-tauri/src/transcription/` module in the implementation phase. Keep today's DTOs as the seed (`src-tauri/src/transcription.rs:36`, `src-tauri/src/transcription.rs:78`), then split into:

- `request.rs`: `TranscriptionRequest`, `TranscriptionAudio`, engine/model selection, timeout, cancellation, context.
- `error.rs`: stable `TranscriptionError` and `TranscriptionErrorCode`.
- `executor.rs`: one executor trait/function used by desktop recording, upload, remote server inbound, and CLI.
- `capabilities.rs`: current provider matrix folded in from `provider_capabilities.rs`, whose purpose is already engine behavior flags (`src-tauri/src/provider_capabilities.rs:1`, `src-tauri/src/provider_capabilities.rs:16`).

`ActiveEngineSelection` remains an internal resolved form for local engine execution, not a request shape (`src-tauri/src/commands/audio.rs:1615`, `src-tauri/src/commands/audio.rs:1636`). `ProviderCapabilities` should decide whether a request may translate, accept prompt/context, use vocabulary terms, or be shared remotely (`src-tauri/src/provider_capabilities.rs:44`, `src-tauri/src/provider_capabilities.rs:78`).

### Rust type sketches

```rust
pub struct TranscriptionRequest {
    pub source: TranscriptionSource,
    pub audio: TranscriptionAudio,
    pub engine: EngineSelection,
    pub spoken_language: Option<String>,
    pub task: TranscriptionTask,
    pub context: RequestContext,
    pub cancellation: CancellationToken,
    pub timeout: TimeoutPolicy,
}

pub enum TranscriptionAudio {
    Path {
        path: std::path::PathBuf,
        format_hint: Option<AudioFormatHint>,
        cleanup: CleanupPolicy,
    },
    Bytes {
        bytes: bytes::Bytes,
        format_hint: Option<AudioFormatHint>,
    },
}

pub enum EngineSelection {
    Explicit {
        engine: ProviderEngine,
        model: String,
    },
    /// Used by remote clients: the server snapshots its current shared engine/model.
    HostDefault,
}

pub struct RequestContext {
    pub terms: Vec<RequestTerm>,
    pub corrections: Vec<RequestCorrection>,
    pub max_bytes: u32,
}

pub struct RequestTerm {
    pub text: String,
    pub aliases: Vec<String>,
}

pub struct RequestCorrection {
    pub from: String,
    pub to: String,
}

pub enum TimeoutPolicy {
    Interactive,
    Upload,
    Explicit(std::time::Duration),
    None,
}

pub enum CleanupPolicy {
    CallerOwns,
    DeleteAfterAttempt,
    PreserveOnRetryableFailure,
}
```

The request context replaces the current base64 string context (`src-tauri/src/remote/client.rs:321`, `src-tauri/src/remote/http.rs:358`) and is byte-capped before serialization. The cap is negotiated from remote status, not hidden in the caller. Existing timeout behavior is preserved by mapping `Interactive` to the live-recording source and `Upload` to the upload source (`src-tauri/src/remote/client.rs:296`, `src-tauri/src/remote/client.rs:307`).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionErrorCode {
    ModelUnavailable,
    EngineUnavailable,
    EngineFailed,
    AudioInvalid,
    Cancelled,
    Timeout,
    TransportFailed,
    Unauthorized,
    UnsupportedMediaType,
    ResponseInvalid,
    Internal,
}

pub struct TranscriptionError {
    pub code: TranscriptionErrorCode,
    pub retryable: bool,
    pub user_message: String,
    /// Log/history-only detail. Never serialize this field to remote clients.
    pub detail: Option<String>,
    pub source: TranscriptionSource,
}

#[derive(serde::Serialize)]
pub struct PublicTranscriptionError<'a> {
    pub code: TranscriptionErrorCode,
    pub retryable: bool,
    pub user_message: &'a str,
}
```

Mapping rules:

- `Unauthorized` maps from HTTP 401 auth failures, now returned as `ErrorResponse { error: "unauthorized" }` (`src-tauri/src/remote/http.rs:433`, `src-tauri/src/remote/http.rs:435`) and client `RemoteClientError::AuthFailed` (`src-tauri/src/remote/client.rs:137`, `src-tauri/src/remote/client.rs:140`).
- `Timeout` maps from `RemoteClientError::Timeout` (`src-tauri/src/remote/client.rs:141`, `src-tauri/src/remote/client.rs:145`).
- `TransportFailed` maps from connect, HTTP status, decode/schema, request-build, and join failures (`src-tauri/src/remote/client.rs:146`, `src-tauri/src/remote/client.rs:172`).
- `AudioInvalid` maps from empty audio, unsupported media, invalid metadata, and too-short gates (`src-tauri/src/remote/transcription.rs:291`, `src-tauri/src/remote/http.rs:443`, `src-tauri/src/commands/audio.rs:3748`).
- `ModelUnavailable` maps from missing/download-required local model errors (`src-tauri/src/commands/audio.rs:1750`, `src-tauri/src/commands/audio.rs:1769`).
- `EngineFailed` maps from Whisper/Parakeet/Soniox execution failures (`src-tauri/src/remote/transcription.rs:382`, `src-tauri/src/remote/transcription.rs:432`).
- `Cancelled` maps from the cancellation token instead of string matching (`src-tauri/src/commands/audio.rs:3228`, `src-tauri/src/commands/audio.rs:3732`).

### Executor seam

```rust
#[async_trait::async_trait]
pub trait TranscriptionExecutor {
    async fn transcribe(
        &self,
        request: TranscriptionRequest,
    ) -> Result<TranscriptionResult, TranscriptionError>;
}

pub async fn transcribe_with_app(
    app: &tauri::AppHandle,
    request: TranscriptionRequest,
) -> Result<TranscriptionResult, TranscriptionError>;
```

The executor snapshots engine/model/capabilities at start, normalizes audio only when the resolved provider requires it, routes to Whisper/Parakeet/Soniox/Remote exactly once, and returns the existing `TranscriptionResult` shape. Desktop recording, upload, and CLI local call `transcribe_with_app`; remote server inbound calls an executor that uses `EngineSelection::HostDefault` resolved from `SharedServerState` (`src-tauri/src/remote/transcription.rs:37`, `src-tauri/src/remote/transcription.rs:85`). Remote client outbound serializes the same request to HTTP instead of creating a second DTO (`src-tauri/src/remote/client.rs:71`, `src-tauri/src/remote/client.rs:85`).

## Wire mapping

### `/api/v1/transcribe`

Recommendation: clean cutover to multipart, no version-negotiated legacy raw-body endpoint for first ship.

Reasons:

- The deferred plan already states the sequence is a single current protocol and no legacy route because remote had not shipped (`docs/plans/2026-06-07-001-v2-deferred-items-plan.md:82`, `docs/plans/2026-06-07-001-v2-deferred-items-plan.md:83`).
- Current code still posts raw WAV with metadata in headers (`src-tauri/src/remote/client.rs:523`, `src-tauri/src/remote/client.rs:538`) and the server expects raw body plus headers (`src-tauri/src/remote/http.rs:94`, `src-tauri/src/remote/http.rs:105`).
- The multipart target is audio binary part plus JSON metadata, avoiding base64 bloat and header limits (`docs/plans/2026-06-07-001-v2-deferred-items-plan.md:85`, `docs/plans/2026-06-07-001-v2-deferred-items-plan.md:87`).

Multipart shape:

```http
POST /api/v1/transcribe
Content-Type: multipart/form-data; boundary=...
X-Voicetypr-Key: optional-password

part "audio": application/octet-stream or audio/wav raw bytes
part "metadata": application/json
```

```json
{
  "protocol_version": 3,
  "source": "desktop_recording | audio_file | audio_bytes | remote_server",
  "engine": "host_default | whisper | parakeet | soniox | remote",
  "model": null,
  "spoken_language": "en",
  "transcription_task": "transcribe | translate_to_english",
  "timeout_policy": "interactive | upload | explicit | none",
  "context": {
    "terms": [{ "text": "Voicetypr", "aliases": ["voice typer"] }],
    "corrections": [{ "from": "voice typer", "to": "Voicetypr" }]
  }
}
```

Remote client mapping:

- `TranscriptionAudio::Bytes` maps directly to the `audio` part; `Path` is streamed/read by the client boundary layer.
- `EngineSelection::HostDefault` maps to `engine: "host_default"` and `model: null`; server snapshots shared state at job start.
- `RequestContext` is pruned deterministically to the peer's advertised `max_bytes`; if it still exceeds the cap, omit `context` and proceed, matching the deferred plan (`docs/plans/2026-06-07-001-v2-deferred-items-plan.md:89`, `docs/plans/2026-06-07-001-v2-deferred-items-plan.md:95`).
- Remote responses serialize `TranscriptionResult` fields currently returned by `TranscribeResponse`: text, duration, model, and transcript language (`src-tauri/src/remote/server.rs:43`, `src-tauri/src/remote/server.rs:50`), plus future-safe optional segments/timings from `TranscriptionResult` (`src-tauri/src/transcription.rs:86`, `src-tauri/src/transcription.rs:87`).
- Remote errors serialize only `PublicTranscriptionError`, replacing today's single string `ErrorResponse` (`src-tauri/src/remote/server.rs:53`, `src-tauri/src/remote/server.rs:56`).

### `/api/v1/status` capability advertisement

Current status already returns `protocol_version`, `engine`, and optional capabilities (`src-tauri/src/remote/server.rs:35`, `src-tauri/src/remote/server.rs:40`), and `handle_status` fills them from engine capabilities (`src-tauri/src/remote/http.rs:314`, `src-tauri/src/remote/http.rs:323`). The new status payload should advertise:

```rust
pub struct StatusResponse {
    pub status: String,
    pub version: String,
    pub model: String,
    pub name: String,
    pub machine_id: String,
    pub protocol_version: u16,
    pub engine: String,
    pub capabilities: RemoteCapabilities,
}

pub struct RemoteCapabilities {
    pub transcription_tasks: Vec<TranscriptionTask>,
    pub request_context: RemoteRequestContextCapabilities,
    pub acceleration: Vec<String>,
    pub model_control: bool,
}

pub struct RemoteRequestContextCapabilities {
    pub terms: bool,
    pub aliases: bool,
    pub corrections: bool,
    pub max_bytes: u32,
}
```

`request_context` replaces the current `accepts_request_context`/`max_context_bytes` pair (`src-tauri/src/remote/server.rs:19`, `src-tauri/src/remote/server.rs:21`). `transcription_tasks` is derived from `supports_translate_task` (`src-tauri/src/provider_capabilities.rs:21`, `src-tauri/src/provider_capabilities.rs:58`). `model_control` reflects the existing model-control route and auth gate (`src-tauri/src/remote/http.rs:112`, `src-tauri/src/remote/http.rs:128`). CPU-only acceleration remains the current advertisement (`src-tauri/src/remote/http.rs:349`, `docs/plans/2026-06-07-001-v2-deferred-items-plan.md:89`).

Privacy rule: metadata is request-local only. Do not log term values, echo them in status, or persist them; this continues the deferred plan's boundary (`docs/plans/2026-06-07-001-v2-deferred-items-plan.md:88`, `docs/plans/2026-06-07-001-v2-deferred-items-plan.md:98`).

## In-flight semantics

Rule: a job completes on the engine/model/capability snapshot it started with. Model changes affect only later jobs.

Why this rule:

- Remote shared state is mutable via `SharedServerState::update_model` (`src-tauri/src/remote/transcription.rs:55`, `src-tauri/src/remote/transcription.rs:64`).
- Current `transcribe_inner` reads engine and model at the start before routing (`src-tauri/src/remote/transcription.rs:270`, `src-tauri/src/remote/transcription.rs:281`).
- Current HTTP server serializes transcription work with a semaphore (`src-tauri/src/remote/http.rs:462`, `src-tauri/src/remote/http.rs:466`).

Implementation consequence: copy `engine`, `model`, provider capabilities, and model path/remote endpoint into a `ResolvedTranscriptionRequest` before the first await/blocking transcribe call. A concurrent model-control update may update shared state immediately for status and queued jobs, but it must not cancel, retarget, or relabel the in-flight result. If the selected model is deleted/unloaded during the job and the engine reports failure, return `model_unavailable` or `engine_failed` with `retryable` based on whether another attempt may succeed.

## Migration map

Each stage should be a separate implementation plan/branch and leave the build green.

| Stage | Change | Files | Tests to pin | Rollback |
|---|---|---|---|---|
| 1. Add DTOs and executor beside existing code | Create `transcription/` module with request/error/capability DTOs and an executor that initially delegates to existing helpers without deleting old paths. Keep current `TranscriptionResult` behavior (`src-tauri/src/transcription.rs:78`, `src-tauri/src/transcription.rs:90`). | `src-tauri/src/transcription.rs` or new `src-tauri/src/transcription/{mod,request,error,executor,capabilities}.rs`; `src-tauri/src/lib.rs` only for module wiring. | Existing `transcription` unit tests in `src-tauri/src/transcription.rs:129`; add focused request/error tests. | Remove new module wiring; old callsites untouched. |
| 2. Port local desktop recording path | Replace the spawned task's engine match with `TranscriptionRequest { source: DesktopRecording, audio: Path, engine: Explicit, ... }` and executor call. Preserve remote-failure history behavior. | `src-tauri/src/commands/audio.rs`, new executor files. | Plan-003 recording state characterization module (`plans/003-recording-state-characterization-tests.md:76`, `plans/003-recording-state-characterization-tests.md:80`), `src-tauri/src/tests/transcription_history.rs`, and existing audio command tests. | Revert the desktop callsite to the existing match; DTOs remain unused. |
| 3. Port upload and CLI local | Replace upload/audio-bytes local branches and CLI local `transcribe_audio_file_for_cli` use with the shared request. | `src-tauri/src/commands/audio.rs`, `src-tauri/src/cli.rs`, executor files. | `src-tauri/src/tests/audio_commands.rs`, CLI unit tests in `src-tauri/src/cli.rs:402`, and transcription result tests. | Revert upload/CLI callsites; desktop remains on executor if stage 2 passed. |
| 4. Port remote server inbound and multipart parser | Change `/api/v1/transcribe` to multipart, parse metadata into `TranscriptionRequest`, return `PublicTranscriptionError`, and use executor with `HostDefault`. | `src-tauri/src/remote/http.rs`, `src-tauri/src/remote/server.rs`, `src-tauri/src/remote/transcription.rs`, executor files. | `remote_http_tests`, `remote_transcription_tests`, and `src-tauri/src/remote/concurrent_tests.rs`; pin no-context multipart, metadata multipart, oversized metadata, auth, and error redaction. | Revert route/parser/DTO response changes; client has not moved yet. |
| 5. Port remote client and CLI remote | Make `RemoteServerConnection` serialize the shared request as multipart; update CLI remote to include context if advertised and normalize output schema. | `src-tauri/src/remote/client.rs`, `src-tauri/src/commands/audio.rs` remote builders, `src-tauri/src/cli.rs`, `src-tauri/src/remote/settings.rs` if caching capabilities. | `remote_client_tests`, CLI tests in `src-tauri/src/cli.rs:402`, remote settings tests, and remote command tests. | Revert client serializer and CLI remote path to raw-body/header protocol if stage 4 is reverted too. |
| 6. Delete legacy errors, DTOs, and header-context path | Remove `TranscriptionFailure`, `remote::client::TranscriptionRequest`, base64 `X-Voicetypr-Context`, ad-hoc string mappings, and duplicate engine-routing branches. | `src-tauri/src/commands/audio.rs`, `src-tauri/src/remote/client.rs`, `src-tauri/src/remote/http.rs`, `src-tauri/src/remote/server.rs`, `src-tauri/src/remote/transcription.rs`, `src-tauri/src/provider_capabilities.rs` or new capabilities module. | Full backend targeted suites: `remote_http_tests`, `remote_client_tests`, `remote_transcription_tests`, `transcription_history`, plan-003 recording state module, audio command tests. | Last safe rollback is stage 5. After deletion, rollback by reverting the stage-6 commit; do not reintroduce compatibility shims. |

## Open questions

1. **Should the first shipped remote protocol preserve raw-body/header compatibility?** Recommended answer: no. Clean cutover to multipart; release evidence and the deferred plan both point to remote not having shipped (`docs/plans/2026-06-07-001-v2-deferred-items-plan.md:83`, `CHANGELOG.md:1`).
2. **Should CLI JSON output schema freeze now?** Recommended answer: yes, freeze after stage 3. Use `{ text, model, engine, duration_ms?, transcript_language?, output_language?, mode?, applied_operations?, warnings?, error? }` across local and remote because current local and remote payloads differ (`src-tauri/src/cli.rs:228`, `src-tauri/src/cli.rs:383`).
3. **Should `TranscriptionFailure::Remote` retry-from-history semantics generalize to all retryable errors?** Recommended answer: yes. Preserve the recording for any `TranscriptionError { retryable: true }` when audio exists, not only remote errors, because the current retry path is tied to `TranscriptionFailure::Remote` (`src-tauri/src/commands/audio.rs:3441`, `src-tauri/src/commands/audio.rs:3778`).
4. **Should request context include Personal Library persistence metadata or snippets?** Recommended answer: no. Send only request-local terms/corrections/aliases that the peer advertises; the deferred plan explicitly forbids snippets, full writing settings, and persistence metadata (`docs/plans/2026-06-07-001-v2-deferred-items-plan.md:88`).
5. **Should model-control updates cancel queued or running jobs?** Recommended answer: no for running jobs; queued jobs should use the latest snapshot when they start. This follows the snapshot-at-start rule and current serialized transcribe guard (`src-tauri/src/remote/http.rs:462`, `src-tauri/src/remote/transcription.rs:270`).

## Superseded deferred-plan fragments

This design supersedes the contract-shaped parts of deferred item 3's capability shape and item 4's remote protocol shape (`docs/plans/2026-06-07-001-v2-deferred-items-plan.md:69`, `docs/plans/2026-06-07-001-v2-deferred-items-plan.md:82`). It keeps item 4's privacy boundary and multipart direction (`docs/plans/2026-06-07-001-v2-deferred-items-plan.md:85`, `docs/plans/2026-06-07-001-v2-deferred-items-plan.md:88`). The deferred plan's voice-command item is stale per its own later implementation note in the assignment plan; this document does not redesign voice commands.
