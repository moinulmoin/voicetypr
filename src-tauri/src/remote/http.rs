//! HTTP server implementation for remote transcription
//!
//! Uses warp to create REST API endpoints for status and transcription.

use std::sync::Arc;
use tokio::sync::Mutex;
use warp::{http::StatusCode, Filter, Rejection, Reply};

use super::server::{ErrorResponse, StatusResponse, TranscribeResponse};

/// Auth header name
const AUTH_HEADER: &str = "X-VoiceTypr-Key";

/// Trait for server context (allows mocking in tests)
pub trait ServerContext: Send + Sync {
    fn get_model_name(&self) -> String;
    fn get_server_name(&self) -> String;
    fn get_password(&self) -> Option<String>;
    fn transcribe(&self, audio_data: &[u8]) -> Result<TranscribeResponse, String>;
}

/// Create all warp routes for the remote transcription API
pub fn create_routes<T: ServerContext + 'static>(
    ctx: Arc<Mutex<T>>,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let status_route = status_endpoint(ctx.clone());
    let transcribe_route = transcribe_endpoint(ctx);

    status_route.or(transcribe_route)
}

/// GET /api/v1/status - Returns server status and model info
fn status_endpoint<T: ServerContext + 'static>(
    ctx: Arc<Mutex<T>>,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    warp::path!("api" / "v1" / "status")
        .and(warp::get())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(with_context(ctx))
        .and_then(handle_status)
}

/// POST /api/v1/transcribe - Accepts audio and returns transcription
fn transcribe_endpoint<T: ServerContext + 'static>(
    ctx: Arc<Mutex<T>>,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    warp::path!("api" / "v1" / "transcribe")
        .and(warp::post())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(warp::header::<String>("content-type"))
        .and(warp::body::bytes())
        .and(with_context(ctx))
        .and_then(handle_transcribe)
}

/// Helper to inject context into handlers
fn with_context<T: ServerContext + 'static>(
    ctx: Arc<Mutex<T>>,
) -> impl Filter<Extract = (Arc<Mutex<T>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || ctx.clone())
}

/// Handle GET /api/v1/status
async fn handle_status<T: ServerContext + 'static>(
    auth_key: Option<String>,
    ctx: Arc<Mutex<T>>,
) -> Result<impl Reply, Rejection> {
    let ctx = ctx.lock().await;

    // Check authentication
    if let Some(required_password) = ctx.get_password() {
        match auth_key {
            Some(provided) if provided == required_password => {}
            _ => {
                return Ok(warp::reply::with_status(
                    warp::reply::json(&ErrorResponse {
                        error: "unauthorized".to_string(),
                    }),
                    StatusCode::UNAUTHORIZED,
                ));
            }
        }
    }

    let response = StatusResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        model: ctx.get_model_name(),
        name: ctx.get_server_name(),
    };

    Ok(warp::reply::with_status(
        warp::reply::json(&response),
        StatusCode::OK,
    ))
}

/// Handle POST /api/v1/transcribe
async fn handle_transcribe<T: ServerContext + 'static>(
    auth_key: Option<String>,
    content_type: String,
    body: bytes::Bytes,
    ctx: Arc<Mutex<T>>,
) -> Result<impl Reply, Rejection> {
    let ctx = ctx.lock().await;

    // Check authentication
    if let Some(required_password) = ctx.get_password() {
        match auth_key {
            Some(provided) if provided == required_password => {}
            _ => {
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
        return Ok(warp::reply::with_status(
            warp::reply::json(&ErrorResponse {
                error: "unsupported_media_type".to_string(),
            }),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ));
    }

    // Perform transcription
    match ctx.transcribe(&body) {
        Ok(response) => Ok(warp::reply::with_status(
            warp::reply::json(&response),
            StatusCode::OK,
        )),
        Err(error) => Ok(warp::reply::with_status(
            warp::reply::json(&ErrorResponse { error }),
            StatusCode::INTERNAL_SERVER_ERROR,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        fn transcribe(&self, _audio_data: &[u8]) -> Result<TranscribeResponse, String> {
            Ok(TranscribeResponse {
                text: "mock transcription".to_string(),
                duration_ms: 100,
                model: "mock-model".to_string(),
            })
        }
    }

    #[test]
    fn test_mock_context() {
        let ctx = MockContext;
        assert_eq!(ctx.get_model_name(), "mock-model");
        assert_eq!(ctx.get_server_name(), "mock-server");
        assert!(ctx.get_password().is_none());
    }
}
