//! HTTP integration tests for remote transcription
//!
//! These tests verify the actual HTTP server and client communication
//! without requiring whisper models (uses mock transcription).

use crate::remote::http::{create_routes, ServerContext};
use crate::remote::server::{RemoteServerConfig, StatusResponse, TranscribeResponse};
use std::sync::Arc;
use tokio::sync::Mutex;
use warp::Filter;

/// Test context for mock transcription
struct TestContext {
    model_name: String,
    server_name: String,
    password: Option<String>,
    /// Mock transcription result
    mock_result: String,
}

impl ServerContext for TestContext {
    fn get_model_name(&self) -> String {
        self.model_name.clone()
    }

    fn get_server_name(&self) -> String {
        self.server_name.clone()
    }

    fn get_password(&self) -> Option<String> {
        self.password.clone()
    }

    fn transcribe(&self, _audio_data: &[u8]) -> Result<TranscribeResponse, String> {
        Ok(TranscribeResponse {
            text: self.mock_result.clone(),
            duration_ms: 1000,
            model: self.model_name.clone(),
        })
    }
}

fn create_test_context() -> Arc<Mutex<TestContext>> {
    Arc::new(Mutex::new(TestContext {
        model_name: "test-model".to_string(),
        server_name: "test-server".to_string(),
        password: None,
        mock_result: "Hello, this is a test transcription.".to_string(),
    }))
}

fn create_test_context_with_password(password: &str) -> Arc<Mutex<TestContext>> {
    Arc::new(Mutex::new(TestContext {
        model_name: "test-model".to_string(),
        server_name: "test-server".to_string(),
        password: Some(password.to_string()),
        mock_result: "Hello, this is a test transcription.".to_string(),
    }))
}

/// Test status endpoint returns correct JSON
#[tokio::test]
async fn test_status_endpoint_returns_ok() {
    let ctx = create_test_context();
    let routes = create_routes(ctx);

    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);

    let body: StatusResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body.status, "ok");
    assert_eq!(body.model, "test-model");
    assert_eq!(body.name, "test-server");
}

/// Test status endpoint with password protection (no password provided)
#[tokio::test]
async fn test_status_endpoint_requires_password() {
    let ctx = create_test_context_with_password("secret123");
    let routes = create_routes(ctx);

    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 401);
}

/// Test status endpoint with correct password
#[tokio::test]
async fn test_status_endpoint_accepts_correct_password() {
    let ctx = create_test_context_with_password("secret123");
    let routes = create_routes(ctx);

    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .header("X-VoiceTypr-Key", "secret123")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);
}

/// Test status endpoint with wrong password
#[tokio::test]
async fn test_status_endpoint_rejects_wrong_password() {
    let ctx = create_test_context_with_password("secret123");
    let routes = create_routes(ctx);

    let response = warp::test::request()
        .method("GET")
        .path("/api/v1/status")
        .header("X-VoiceTypr-Key", "wrongpassword")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 401);
}

/// Test transcribe endpoint accepts audio and returns transcription
#[tokio::test]
async fn test_transcribe_endpoint_returns_transcription() {
    let ctx = create_test_context();
    let routes = create_routes(ctx);

    // Create fake audio data (WAV header + some samples)
    let audio_data = create_test_wav_data();

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/wav")
        .body(audio_data)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);

    let body: TranscribeResponse = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body.text, "Hello, this is a test transcription.");
    assert_eq!(body.model, "test-model");
}

/// Test transcribe endpoint requires authentication when password set
#[tokio::test]
async fn test_transcribe_endpoint_requires_password() {
    let ctx = create_test_context_with_password("secret123");
    let routes = create_routes(ctx);

    let audio_data = create_test_wav_data();

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/wav")
        .body(audio_data)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 401);
}

/// Test transcribe endpoint with correct password
#[tokio::test]
async fn test_transcribe_endpoint_accepts_correct_password() {
    let ctx = create_test_context_with_password("secret123");
    let routes = create_routes(ctx);

    let audio_data = create_test_wav_data();

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/wav")
        .header("X-VoiceTypr-Key", "secret123")
        .body(audio_data)
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 200);
}

/// Test transcribe endpoint rejects non-audio content type
#[tokio::test]
async fn test_transcribe_endpoint_rejects_wrong_content_type() {
    let ctx = create_test_context();
    let routes = create_routes(ctx);

    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "application/json")
        .body("{\"foo\": \"bar\"}")
        .reply(&routes)
        .await;

    assert_eq!(response.status(), 415); // Unsupported Media Type
}

/// Helper to create minimal WAV data for testing
fn create_test_wav_data() -> Vec<u8> {
    // Minimal WAV header (44 bytes) + some samples
    let sample_rate = 16000u32;
    let num_channels = 1u16;
    let bits_per_sample = 16u16;
    let num_samples = 1600u32; // 100ms at 16kHz

    let data_size = num_samples * (bits_per_sample as u32 / 8);
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity(44 + data_size as usize);

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    wav.extend_from_slice(&num_channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * num_channels as u32 * bits_per_sample as u32 / 8).to_le_bytes()); // byte rate
    wav.extend_from_slice(&(num_channels * bits_per_sample / 8).to_le_bytes()); // block align
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    // Samples (silence)
    wav.extend(std::iter::repeat(0u8).take(data_size as usize));

    wav
}
