//! HTTP server implementation for remote transcription
//!
//! Uses warp to create REST API endpoints for status and transcription.

use log::{info, warn};
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use warp::{http::StatusCode, Filter, Rejection, Reply};

use super::server::{
    ErrorResponse, RemoteModelControlSnapshot, RemoteModelControlUpdate, StatusResponse,
    TranscribeResponse,
};
use crate::transcription::TranscriptionResult;

/// Auth header name
const AUTH_HEADER: &str = "X-VoiceTypr-Key";

const MAX_AUDIO_BODY_BYTES: u64 = 50 * 1024 * 1024; // 50 MB
const MAX_CONTROL_BODY_BYTES: u64 = 4 * 1024;

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
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let transcription_guard = Arc::new(Semaphore::new(1));
    let status_route = status_endpoint(ctx.clone());
    let transcribe_route = transcribe_endpoint(ctx.clone(), transcription_guard);
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
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    warp::path!("api" / "v1" / "transcribe")
        .and(warp::post())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(warp::header::<String>("content-type"))
        .and(warp::header::optional::<String>(
            "X-VoiceTypr-Speech-Language",
        ))
        .and(warp::header::optional::<String>(
            "X-VoiceTypr-Transcription-Task",
        ))
        .and(warp::body::content_length_limit(MAX_AUDIO_BODY_BYTES))
        .and(warp::body::bytes())
        .and(with_context(ctx))
        .and(with_transcription_guard(transcription_guard))
        .and_then(handle_transcribe)
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

/// Helper to inject transcription concurrency guard into the transcribe handler
fn with_transcription_guard(
    guard: Arc<Semaphore>,
) -> impl Filter<Extract = (Arc<Semaphore>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || guard.clone())
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
        Some(provided) if provided == *required => Ok(()),
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

    // Check authentication
    if let Some(required_password) = ctx.get_password() {
        match auth_key {
            Some(provided) if provided == required_password => {
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

    let response = StatusResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        model: ctx.get_model_name(),
        name: ctx.get_server_name(),
        machine_id,
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

/// Handle POST /api/v1/transcribe
async fn handle_transcribe<T: ServerContext + 'static>(
    auth_key: Option<String>,
    content_type: String,
    spoken_language: Option<String>,
    transcription_task: Option<String>,
    body: bytes::Bytes,
    ctx: Arc<RwLock<T>>,
    transcription_guard: Arc<Semaphore>,
) -> Result<impl Reply, Rejection> {
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
            Some(provided) if provided == required_password => {
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

    info!(
        "[Remote Server] Starting transcription with model '{}' for {:.1} KB audio",
        model_name, audio_size_kb
    );

    // Serialize transcription work on the sharing host.
    let _permit = transcription_guard
        .acquire()
        .await
        .expect("transcription semaphore closed");

    // Perform transcription
    let ctx = ctx.read().await;
    match ctx.transcribe(
        &body,
        spoken_language.as_deref(),
        transcription_task.as_deref(),
    ) {
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
                warp::reply::json(&ErrorResponse { error }),
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

    use std::sync::atomic::{AtomicU32, Ordering};
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

    // ============================================================================
    // Regression Tests for P1/P2 fixes
    // ============================================================================

    #[tokio::test]
    async fn test_transcribe_endpoint_rejects_oversized_body() {
        let ctx = Arc::new(RwLock::new(MockContext));
        let routes = create_routes(ctx);

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
        let routes = create_routes(ctx);

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .body(b"audio".to_vec())
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
            let routes = create_routes(server_context);

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
                    .body(b"audio".to_vec())
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

    #[tokio::test]
    async fn test_concurrent_transcribe_requests_serialize() {
        let context = Arc::new(RwLock::new(DelayedMockContext::new(100)));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(server_context);

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
            let routes = create_routes(server_context);

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
            let routes = create_routes(server_context);

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
            let routes = create_routes(server_context);

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
            let routes = create_routes(server_context);

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
            let routes = create_routes(server_context);

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
        let routes = create_routes(ctx);

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
        let routes = create_routes(ctx);

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
        let routes = create_routes(ctx);

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
        let routes = create_routes(ctx);

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
        let routes = create_routes(ctx);

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
        let routes = create_routes(ctx);

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
        let routes = create_routes(ctx);

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
        let routes = create_routes(ctx);

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
