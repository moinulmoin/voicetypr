//! Level 3 Integration Tests for Remote Transcription
//!
//! These tests require external resources (Whisper models) and are ignored by default.
//! To run these tests manually:
//!
//! 1. Ensure the tiny.en model is downloaded
//! 2. Run: cargo test --package voicetypr_app integration_tests -- --ignored --nocapture
//!
//! On Windows, use run-tests.ps1 for proper manifest embedding:
//! ./run-tests.ps1 -IgnoredOnly

#[cfg(test)]
mod tests {
    use crate::remote::http::create_routes;
    use crate::remote::transcription::{RealTranscriptionContext, TranscriptionServerConfig};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::sync::Mutex;
    use tokio::time::sleep;

    /// Create a valid WAV file with silent audio samples
    /// Whisper will likely return empty text for silence, but we test the full pipeline
    fn create_test_wav(path: &std::path::Path) -> Result<(), String> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = hound::WavWriter::create(path, spec)
            .map_err(|e| format!("Failed to create WAV writer: {}", e))?;

        // Write 1 second of silence (16000 samples at 16kHz)
        for _ in 0..16000 {
            writer
                .write_sample(0i16)
                .map_err(|e| format!("Failed to write sample: {}", e))?;
        }

        writer
            .finalize()
            .map_err(|e| format!("Failed to finalize WAV: {}", e))?;

        Ok(())
    }

    /// Get the path to the tiny.en model if it exists
    fn get_tiny_model_path() -> Option<PathBuf> {
        // Check common model locations
        let possible_paths = [
            // User data directory (where VoiceTypr downloads models)
            dirs::data_local_dir()
                .map(|p| p.join("com.voicetypr.app").join("models").join("ggml-tiny.en.bin")),
            // Development directory
            Some(PathBuf::from("models/ggml-tiny.en.bin")),
        ];

        for maybe_path in possible_paths.into_iter().flatten() {
            if maybe_path.exists() {
                return Some(maybe_path);
            }
        }

        None
    }

    /// Level 3 Integration Test: Full transcription pipeline
    ///
    /// This test:
    /// 1. Creates a real transcription server with actual Whisper model
    /// 2. Sends audio via HTTP
    /// 3. Verifies transcription response
    ///
    /// Ignored by default - requires tiny.en model to be downloaded
    #[tokio::test]
    #[ignore]
    async fn test_full_transcription_pipeline() {
        // Check if model exists
        let model_path = match get_tiny_model_path() {
            Some(p) => p,
            None => {
                eprintln!("SKIPPED: tiny.en model not found");
                eprintln!(
                    "Download the tiny.en model via VoiceTypr or manually place it in models/"
                );
                return;
            }
        };

        println!("Using model: {:?}", model_path);

        // Create test audio
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let audio_path = temp_dir.path().join("test.wav");
        create_test_wav(&audio_path).expect("Failed to create test WAV");
        println!("Created test audio: {:?}", audio_path);

        // Create server config
        let config = TranscriptionServerConfig {
            server_name: "Test Server".to_string(),
            password: None,
            model_name: "tiny.en".to_string(),
            model_path,
        };

        // Create real transcription context wrapped in Arc<Mutex>
        let context = Arc::new(Mutex::new(RealTranscriptionContext::new(config)));

        // Start server
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(context);

            let (addr, server) = warp::serve(routes)
                .bind_with_graceful_shutdown(addr, async {
                    shutdown_rx.await.ok();
                });

            let _ = addr_tx.send(addr);
            server.await;
        });

        // Wait for server to start
        let addr = addr_rx.await.expect("Server failed to start");
        println!("Server started at: http://{}", addr);

        // Give server time to fully initialize
        sleep(Duration::from_millis(100)).await;

        // Test status endpoint
        let client = reqwest::Client::new();
        let status_url = format!("http://{}/api/v1/status", addr);
        let status_response = client
            .get(&status_url)
            .send()
            .await
            .expect("Failed to get status");

        assert!(
            status_response.status().is_success(),
            "Status endpoint failed"
        );
        let status_json: serde_json::Value = status_response
            .json()
            .await
            .expect("Failed to parse status JSON");
        println!("Status response: {:?}", status_json);
        assert_eq!(status_json["status"], "ready");

        // Test transcription endpoint
        let audio_data = std::fs::read(&audio_path).expect("Failed to read audio file");
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let transcribe_response = client
            .post(&transcribe_url)
            .header("Content-Type", "audio/wav")
            .body(audio_data)
            .timeout(Duration::from_secs(60)) // Allow 60s for model loading + transcription
            .send()
            .await
            .expect("Failed to send transcription request");

        assert!(
            transcribe_response.status().is_success(),
            "Transcription endpoint failed with status: {}",
            transcribe_response.status()
        );

        let transcribe_json: serde_json::Value = transcribe_response
            .json()
            .await
            .expect("Failed to parse transcription JSON");
        println!("Transcription response: {:?}", transcribe_json);

        // Verify response structure
        assert!(
            transcribe_json["text"].is_string(),
            "Missing 'text' in response"
        );
        assert!(
            transcribe_json["model"].is_string(),
            "Missing 'model' in response"
        );

        // Shutdown server
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;

        println!("✓ Full transcription pipeline test passed!");
    }

    /// Level 3 Integration Test: Server authentication
    ///
    /// Tests that password protection works correctly
    #[tokio::test]
    #[ignore]
    async fn test_server_authentication() {
        let model_path = match get_tiny_model_path() {
            Some(p) => p,
            None => {
                eprintln!("SKIPPED: tiny.en model not found");
                return;
            }
        };

        // Create server with password
        let config = TranscriptionServerConfig {
            server_name: "Protected Server".to_string(),
            password: Some("test-password".to_string()),
            model_name: "tiny.en".to_string(),
            model_path,
        };

        let context = Arc::new(Mutex::new(RealTranscriptionContext::new(config)));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(context);

            let (addr, server) = warp::serve(routes)
                .bind_with_graceful_shutdown(addr, async {
                    shutdown_rx.await.ok();
                });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(100)).await;

        let client = reqwest::Client::new();
        let status_url = format!("http://{}/api/v1/status", addr);

        // Request without password should fail
        let response = client.get(&status_url).send().await.expect("Request failed");
        assert_eq!(
            response.status(),
            reqwest::StatusCode::UNAUTHORIZED,
            "Expected 401 without password"
        );

        // Request with wrong password should fail
        let response = client
            .get(&status_url)
            .header("X-VoiceTypr-Key", "wrong-password")
            .send()
            .await
            .expect("Request failed");
        assert_eq!(
            response.status(),
            reqwest::StatusCode::UNAUTHORIZED,
            "Expected 401 with wrong password"
        );

        // Request with correct password should succeed
        let response = client
            .get(&status_url)
            .header("X-VoiceTypr-Key", "test-password")
            .send()
            .await
            .expect("Request failed");
        assert!(
            response.status().is_success(),
            "Expected success with correct password"
        );

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;

        println!("✓ Server authentication test passed!");
    }
}
