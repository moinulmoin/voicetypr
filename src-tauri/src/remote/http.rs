//! HTTP server implementation for remote transcription
//!
//! Uses warp to create REST API endpoints for status and transcription.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use log::{info, warn};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore};
use warp::{http::StatusCode, Filter, Rejection, Reply};

use super::server::{
    ErrorResponse, RemoteCapabilities, RemoteModelControlSnapshot, RemoteModelControlUpdate,
    StatusResponse, TranscribeResponse, REMOTE_PROTOCOL_VERSION,
};
use crate::transcription::TranscriptionResult;

/// Auth header name
const AUTH_HEADER: &str = "X-VoiceTypr-Key";
const CONTEXT_HEADER: &str = "X-VoiceTypr-Context";

const MAX_AUDIO_BODY_BYTES: u64 = 50 * 1024 * 1024; // 50 MB
const MAX_CONTEXT_HEADER_BYTES: usize = 4096;
const MAX_CONTEXT_HEADER_ENCODED_BYTES: usize = MAX_CONTEXT_HEADER_BYTES.div_ceil(3) * 4;
const MAX_CONTROL_BODY_BYTES: u64 = 4 * 1024;

/// Shared map from client IP to last-seen time for recent-client counting.
pub type ClientActivityMap = Arc<Mutex<HashMap<IpAddr, Instant>>>;

/// How long after a client's last transcription request they are counted as "recent".
pub const RECENT_CLIENT_WINDOW: std::time::Duration = std::time::Duration::from_secs(300);

/// Remove entries from `map` whose last-seen time is older than `window` relative to `now`.
///
/// Called on both write (to bound map size) and read (for the status count).
pub fn prune_stale_clients(
    map: &mut HashMap<IpAddr, Instant>,
    now: Instant,
    window: std::time::Duration,
) {
    map.retain(|_, last_seen| {
        now.checked_duration_since(*last_seen)
            .is_some_and(|elapsed| elapsed <= window)
    });
}

/// Count distinct client IPs whose last-seen time is within `window` of `now`,
/// pruning stale entries from `map` in place.
///
/// Pure function — takes explicit `now` so it can be unit-tested without timers.
pub fn count_recent_clients(
    map: &mut HashMap<IpAddr, Instant>,
    now: Instant,
    window: std::time::Duration,
) -> u32 {
    prune_stale_clients(map, now, window);
    map.len() as u32
}

/// Trait for server context (allows mocking in tests)
pub trait ServerContext: Send + Sync {
    fn get_model_name(&self) -> String;
    fn get_server_name(&self) -> String;
    fn get_password(&self) -> Option<String>;
    fn allow_model_control(&self) -> bool {
        crate::remote::model_control::is_model_control_enabled()
    }
    fn transcribe(
        &self,
        audio_data: &[u8],
        spoken_language: Option<&str>,
        transcription_task: Option<&str>,
    ) -> Result<TranscriptionResult, String>;
    fn get_engine(&self) -> String {
        "whisper".to_string()
    }
    /// Engine and model for status/capabilities in one consistent read.
    fn model_status_snapshot(&self) -> (String, String) {
        let engine = self.get_engine();
        let model = self.get_model_name();
        (engine, model)
    }
    fn transcribe_with_context(
        &self,
        audio_data: &[u8],
        spoken_language: Option<&str>,
        transcription_task: Option<&str>,
        context: Option<&str>,
    ) -> Result<crate::transcription::TranscriptionResult, String> {
        let _ = context;
        self.transcribe(audio_data, spoken_language, transcription_task)
    }
    fn get_model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
        Err("Remote model control is unavailable for this host".to_string())
    }
    fn update_shared_model(
        &self,
        _model_id: &str,
        _engine: &str,
    ) -> Result<RemoteModelControlSnapshot, String> {
        Err("Remote model control is unavailable for this host".to_string())
    }
}

/// Create all warp routes for the remote transcription API
pub fn create_routes<T: ServerContext + 'static>(
    ctx: Arc<RwLock<T>>,
    transcription_guard: Arc<Semaphore>,
    client_activity: ClientActivityMap,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let status_route = status_endpoint(ctx.clone());
    let transcribe_route = transcribe_endpoint(ctx.clone(), transcription_guard, client_activity);
    let control_models_route = control_models_endpoint(ctx);

    status_route.or(transcribe_route).or(control_models_route)
}

/// GET /api/v1/status - Returns server status and model info
fn status_endpoint<T: ServerContext + 'static>(
    ctx: Arc<RwLock<T>>,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    warp::path!("api" / "v1" / "status")
        .and(warp::get())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(with_context(ctx))
        .and_then(handle_status)
}

/// POST /api/v1/transcribe - Accepts audio and returns transcription
fn transcribe_endpoint<T: ServerContext + 'static>(
    ctx: Arc<RwLock<T>>,
    transcription_guard: Arc<Semaphore>,
    client_activity: ClientActivityMap,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    // Auth preflight runs first and carries NO permit/body filters, so an unauthenticated
    // client is rejected before it can take the single transcription permit or stream the
    // body. Authenticated (or no-password) requests fall through to `main_route`.
    let auth_error_route = warp::path!("api" / "v1" / "transcribe")
        .and(warp::post())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(with_context(ctx.clone()))
        .and_then(handle_transcribe_auth_preflight);

    let main_route = warp::path!("api" / "v1" / "transcribe")
        .and(warp::post())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(warp::header::<String>("content-type"))
        .and(warp::header::optional::<String>(
            "X-VoiceTypr-Speech-Language",
        ))
        .and(warp::header::optional::<String>(
            "X-VoiceTypr-Transcription-Task",
        ))
        .and(warp::header::optional::<String>(CONTEXT_HEADER))
        // Acquire a permit BEFORE the body is buffered: only the permit holder buffers the
        // (potentially ~50 MB) body, which bounds memory while preserving queueing.
        .and(with_transcription_permit(transcription_guard))
        .and(warp::body::content_length_limit(MAX_AUDIO_BODY_BYTES))
        .and(warp::body::bytes())
        .map(
            |auth_key,
             content_type,
             spoken_language,
             transcription_task,
             request_context,
             permit,
             body| {
                TranscribeRequestParts {
                    auth_key,
                    content_type,
                    spoken_language,
                    transcription_task,
                    request_context,
                    body,
                    permit,
                }
            },
        )
        .and(warp::filters::addr::remote())
        .and(with_context(ctx))
        .and(with_client_activity(client_activity))
        .and_then(handle_transcribe);

    auth_error_route.or(main_route)
}

/// Auth preflight for POST /api/v1/transcribe. Runs BEFORE the permit/body filters so an
/// unauthenticated client can never hold the single transcription permit or stream the body.
/// On a password-protected server a missing/wrong key returns 401 here; otherwise it rejects
/// with not_found so the request falls through to the main route. A server without a password
/// leaves transcription open and always falls through. Mirrors the handler's own auth check,
/// which stays as defense-in-depth.
async fn handle_transcribe_auth_preflight<T: ServerContext + 'static>(
    auth_key: Option<String>,
    ctx: Arc<RwLock<T>>,
) -> Result<Box<dyn Reply>, Rejection> {
    let required_password = { ctx.read().await.get_password() };
    let Some(required_password) = required_password else {
        // Open server: no password configured.
        return Err(warp::reject::not_found());
    };
    match auth_key {
        Some(provided) if auth_matches(&provided, &required_password) => {
            Err(warp::reject::not_found())
        }
        _ => Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&ErrorResponse {
                error: "unauthorized".to_string(),
            }),
            StatusCode::UNAUTHORIZED,
        ))),
    }
}

/// GET/PATCH /api/v1/control/models - Password-gated remote model control
fn control_models_endpoint<T: ServerContext + 'static>(
    ctx: Arc<RwLock<T>>,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let get_route = warp::path!("api" / "v1" / "control" / "models")
        .and(warp::get())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(with_context(ctx.clone()))
        .and_then(handle_get_model_control);

    let patch_auth_error_route = warp::path!("api" / "v1" / "control" / "models")
        .and(warp::patch())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(with_context(ctx.clone()))
        .and_then(handle_patch_control_preflight);

    let patch_route = warp::path!("api" / "v1" / "control" / "models")
        .and(warp::patch())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(with_context(ctx))
        .and_then(require_control_auth)
        .and(warp::body::content_length_limit(MAX_CONTROL_BODY_BYTES))
        .and(warp::body::json())
        .and_then(handle_patch_model_control);

    get_route.or(patch_auth_error_route).or(patch_route)
}

/// Helper to inject context into handlers
fn with_context<T: ServerContext + 'static>(
    ctx: Arc<RwLock<T>>,
) -> impl Filter<Extract = (Arc<RwLock<T>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || ctx.clone())
}

/// Acquire an owned transcription permit, waiting if all permits are in use.
/// This filter runs BEFORE the body filter, so a queued request never buffers
/// the (potentially ~50 MB) request body — only the permit holder buffers,
/// which bounds memory while preserving queueing instead of rejecting callers.
fn with_transcription_permit(
    guard: Arc<Semaphore>,
) -> impl Filter<Extract = (OwnedSemaphorePermit,), Error = Rejection> + Clone {
    warp::any().and_then(move || {
        let guard = guard.clone();
        async move {
            guard
                .acquire_owned()
                .await
                .map_err(|_| warp::reject::reject())
        }
    })
}

/// Helper to inject the client-activity map into the transcribe handler
fn with_client_activity(
    map: ClientActivityMap,
) -> impl Filter<Extract = (ClientActivityMap,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || map.clone())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let max_len = a.len().max(b.len());
    let mut diff = a.len() ^ b.len();

    for i in 0..max_len {
        let a_byte = a.get(i).copied().unwrap_or(0);
        let b_byte = b.get(i).copied().unwrap_or(0);
        diff |= usize::from(a_byte ^ b_byte);
    }

    diff == 0
}

fn auth_matches(provided: &str, required: &str) -> bool {
    constant_time_eq(provided.as_bytes(), required.as_bytes())
}

async fn require_control_auth<T: ServerContext + 'static>(
    auth_key: Option<String>,
    ctx: Arc<RwLock<T>>,
) -> Result<Arc<RwLock<T>>, Rejection> {
    let (password, allow_model_control) = {
        let ctx = ctx.read().await;
        (ctx.get_password(), ctx.allow_model_control())
    };

    check_control_auth(&password, allow_model_control, auth_key)
        .map_err(|_| warp::reject::not_found())?;

    Ok(ctx)
}

async fn handle_patch_control_preflight<T: ServerContext + 'static>(
    auth_key: Option<String>,
    ctx: Arc<RwLock<T>>,
) -> Result<Box<dyn Reply>, Rejection> {
    let (password, allow_model_control) = {
        let ctx = ctx.read().await;
        (ctx.get_password(), ctx.allow_model_control())
    };

    match check_control_auth(&password, allow_model_control, auth_key) {
        Ok(()) => Err(warp::reject::not_found()),
        Err(failure) => Ok(control_auth_error(failure)),
    }
}

fn control_requires_password(password: &Option<String>) -> bool {
    password.as_ref().is_some_and(|value| !value.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlAuthFailure {
    RequiresPassword,
    ModelControlDisabled,
    Unauthorized,
}

fn check_control_auth(
    password: &Option<String>,
    allow_model_control: bool,
    auth_key: Option<String>,
) -> Result<(), ControlAuthFailure> {
    if !control_requires_password(password) {
        return Err(ControlAuthFailure::RequiresPassword);
    }

    if !allow_model_control {
        return Err(ControlAuthFailure::ModelControlDisabled);
    }

    let required = password.as_ref().expect("password checked above");
    match auth_key {
        Some(provided) if auth_matches(&provided, required) => Ok(()),
        _ => Err(ControlAuthFailure::Unauthorized),
    }
}

fn control_auth_error(failure: ControlAuthFailure) -> Box<dyn Reply> {
    let (status, error) = match failure {
        ControlAuthFailure::RequiresPassword => {
            (StatusCode::FORBIDDEN, "control_requires_password")
        }
        ControlAuthFailure::ModelControlDisabled => {
            (StatusCode::FORBIDDEN, "model_control_disabled")
        }
        ControlAuthFailure::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
    };
    Box::new(warp::reply::with_status(
        warp::reply::json(&ErrorResponse {
            error: error.to_string(),
        }),
        status,
    ))
}

/// Handle GET /api/v1/control/models
async fn handle_get_model_control<T: ServerContext + 'static>(
    auth_key: Option<String>,
    ctx: Arc<RwLock<T>>,
) -> Result<Box<dyn Reply>, Rejection> {
    let ctx = ctx.read().await;
    if let Err(failure) =
        check_control_auth(&ctx.get_password(), ctx.allow_model_control(), auth_key)
    {
        return Ok(control_auth_error(failure));
    }

    match ctx.get_model_control_snapshot() {
        Ok(snapshot) => Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&snapshot),
            StatusCode::OK,
        ))),
        Err(error) => Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&ErrorResponse { error }),
            StatusCode::INTERNAL_SERVER_ERROR,
        ))),
    }
}

/// Handle PATCH /api/v1/control/models
async fn handle_patch_model_control<T: ServerContext + 'static>(
    ctx: Arc<RwLock<T>>,
    update: RemoteModelControlUpdate,
) -> Result<Box<dyn Reply>, Rejection> {
    let ctx = ctx.write().await;

    match ctx.update_shared_model(&update.model, &update.engine) {
        Ok(snapshot) => Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&snapshot),
            StatusCode::OK,
        ))),
        Err(error) => Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&ErrorResponse { error }),
            StatusCode::BAD_REQUEST,
        ))),
    }
}

/// Handle GET /api/v1/status
async fn handle_status<T: ServerContext + 'static>(
    auth_key: Option<String>,
    ctx: Arc<RwLock<T>>,
) -> Result<impl Reply, Rejection> {
    let ctx = ctx.read().await;
    let server_name = ctx.get_server_name();

    info!(
        "[Remote Server] Status request received on '{}'",
        server_name
    );

    if let Some(required_password) = ctx.get_password() {
        match auth_key {
            Some(provided) if auth_matches(&provided, &required_password) => {
                info!("[Remote Server] Status request authenticated successfully");
            }
            _ => {
                warn!(
                    "[Remote Server] Status request REJECTED - authentication failed on '{}'",
                    server_name
                );
                return Ok(warp::reply::with_status(
                    warp::reply::json(&ErrorResponse {
                        error: "unauthorized".to_string(),
                    }),
                    StatusCode::UNAUTHORIZED,
                ));
            }
        }
    }

    // Get unique machine ID to allow clients to detect self-connection
    let machine_id =
        crate::license::device::get_device_hash().unwrap_or_else(|_| "unknown".to_string());
    let (engine, model) = ctx.model_status_snapshot();
    let response = StatusResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        model,
        name: ctx.get_server_name(),
        machine_id,
        protocol_version: REMOTE_PROTOCOL_VERSION,
        engine: Some(engine.clone()),
        capabilities: Some(remote_capabilities_for_engine(&engine)),
    };

    info!(
        "[Remote Server] Status response sent: model='{}', server='{}'",
        response.model, response.name
    );

    Ok(warp::reply::with_status(
        warp::reply::json(&response),
        StatusCode::OK,
    ))
}

fn remote_capabilities_for_engine(engine: &str) -> RemoteCapabilities {
    match crate::provider_capabilities::capabilities_for_engine(engine) {
        Some(capabilities) => RemoteCapabilities {
            supports_initial_prompt: capabilities.supports_initial_prompt,
            supports_structured_terms: capabilities.supports_structured_terms,
            supports_vocabulary_terms: capabilities.supports_vocabulary_terms,
            accepts_request_context: capabilities.supports_initial_prompt,
            max_context_bytes: if capabilities.supports_initial_prompt {
                900
            } else {
                0
            },
            acceleration: vec!["cpu".to_string()],
        },
        None => RemoteCapabilities {
            acceleration: vec!["cpu".to_string()],
            ..RemoteCapabilities::default()
        },
    }
}

fn decode_context_header(encoded_context: Option<String>) -> Option<String> {
    let encoded_context = encoded_context?;
    let encoded_len = encoded_context.len();

    if encoded_len > MAX_CONTEXT_HEADER_ENCODED_BYTES {
        warn!(
            "[Remote Server] Dropping oversized encoded transcription context header: {} bytes",
            encoded_len
        );
        return None;
    }

    let decoded = match BASE64.decode(encoded_context.as_bytes()) {
        Ok(decoded) => decoded,
        Err(_) => {
            warn!("[Remote Server] Dropping invalid base64 transcription context header");
            return None;
        }
    };

    let decoded_len = decoded.len();
    if decoded_len > MAX_CONTEXT_HEADER_BYTES {
        warn!(
            "[Remote Server] Dropping oversized transcription context header: {} decoded bytes",
            decoded_len
        );
        return None;
    }

    match String::from_utf8(decoded) {
        Ok(context) => Some(context),
        Err(_) => {
            warn!("[Remote Server] Dropping non-UTF-8 transcription context header");
            None
        }
    }
}

struct TranscribeRequestParts {
    auth_key: Option<String>,
    content_type: String,
    spoken_language: Option<String>,
    transcription_task: Option<String>,
    request_context: Option<String>,
    body: bytes::Bytes,
    /// Owned semaphore permit acquired before the body was buffered.
    /// Held for the full duration of the blocking transcription work.
    permit: OwnedSemaphorePermit,
}

/// Handle POST /api/v1/transcribe
async fn handle_transcribe<T: ServerContext + 'static>(
    parts: TranscribeRequestParts,
    client_addr: Option<SocketAddr>,
    ctx: Arc<RwLock<T>>,
    client_activity: ClientActivityMap,
) -> Result<impl Reply, Rejection> {
    let TranscribeRequestParts {
        auth_key,
        content_type,
        spoken_language,
        transcription_task,
        request_context,
        body,
        permit,
    } = parts;

    let audio_size_kb = body.len() as f64 / 1024.0;
    let (server_name, model_name, required_password) = {
        let ctx = ctx.read().await;
        (
            ctx.get_server_name(),
            ctx.get_model_name(),
            ctx.get_password(),
        )
    };

    info!(
        "🎙️ [Remote Server] Transcription request received on '{}': {:.1} KB audio, content-type='{}'",
        server_name, audio_size_kb, content_type
    );

    // Check authentication
    if let Some(required_password) = required_password {
        match auth_key {
            Some(provided) if auth_matches(&provided, &required_password) => {
                info!("[Remote Server] Transcription request authenticated successfully");
            }
            _ => {
                warn!(
                    "[Remote Server] Transcription request REJECTED - authentication failed on '{}'",
                    server_name
                );
                return Ok(warp::reply::with_status(
                    warp::reply::json(&ErrorResponse {
                        error: "unauthorized".to_string(),
                    }),
                    StatusCode::UNAUTHORIZED,
                ));
            }
        }
    }

    // Validate content type
    if !content_type.starts_with("audio/") {
        warn!(
            "[Remote Server] Transcription request REJECTED - unsupported content type: '{}'",
            content_type
        );
        return Ok(warp::reply::with_status(
            warp::reply::json(&ErrorResponse {
                error: "unsupported_media_type".to_string(),
            }),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ));
    }

    // Record client IP for active-connections counting (rolling-window distinct clients).
    // We record after auth + content-type pass so only genuine requests are counted.
    if let Some(addr) = client_addr {
        if let Ok(mut map) = client_activity.lock() {
            let now = Instant::now();
            // Prune stale entries on every write so the map is bounded to
            // distinct-recent IPs regardless of how often get_status() is called.
            prune_stale_clients(&mut map, now, RECENT_CLIENT_WINDOW);
            map.insert(addr.ip(), now);
        }
    }

    info!(
        "[Remote Server] Starting transcription with model '{}' for {:.1} KB audio",
        model_name, audio_size_kb
    );
    let request_context = decode_context_header(request_context);

    // Perform transcription on a blocking thread: transcribe_with_context is
    // synchronous CPU work (full Whisper/Parakeet run) and must not pin a
    // runtime worker. Hold the owned permit for the full blocking work so
    // client disconnect / request-future drop cannot release serialization early.
    let ctx_for_blocking = ctx.clone();
    let body_for_blocking = body.clone();
    let transcription_outcome = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let guard = ctx_for_blocking.blocking_read();
        guard.transcribe_with_context(
            &body_for_blocking,
            spoken_language.as_deref(),
            transcription_task.as_deref(),
            request_context.as_deref(),
        )
    })
    .await
    .unwrap_or_else(|join_err| Err(format!("transcription task failed: {join_err}")));

    match transcription_outcome {
        Ok(result) => {
            let response = TranscribeResponse {
                text: result.raw_text,
                duration_ms: result.timings.processing_duration_ms.unwrap_or_default(),
                model: result.model,
                transcript_language: result.transcript_language,
            };
            info!(
                "🎯 [Remote Server] Transcription COMPLETED on '{}': {} chars in {}ms using '{}'",
                server_name,
                response.text.len(),
                response.duration_ms,
                response.model
            );
            Ok(warp::reply::with_status(
                warp::reply::json(&response),
                StatusCode::OK,
            ))
        }
        Err(error) => {
            warn!(
                "[Remote Server] Transcription FAILED on '{}': {}",
                server_name, error
            );
            Ok(warp::reply::with_status(
                warp::reply::json(&ErrorResponse {
                    error: "transcription_failed".to_string(),
                }),
                StatusCode::INTERNAL_SERVER_ERROR,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::server::ShareableRemoteModelInfo;

    fn mock_model_control_snapshot(model_name: &str) -> RemoteModelControlSnapshot {
        RemoteModelControlSnapshot {
            current: ShareableRemoteModelInfo {
                id: model_name.to_string(),
                display_name: format!("{model_name} display"),
                engine: "whisper".to_string(),
                recommended: Some(true),
                speed_score: Some(8),
                accuracy_score: Some(7),
            },
            available: vec![ShareableRemoteModelInfo {
                id: model_name.to_string(),
                display_name: format!("{model_name} display"),
                engine: "whisper".to_string(),
                recommended: Some(true),
                speed_score: Some(8),
                accuracy_score: Some(7),
            }],
        }
    }
    fn test_transcription_guard() -> Arc<Semaphore> {
        Arc::new(Semaphore::new(1))
    }

    fn test_client_activity() -> ClientActivityMap {
        Arc::new(Mutex::new(std::collections::HashMap::new()))
    }

    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::time::sleep;

    struct MockContext;

    impl ServerContext for MockContext {
        fn get_model_name(&self) -> String {
            "mock-model".to_string()
        }
        fn get_server_name(&self) -> String {
            "mock-server".to_string()
        }
        fn get_password(&self) -> Option<String> {
            None
        }
        fn transcribe(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                "mock-model",
                None,
                false,
            );
            Ok(
                crate::transcription::TranscriptionResult::new(&job, "mock transcription")
                    .with_processing_duration_ms(Some(100)),
            )
        }

        fn get_model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
            Ok(mock_model_control_snapshot("mock-model"))
        }
        fn update_shared_model(
            &self,
            model_id: &str,
            engine: &str,
        ) -> Result<RemoteModelControlSnapshot, String> {
            if engine != "whisper" && engine != "parakeet" {
                return Err(format!("Unsupported sharing engine '{engine}'"));
            }
            Ok(mock_model_control_snapshot(model_id))
        }
    }

    /// Mock context with configurable delay to simulate transcription time
    struct DelayedMockContext {
        delay_ms: u64,
        request_counter: AtomicU32,
        active_transcriptions: AtomicU32,
        max_concurrent: AtomicU32,
    }

    impl DelayedMockContext {
        fn new(delay_ms: u64) -> Self {
            Self {
                delay_ms,
                request_counter: AtomicU32::new(0),
                active_transcriptions: AtomicU32::new(0),
                max_concurrent: AtomicU32::new(0),
            }
        }

        fn max_concurrent_transcriptions(&self) -> u32 {
            self.max_concurrent.load(Ordering::SeqCst)
        }
    }

    impl ServerContext for DelayedMockContext {
        fn get_model_name(&self) -> String {
            "delayed-mock-model".to_string()
        }
        fn get_server_name(&self) -> String {
            "delayed-mock-server".to_string()
        }
        fn get_password(&self) -> Option<String> {
            None
        }
        fn transcribe(
            &self,
            audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            // Increment request counter
            let request_num = self.request_counter.fetch_add(1, Ordering::SeqCst) + 1;

            let current = self.active_transcriptions.fetch_add(1, Ordering::SeqCst) + 1;
            let mut max = self.max_concurrent.load(Ordering::SeqCst);
            while current > max {
                match self.max_concurrent.compare_exchange(
                    max,
                    current,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => break,
                    Err(actual) => max = actual,
                }
            }

            // Simulate transcription delay (blocking, as real transcription would be)
            std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));

            self.active_transcriptions.fetch_sub(1, Ordering::SeqCst);

            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                "delayed-mock-model",
                None,
                false,
            );

            Ok(crate::transcription::TranscriptionResult::new(
                &job,
                format!("transcription-{}-len-{}", request_num, audio_data.len()),
            )
            .with_processing_duration_ms(Some(self.delay_ms)))
        }

        fn get_model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
            Ok(mock_model_control_snapshot("delayed-mock-model"))
        }
        fn update_shared_model(
            &self,
            model_id: &str,
            engine: &str,
        ) -> Result<RemoteModelControlSnapshot, String> {
            if engine != "whisper" && engine != "parakeet" {
                return Err(format!("Unsupported sharing engine '{engine}'"));
            }
            Ok(mock_model_control_snapshot(model_id))
        }
    }

    /// Mock context that fails on specific request numbers
    struct FailingMockContext {
        fail_on_requests: Vec<u32>,
        request_counter: AtomicU32,
    }

    impl FailingMockContext {
        fn new(fail_on_requests: Vec<u32>) -> Self {
            Self {
                fail_on_requests,
                request_counter: AtomicU32::new(0),
            }
        }
    }

    impl ServerContext for FailingMockContext {
        fn get_model_name(&self) -> String {
            "failing-mock-model".to_string()
        }
        fn get_server_name(&self) -> String {
            "failing-mock-server".to_string()
        }
        fn get_password(&self) -> Option<String> {
            None
        }
        fn transcribe(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            let request_num = self.request_counter.fetch_add(1, Ordering::SeqCst) + 1;

            if self.fail_on_requests.contains(&request_num) {
                return Err(format!("Simulated failure on request {}", request_num));
            }

            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                "failing-mock-model",
                None,
                false,
            );

            Ok(crate::transcription::TranscriptionResult::new(
                &job,
                format!("success-{}", request_num),
            )
            .with_processing_duration_ms(Some(10)))
        }

        fn get_model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
            Ok(mock_model_control_snapshot("failing-mock-model"))
        }
        fn update_shared_model(
            &self,
            model_id: &str,
            engine: &str,
        ) -> Result<RemoteModelControlSnapshot, String> {
            if engine != "whisper" && engine != "parakeet" {
                return Err(format!("Unsupported sharing engine '{engine}'"));
            }
            Ok(mock_model_control_snapshot(model_id))
        }
    }

    #[test]
    fn test_mock_context() {
        let ctx = MockContext;
        assert_eq!(ctx.get_model_name(), "mock-model");
        assert_eq!(ctx.get_server_name(), "mock-server");
        assert!(ctx.get_password().is_none());
    }
    struct ContextCaptureMock {
        captured_context: Arc<Mutex<Option<String>>>,
    }

    impl ServerContext for ContextCaptureMock {
        fn get_model_name(&self) -> String {
            "mock-model".to_string()
        }

        fn get_server_name(&self) -> String {
            "mock-server".to_string()
        }

        fn get_password(&self) -> Option<String> {
            None
        }

        fn transcribe(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            Err("transcribe_with_context should be used".to_string())
        }

        fn transcribe_with_context(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
            context: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            *self.captured_context.lock().unwrap() = context.map(str::to_string);

            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                "mock-model",
                None,
                false,
            );
            Ok(crate::transcription::TranscriptionResult::new(
                &job,
                "mock transcription",
            ))
        }
    }

    struct ConsistentStatusMock;

    impl ServerContext for ConsistentStatusMock {
        fn get_model_name(&self) -> String {
            "stale-model".to_string()
        }

        fn get_server_name(&self) -> String {
            "consistent-server".to_string()
        }

        fn get_password(&self) -> Option<String> {
            None
        }

        fn get_engine(&self) -> String {
            "whisper".to_string()
        }

        fn model_status_snapshot(&self) -> (String, String) {
            ("parakeet".to_string(), "nano".to_string())
        }

        fn transcribe(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            Err("unused".to_string())
        }
    }

    #[tokio::test]
    async fn test_status_uses_single_model_status_snapshot() {
        let ctx = Arc::new(RwLock::new(ConsistentStatusMock));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("GET")
            .path("/api/v1/status")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);

        let body: StatusResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.model, "nano");
        assert_eq!(body.engine.as_deref(), Some("parakeet"));
        let caps = body.capabilities.expect("capabilities");
        assert!(
            !caps.accepts_request_context,
            "parakeet capabilities must not use whisper initial-prompt caps"
        );
    }

    struct SlowTranscribeMock {
        started: Arc<AtomicBool>,
        finished: Arc<AtomicBool>,
    }

    impl ServerContext for SlowTranscribeMock {
        fn get_model_name(&self) -> String {
            "slow-model".to_string()
        }

        fn get_server_name(&self) -> String {
            "slow-server".to_string()
        }

        fn get_password(&self) -> Option<String> {
            None
        }

        fn transcribe(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            Err("use transcribe_with_context".to_string())
        }

        fn transcribe_with_context(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
            _context: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            self.started.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(800));
            self.finished.store(true, Ordering::SeqCst);
            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                "slow-model",
                None,
                false,
            );
            Ok(crate::transcription::TranscriptionResult::new(&job, "done"))
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn transcribe_owned_permit_held_when_handler_future_dropped() {
        let started = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let guard = test_transcription_guard();
        let ctx = Arc::new(RwLock::new(SlowTranscribeMock {
            started: started.clone(),
            finished: finished.clone(),
        }));

        let ctx_for_handler = ctx.clone();
        // Acquire the permit explicitly so the test can hand it to the handler
        // (mirroring what the filter chain does before the body is read).
        let permit = guard
            .clone()
            .try_acquire_owned()
            .expect("semaphore has capacity");
        let handler_task = tokio::spawn(async move {
            handle_transcribe(
                TranscribeRequestParts {
                    auth_key: None,
                    content_type: "audio/wav".to_string(),
                    spoken_language: None,
                    transcription_task: None,
                    request_context: None,
                    body: bytes::Bytes::from_static(b"audio"),
                    permit,
                },
                None, // client_addr — not relevant to this permit-hold test
                ctx_for_handler,
                test_client_activity(),
            )
            .await
        });

        tokio::time::timeout(Duration::from_secs(2), async {
            while !started.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("transcription should start");

        handler_task.abort();
        let _ = handler_task.await;

        let acquire_started = std::time::Instant::now();
        let permit = guard
            .acquire_owned()
            .await
            .expect("semaphore should remain held until blocking work completes");
        drop(permit);

        assert!(
            acquire_started.elapsed() >= Duration::from_millis(300),
            "dropping the handler future must not release the transcription permit early"
        );
        assert!(
            finished.load(Ordering::SeqCst),
            "blocking transcription should run to completion"
        );
    }

    #[tokio::test]
    async fn test_status_advertises_protocol_engine_and_capabilities() {
        let ctx = Arc::new(RwLock::new(MockContext));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("GET")
            .path("/api/v1/status")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);

        let body: StatusResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.protocol_version, REMOTE_PROTOCOL_VERSION);
        assert_eq!(body.engine.as_deref(), Some("whisper"));
        assert!(
            body.capabilities
                .as_ref()
                .expect("capabilities should be advertised")
                .accepts_request_context
        );
    }

    #[tokio::test]
    async fn test_transcribe_forwards_context_header() {
        let captured_context = Arc::new(Mutex::new(None));
        let ctx = Arc::new(RwLock::new(ContextCaptureMock {
            captured_context: captured_context.clone(),
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let context = "project glossary: José 中";
        let encoded_context = BASE64.encode(context.as_bytes());

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .header(CONTEXT_HEADER, encoded_context)
            .body(b"audio")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(captured_context.lock().unwrap().as_deref(), Some(context));
    }

    #[tokio::test]
    async fn test_transcribe_without_context_header_passes_none() {
        let captured_context = Arc::new(Mutex::new(Some("stale".to_string())));
        let ctx = Arc::new(RwLock::new(ContextCaptureMock {
            captured_context: captured_context.clone(),
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .body(b"audio")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(captured_context.lock().unwrap().as_deref(), None);
    }

    #[tokio::test]
    async fn test_transcribe_drops_oversized_context_header() {
        let captured_context = Arc::new(Mutex::new(Some("stale".to_string())));
        let ctx = Arc::new(RwLock::new(ContextCaptureMock {
            captured_context: captured_context.clone(),
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());
        let oversized_context = BASE64.encode(vec![b'a'; MAX_CONTEXT_HEADER_BYTES + 1]);

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .header(CONTEXT_HEADER, oversized_context)
            .body(b"audio")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(captured_context.lock().unwrap().as_deref(), None);
    }

    #[tokio::test]
    async fn test_transcribe_drops_invalid_base64_context() {
        let captured_context = Arc::new(Mutex::new(Some("stale".to_string())));
        let ctx = Arc::new(RwLock::new(ContextCaptureMock {
            captured_context: captured_context.clone(),
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .header(CONTEXT_HEADER, "not-valid-base64!")
            .body(b"audio")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(captured_context.lock().unwrap().as_deref(), None);
    }

    // ============================================================================
    // Regression Tests for P1/P2 fixes
    // ============================================================================

    #[tokio::test]
    async fn test_transcribe_endpoint_rejects_oversized_body() {
        let ctx = Arc::new(RwLock::new(MockContext));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let oversized_audio = vec![0u8; MAX_AUDIO_BODY_BYTES as usize + 1];

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .body(oversized_audio)
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 413);
    }

    struct Utf8PreviewContext;

    impl ServerContext for Utf8PreviewContext {
        fn get_model_name(&self) -> String {
            "utf8-preview-model".to_string()
        }

        fn get_server_name(&self) -> String {
            "utf8-preview-server".to_string()
        }

        fn get_password(&self) -> Option<String> {
            None
        }

        fn transcribe(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                "utf8-preview-model",
                None,
                false,
            );
            Ok(
                crate::transcription::TranscriptionResult::new(
                    &job,
                    format!("{}é", "a".repeat(99)),
                )
                .with_processing_duration_ms(Some(1)),
            )
        }

        fn get_model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
            Ok(mock_model_control_snapshot("utf8-preview-model"))
        }
        fn update_shared_model(
            &self,
            model_id: &str,
            engine: &str,
        ) -> Result<RemoteModelControlSnapshot, String> {
            if engine != "whisper" && engine != "parakeet" {
                return Err(format!("Unsupported sharing engine '{engine}'"));
            }
            Ok(mock_model_control_snapshot(model_id))
        }
    }

    #[tokio::test]
    async fn test_transcribe_endpoint_handles_multibyte_preview_safely() {
        let ctx = Arc::new(RwLock::new(Utf8PreviewContext));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .body(b"audio")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);

        let body: TranscribeResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.text, format!("{}é", "a".repeat(99)));
    }

    #[tokio::test]
    async fn test_status_requests_are_not_blocked_by_transcription() {
        let context = Arc::new(RwLock::new(DelayedMockContext::new(200)));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let status_url = format!("http://{}/api/v1/status", addr);

        let transcribe_handle = tokio::spawn({
            let client = client.clone();
            let url = transcribe_url.clone();
            async move {
                client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(b"audio".as_slice())
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await
            }
        });

        sleep(Duration::from_millis(50)).await;

        let status_start = std::time::Instant::now();
        let status_response = client
            .get(&status_url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .expect("Status request failed");
        let status_elapsed = status_start.elapsed();

        assert!(status_response.status().is_success());
        assert!(
            status_elapsed < Duration::from_millis(100),
            "Status request should not wait for transcription; took {:?}",
            status_elapsed
        );

        let transcribe_response = transcribe_handle
            .await
            .expect("Transcribe task panicked")
            .expect("Transcribe request failed");
        assert!(transcribe_response.status().is_success());

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn transcribe_runs_off_runtime_worker() {
        let context = Arc::new(RwLock::new(DelayedMockContext::new(300)));
        let routes = create_routes(
            context.clone(),
            test_transcription_guard(),
            test_client_activity(),
        );

        let transcribe_routes = routes.clone();
        let started_at = std::time::Instant::now();
        let transcribe_handle = tokio::spawn(async move {
            warp::test::request()
                .method("POST")
                .path("/api/v1/transcribe")
                .header("Content-Type", "audio/wav")
                .body(b"audio")
                .reply(&transcribe_routes)
                .await
        });

        tokio::time::timeout(Duration::from_millis(150), async {
            loop {
                if context.read().await.request_counter.load(Ordering::SeqCst) > 0 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("transcription should start without pinning the runtime worker");

        let status_response = tokio::time::timeout(
            Duration::from_millis(150),
            warp::test::request()
                .method("GET")
                .path("/api/v1/status")
                .reply(&routes),
        )
        .await
        .expect("status route should respond while transcription is in flight");

        assert_eq!(status_response.status(), 200);
        assert!(
            started_at.elapsed() < Duration::from_millis(150),
            "status route waited for blocking transcription to finish"
        );

        let transcribe_response = transcribe_handle.await.expect("transcribe task panicked");
        assert_eq!(transcribe_response.status(), 200);
    }

    #[tokio::test]
    async fn test_concurrent_transcribe_requests_serialize() {
        let context = Arc::new(RwLock::new(DelayedMockContext::new(100)));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let audio_data = b"audio".to_vec();

        let req1 = {
            let client = client.clone();
            let url = transcribe_url.clone();
            let audio_data = audio_data.clone();
            tokio::spawn(async move {
                client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data)
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await
            })
        };
        let req2 = {
            let client = client.clone();
            let url = transcribe_url.clone();
            let audio_data = audio_data.clone();
            tokio::spawn(async move {
                client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data)
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await
            })
        };

        let start = std::time::Instant::now();
        let (result1, result2) = tokio::join!(req1, req2);
        let elapsed = start.elapsed();

        assert!(result1
            .expect("task 1 panicked")
            .expect("request 1 failed")
            .status()
            .is_success());
        assert!(result2
            .expect("task 2 panicked")
            .expect("request 2 failed")
            .status()
            .is_success());
        assert!(
            elapsed >= Duration::from_millis(180),
            "Concurrent transcribe requests should serialize; elapsed {:?}",
            elapsed
        );

        let ctx = context.read().await;
        assert_eq!(
            ctx.max_concurrent_transcriptions(),
            1,
            "Only one transcription should run at a time"
        );

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }

    #[tokio::test]
    async fn test_transcription_guard_is_shared_across_route_instances() {
        let context = Arc::new(RwLock::new(DelayedMockContext::new(100)));
        let guard = Arc::new(Semaphore::new(1));
        let routes_a = create_routes(context.clone(), guard.clone(), test_client_activity());
        let routes_b = create_routes(context.clone(), guard, test_client_activity());

        let req_a = tokio::spawn(async move {
            warp::test::request()
                .method("POST")
                .path("/api/v1/transcribe")
                .header("Content-Type", "audio/wav")
                .body(b"audio-a")
                .reply(&routes_a)
                .await
        });
        let req_b = tokio::spawn(async move {
            warp::test::request()
                .method("POST")
                .path("/api/v1/transcribe")
                .header("Content-Type", "audio/wav")
                .body(b"audio-b")
                .reply(&routes_b)
                .await
        });

        let (response_a, response_b) = tokio::join!(req_a, req_b);

        assert_eq!(response_a.expect("request A panicked").status(), 200);
        assert_eq!(response_b.expect("request B panicked").status(), 200);
        assert_eq!(context.read().await.max_concurrent_transcriptions(), 1);
    }

    #[tokio::test]
    async fn test_graceful_shutdown_drains_in_flight_transcription() {
        let context = Arc::new(RwLock::new(DelayedMockContext::new(250)));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let mut server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );
            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("server failed to start");
        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let request = tokio::spawn(async move {
            client
                .post(&transcribe_url)
                .header("Content-Type", "audio/wav")
                .body(b"audio".to_vec())
                .timeout(Duration::from_secs(5))
                .send()
                .await
        });

        tokio::time::timeout(Duration::from_millis(150), async {
            loop {
                if context.read().await.request_counter.load(Ordering::SeqCst) > 0 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("transcription should start before shutdown");

        let _ = shutdown_tx.send(());
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut server_handle)
                .await
                .is_err(),
            "server future resolved before in-flight transcription drained"
        );

        let response = request
            .await
            .expect("request task panicked")
            .expect("request failed");
        assert_eq!(response.status(), 200);
        drop(response);
        tokio::time::timeout(Duration::from_secs(5), server_handle)
            .await
            .expect("server should finish after in-flight request drains")
            .expect("server task panicked");
    }

    // ============================================================================
    // Rapid Sequential Requests Tests (Issue #2)
    // ============================================================================

    /// Test that multiple rapid requests all complete successfully
    /// Verifies request queuing behavior under load
    #[tokio::test]
    async fn test_rapid_sequential_requests_all_complete() {
        // Create context with small delay to simulate work
        let context = Arc::new(RwLock::new(DelayedMockContext::new(10)));

        // Start server
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let num_requests = 5;

        // Send rapid sequential requests
        let mut handles = Vec::new();
        for i in 0..num_requests {
            let client = client.clone();
            let url = transcribe_url.clone();
            let audio_data = format!("audio-data-{}", i).into_bytes();

            handles.push(tokio::spawn(async move {
                client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data)
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await
            }));
        }

        // Wait for all requests to complete
        let mut success_count = 0;
        for handle in handles {
            let result = handle.await.expect("Task panicked");
            match result {
                Ok(response) if response.status().is_success() => {
                    success_count += 1;
                }
                Ok(response) => {
                    panic!("Request failed with status: {}", response.status());
                }
                Err(e) => {
                    panic!("Request error: {}", e);
                }
            }
        }

        assert_eq!(
            success_count, num_requests,
            "All {} requests should complete successfully",
            num_requests
        );

        // Verify request counter
        let ctx = context.read().await;
        assert_eq!(
            ctx.request_counter.load(Ordering::SeqCst),
            num_requests as u32,
            "Request counter should match number of requests"
        );
        drop(ctx);

        // Shutdown
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }

    /// Test that responses contain correct data for each request
    /// Verifies no data corruption or mixing between requests
    #[tokio::test]
    async fn test_rapid_requests_return_correct_responses() {
        let context = Arc::new(RwLock::new(DelayedMockContext::new(5)));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);

        // Send requests with different audio data sizes
        let audio_sizes = [100, 200, 300, 400, 500];
        let mut handles = Vec::new();

        for size in audio_sizes {
            let client = client.clone();
            let url = transcribe_url.clone();
            let audio_data = vec![0u8; size];

            handles.push(tokio::spawn(async move {
                let response = client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data.clone())
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await
                    .expect("Request failed");

                let json: serde_json::Value = response.json().await.expect("Failed to parse JSON");
                (size, json)
            }));
        }

        // Collect and verify responses
        let mut responses: Vec<(usize, serde_json::Value)> = Vec::new();
        for handle in handles {
            let (size, json) = handle.await.expect("Task panicked");
            responses.push((size, json));
        }

        // Verify each response contains expected data
        for (size, json) in &responses {
            let text = json["text"].as_str().expect("Missing text field");
            assert!(
                text.contains(&format!("len-{}", size)),
                "Response should contain audio size: expected len-{}, got {}",
                size,
                text
            );
        }

        // Verify we got unique request numbers (no duplicates)
        let request_nums: Vec<&str> = responses
            .iter()
            .filter_map(|(_, json)| json["text"].as_str())
            .filter_map(|text| text.split('-').nth(1))
            .collect();
        let unique_nums: std::collections::HashSet<_> = request_nums.iter().collect();
        assert_eq!(
            request_nums.len(),
            unique_nums.len(),
            "All request numbers should be unique (no duplicates)"
        );

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }

    /// Test that an error in one request doesn't affect subsequent requests
    /// Verifies error isolation and server resilience
    #[tokio::test]
    async fn test_error_in_one_request_doesnt_affect_others() {
        // Fail on request 2 and 4
        let context = Arc::new(RwLock::new(FailingMockContext::new(vec![2, 4])));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);

        // Send 5 sequential requests (2 and 4 will fail)
        let mut results = Vec::new();
        for i in 1..=5 {
            let response = client
                .post(&transcribe_url)
                .header("Content-Type", "audio/wav")
                .body(format!("audio-{}", i))
                .timeout(Duration::from_secs(5))
                .send()
                .await
                .expect("Request failed to send");

            results.push((i, response.status().is_success()));
        }

        // Verify expected results
        assert!(results[0].1, "Request 1 should succeed");
        assert!(!results[1].1, "Request 2 should fail");
        assert!(results[2].1, "Request 3 should succeed (after failure)");
        assert!(!results[3].1, "Request 4 should fail");
        assert!(results[4].1, "Request 5 should succeed (after failure)");

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }

    /// Test high load with many concurrent requests
    /// Verifies server stability and no data corruption under stress
    #[tokio::test]
    async fn test_high_load_no_data_corruption() {
        let context = Arc::new(RwLock::new(DelayedMockContext::new(2)));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let num_requests = 20;

        // Send many concurrent requests
        let mut handles = Vec::new();
        for i in 0..num_requests {
            let client = client.clone();
            let url = transcribe_url.clone();
            // Each request has unique size for identification
            let audio_data = vec![i as u8; (i + 1) * 10];

            handles.push(tokio::spawn(async move {
                client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data)
                    .timeout(Duration::from_secs(30))
                    .send()
                    .await
            }));
        }

        // Wait for all to complete
        let mut success_count = 0;
        let mut response_texts = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(Ok(response)) if response.status().is_success() => {
                    success_count += 1;
                    if let Ok(json) = response.json::<serde_json::Value>().await {
                        if let Some(text) = json["text"].as_str() {
                            response_texts.push(text.to_string());
                        }
                    }
                }
                Ok(Ok(response)) => {
                    panic!("Request failed with status: {}", response.status());
                }
                Ok(Err(e)) => {
                    panic!("Request error: {}", e);
                }
                Err(e) => {
                    panic!("Task panicked: {}", e);
                }
            }
        }

        assert_eq!(
            success_count, num_requests,
            "All {} requests should succeed under high load",
            num_requests
        );

        // Verify no duplicate request numbers (would indicate corruption)
        let request_nums: Vec<&str> = response_texts
            .iter()
            .filter_map(|text| text.split('-').nth(1))
            .collect();
        let unique_nums: std::collections::HashSet<_> = request_nums.iter().collect();
        assert_eq!(
            request_nums.len(),
            unique_nums.len(),
            "No duplicate request numbers should exist"
        );

        // Verify request counter matches
        let ctx = context.read().await;
        assert_eq!(
            ctx.request_counter.load(Ordering::SeqCst),
            num_requests as u32,
            "Total processed requests should match sent requests"
        );

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }

    /// Test that requests are queued, not rejected, when server is busy
    /// Verifies that no requests are dropped due to concurrent load
    #[tokio::test]
    async fn test_requests_queued_not_rejected() {
        // Use longer delay to ensure requests overlap
        let context = Arc::new(RwLock::new(DelayedMockContext::new(50)));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let num_requests = 3;

        // Send all requests nearly simultaneously
        let start_time = std::time::Instant::now();
        let mut handles = Vec::new();
        for i in 0..num_requests {
            let client = client.clone();
            let url = transcribe_url.clone();
            let audio_data = format!("audio-{}", i).into_bytes();

            handles.push(tokio::spawn(async move {
                let req_start = std::time::Instant::now();
                let result = client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data)
                    .timeout(Duration::from_secs(30))
                    .send()
                    .await;
                let req_duration = req_start.elapsed();
                (result, req_duration)
            }));
        }

        // Collect results
        let mut durations = Vec::new();
        let mut all_success = true;
        for handle in handles {
            let (result, duration) = handle.await.expect("Task panicked");
            durations.push(duration);
            match result {
                Ok(response) if response.status().is_success() => {}
                Ok(response) => {
                    eprintln!("Request failed with status: {}", response.status());
                    all_success = false;
                }
                Err(e) => {
                    eprintln!("Request error: {}", e);
                    all_success = false;
                }
            }
        }

        let total_time = start_time.elapsed();

        assert!(all_success, "All requests should succeed (none rejected)");

        // With 50ms delay per request and 3 concurrent requests,
        // total time should be >= 150ms if properly queued
        assert!(
            total_time >= Duration::from_millis(100),
            "Requests should be queued (total time: {:?})",
            total_time
        );

        // Verify all requests were processed
        let ctx = context.read().await;
        assert_eq!(
            ctx.request_counter.load(Ordering::SeqCst),
            num_requests as u32,
            "All requests should be processed (none dropped)"
        );

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }

    struct PasswordedControlContext {
        password: String,
        model_name: String,
        allow_model_control: bool,
    }

    impl ServerContext for PasswordedControlContext {
        fn get_model_name(&self) -> String {
            self.model_name.clone()
        }
        fn get_server_name(&self) -> String {
            "passworded-server".to_string()
        }
        fn get_password(&self) -> Option<String> {
            Some(self.password.clone())
        }
        fn allow_model_control(&self) -> bool {
            self.allow_model_control
        }
        fn transcribe(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                self.model_name.clone(),
                None,
                false,
            );
            Ok(crate::transcription::TranscriptionResult::new(&job, "ok"))
        }
        fn get_model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
            Ok(mock_model_control_snapshot(&self.model_name))
        }
        fn update_shared_model(
            &self,
            model_id: &str,
            engine: &str,
        ) -> Result<RemoteModelControlSnapshot, String> {
            if engine != "whisper" && engine != "parakeet" {
                return Err(format!("Unsupported sharing engine '{engine}'"));
            }
            if model_id == "missing" {
                return Err(format!("Model '{model_id}' not found or not downloaded"));
            }
            Ok(mock_model_control_snapshot(model_id))
        }
    }

    #[tokio::test]
    async fn test_control_models_requires_password_when_unconfigured() {
        let ctx = Arc::new(RwLock::new(MockContext));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("GET")
            .path("/api/v1/control/models")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 403);
        let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.error, "control_requires_password");
    }

    #[tokio::test]
    async fn test_control_models_rejects_missing_password() {
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: true,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("GET")
            .path("/api/v1/control/models")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn test_control_models_patch_rejects_missing_password_before_body() {
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: true,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("PATCH")
            .path("/api/v1/control/models")
            .header("content-type", "application/json")
            .body(vec![b'a'; MAX_CONTROL_BODY_BYTES as usize + 1])
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 401);
        let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.error, "unauthorized");
    }

    #[tokio::test]
    async fn test_control_models_get_returns_snapshot_with_auth() {
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: true,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("GET")
            .path("/api/v1/control/models")
            .header(AUTH_HEADER, "secret")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);
        let body: RemoteModelControlSnapshot = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.current.id, "base.en");
        assert!(!body.available.is_empty());
    }

    #[tokio::test]
    async fn test_control_models_patch_validates_unknown_model() {
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: true,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("PATCH")
            .path("/api/v1/control/models")
            .header(AUTH_HEADER, "secret")
            .header("content-type", "application/json")
            .body(r#"{"model":"missing","engine":"whisper"}"#)
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn test_control_models_patch_updates_model_with_auth() {
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: true,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("PATCH")
            .path("/api/v1/control/models")
            .header(AUTH_HEADER, "secret")
            .header("content-type", "application/json")
            .body(r#"{"model":"large-v3-turbo","engine":"whisper"}"#)
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);
        let body: RemoteModelControlSnapshot = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.current.id, "large-v3-turbo");
    }
    #[tokio::test]
    async fn test_control_models_rejects_when_host_opt_in_disabled() {
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: false,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("GET")
            .path("/api/v1/control/models")
            .header(AUTH_HEADER, "secret")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 403);
        let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.error, "model_control_disabled");
    }

    #[tokio::test]
    async fn test_control_models_patch_rejects_when_host_opt_in_disabled() {
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: false,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("PATCH")
            .path("/api/v1/control/models")
            .header(AUTH_HEADER, "secret")
            .header("content-type", "application/json")
            .body(r#"{"model":"large-v3-turbo","engine":"whisper"}"#)
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 403);
        let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.error, "model_control_disabled");
    }
}
