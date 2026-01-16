//! Level 3 Integration Tests for Remote Transcription
//!
//! These tests require external resources (Whisper models) and are ignored by default.
//! They work on both macOS and Windows.
//!
//! To run these tests manually:
//!
//! 1. Ensure the tiny.en model is downloaded via VoiceTypr app
//! 2. Run the tests:
//!
//! **macOS:**
//! ```bash
//! cargo test --package voicetypr_app integration_tests -- --ignored --nocapture
//! ```
//!
//! **Windows** (requires manifest embedding):
//! ```powershell
//! cd src-tauri
//! ./run-tests.ps1 -IgnoredOnly
//! ```

#[cfg(test)]
mod tests {
    use crate::remote::http::create_routes;
    use crate::remote::transcription::{RealTranscriptionContext, TranscriptionServerConfig};
    use futures_util::future::join_all;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
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
    /// Works on both macOS and Windows by checking the appropriate data directories
    fn get_tiny_model_path() -> Option<PathBuf> {
        // Check common model locations in order of priority
        let mut possible_paths: Vec<PathBuf> = Vec::new();

        // Primary: User data directory (where VoiceTypr downloads models)
        // - macOS: ~/Library/Application Support/com.voicetypr.app/models/
        // - Windows: C:\Users\<user>\AppData\Roaming\com.voicetypr.app\models\
        if let Some(data_dir) = dirs::data_dir() {
            possible_paths.push(
                data_dir
                    .join("com.voicetypr.app")
                    .join("models")
                    .join("ggml-tiny.en.bin"),
            );
        }

        // Fallback: data_local_dir (Windows AppData\Local, same as data_dir on macOS)
        if let Some(local_dir) = dirs::data_local_dir() {
            possible_paths.push(
                local_dir
                    .join("com.voicetypr.app")
                    .join("models")
                    .join("ggml-tiny.en.bin"),
            );
        }

        // Development directory (relative to src-tauri/)
        possible_paths.push(PathBuf::from("models/ggml-tiny.en.bin"));

        for path in possible_paths {
            if path.exists() {
                return Some(path);
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

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
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

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
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
        let response = client
            .get(&status_url)
            .send()
            .await
            .expect("Request failed");
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

    /// Level 3 Integration Test: Rapid sequential requests (Issue #2)
    ///
    /// Tests that multiple rapid requests from a single client all complete
    /// successfully with real transcription.
    ///
    /// This verifies:
    /// 1. All requests complete (queued, not rejected)
    /// 2. No crashes or data corruption
    /// 3. Server handles concurrent load
    #[tokio::test]
    #[ignore]
    async fn test_rapid_sequential_requests_real_transcription() {
        let model_path = match get_tiny_model_path() {
            Some(p) => p,
            None => {
                eprintln!("SKIPPED: tiny.en model not found");
                return;
            }
        };

        println!("Using model: {:?}", model_path);

        // Create multiple test audio files
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let num_requests = 3;
        let mut audio_files = Vec::new();
        for i in 0..num_requests {
            let audio_path = temp_dir.path().join(format!("test_{}.wav", i));
            create_test_wav(&audio_path).expect("Failed to create test WAV");
            audio_files.push(audio_path);
        }
        println!("Created {} test audio files", num_requests);

        // Create server
        let config = TranscriptionServerConfig {
            server_name: "Rapid Test Server".to_string(),
            password: None,
            model_name: "tiny.en".to_string(),
            model_path,
        };

        let context = Arc::new(Mutex::new(RealTranscriptionContext::new(config)));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(context);

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        println!("Server started at: http://{}", addr);
        sleep(Duration::from_millis(100)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);

        // Send rapid sequential requests concurrently
        println!("Sending {} rapid concurrent requests...", num_requests);
        let start_time = Instant::now();

        let mut handles = Vec::new();
        for (i, audio_path) in audio_files.iter().enumerate() {
            let client = client.clone();
            let url = transcribe_url.clone();
            let audio_data = std::fs::read(audio_path).expect("Failed to read audio");
            let request_id = i;

            handles.push(tokio::spawn(async move {
                let req_start = Instant::now();
                println!("Request {} starting", request_id);

                let result = client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data)
                    .timeout(Duration::from_secs(120)) // 2 min timeout for model loading
                    .send()
                    .await;

                let req_duration = req_start.elapsed();
                println!("Request {} completed in {:?}", request_id, req_duration);

                (request_id, result, req_duration)
            }));
        }

        // Wait for all requests
        let mut success_count = 0;
        let mut total_chars = 0;
        for handle in handles {
            let (request_id, result, duration) = handle.await.expect("Task panicked");
            match result {
                Ok(response) if response.status().is_success() => {
                    let json: serde_json::Value =
                        response.json().await.expect("Failed to parse JSON");
                    let text = json["text"].as_str().unwrap_or("");
                    total_chars += text.len();
                    println!(
                        "Request {}: SUCCESS ({} chars, {:?})",
                        request_id,
                        text.len(),
                        duration
                    );
                    success_count += 1;
                }
                Ok(response) => {
                    println!(
                        "Request {}: FAILED with status {}",
                        request_id,
                        response.status()
                    );
                }
                Err(e) => {
                    println!("Request {}: ERROR - {}", request_id, e);
                }
            }
        }

        let total_time = start_time.elapsed();

        println!("\n--- Results ---");
        println!("Total requests: {}", num_requests);
        println!("Successful: {}", success_count);
        println!("Total time: {:?}", total_time);
        println!("Total chars transcribed: {}", total_chars);

        // Assertions
        assert_eq!(
            success_count, num_requests,
            "All {} requests should complete successfully",
            num_requests
        );

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;

        println!("✓ Rapid sequential requests test passed!");
    }

    /// Level 3 Integration Test: Sequential requests with varying audio sizes
    ///
    /// Tests that requests with different audio sizes all process correctly
    #[tokio::test]
    #[ignore]
    async fn test_sequential_requests_varying_sizes() {
        let model_path = match get_tiny_model_path() {
            Some(p) => p,
            None => {
                eprintln!("SKIPPED: tiny.en model not found");
                return;
            }
        };

        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create audio files of varying sizes (0.5s, 1s, 1.5s of audio)
        let sample_counts = [8000, 16000, 24000]; // samples at 16kHz
        let mut audio_files = Vec::new();

        for (i, &samples) in sample_counts.iter().enumerate() {
            let audio_path = temp_dir.path().join(format!("test_size_{}.wav", i));
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 16000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer =
                hound::WavWriter::create(&audio_path, spec).expect("Failed to create WAV writer");
            for _ in 0..samples {
                writer.write_sample(0i16).expect("Failed to write sample");
            }
            writer.finalize().expect("Failed to finalize WAV");
            audio_files.push((audio_path, samples));
        }

        let config = TranscriptionServerConfig {
            server_name: "Size Test Server".to_string(),
            password: None,
            model_name: "tiny.en".to_string(),
            model_path,
        };

        let context = Arc::new(Mutex::new(RealTranscriptionContext::new(config)));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(context);
            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });
            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(100)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);

        // Send requests sequentially
        for (audio_path, samples) in &audio_files {
            let audio_data = std::fs::read(audio_path).expect("Failed to read audio");
            let response = client
                .post(&transcribe_url)
                .header("Content-Type", "audio/wav")
                .body(audio_data)
                .timeout(Duration::from_secs(120))
                .send()
                .await
                .expect("Request failed");

            assert!(
                response.status().is_success(),
                "Request for {} samples should succeed",
                samples
            );

            let json: serde_json::Value = response.json().await.expect("Failed to parse JSON");
            assert!(json["text"].is_string(), "Response should have text field");
            assert!(
                json["model"].is_string(),
                "Response should have model field"
            );
            println!(
                "Processed {} samples: {:?}",
                samples,
                json["text"].as_str().unwrap_or("")
            );
        }

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;

        println!("✓ Sequential requests with varying sizes test passed!");
    }

    /// Level 3 Integration Test: Host local + remote client concurrent transcription (Issue #3)
    ///
    /// This test simulates the scenario from Issue #3:
    /// 1. PC starts sharing (server mode)
    /// 2. MacBook connects as client
    /// 3. Both machines initiate transcription simultaneously
    /// 4. Verify both complete successfully without crashes
    ///
    /// In this test, we simulate "local" and "remote" by:
    /// - Local: Direct call to the transcription context (as if host is transcribing)
    /// - Remote: HTTP request to the server (as if client is requesting)
    ///
    /// The key verification is that both complete without error when running concurrently.
    #[tokio::test]
    #[ignore]
    async fn test_concurrent_local_and_remote_transcription() {
        let model_path = match get_tiny_model_path() {
            Some(p) => p,
            None => {
                eprintln!("SKIPPED: tiny.en model not found");
                return;
            }
        };

        println!("Using model: {:?}", model_path);

        // Create test audio files
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let audio_path_1 = temp_dir.path().join("test1.wav");
        let audio_path_2 = temp_dir.path().join("test2.wav");
        create_test_wav(&audio_path_1).expect("Failed to create test WAV 1");
        create_test_wav(&audio_path_2).expect("Failed to create test WAV 2");
        println!("Created test audio files");

        // Create server config
        let config = TranscriptionServerConfig {
            server_name: "Concurrent Test Server".to_string(),
            password: None,
            model_name: "tiny.en".to_string(),
            model_path,
        };

        // Create shared transcription context
        // This simulates the host having one context used for both local and remote transcriptions
        let context = Arc::new(Mutex::new(RealTranscriptionContext::new(config)));
        let context_for_local = context.clone();

        // Start HTTP server for remote requests
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(context);

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        println!("Server started at: http://{}", addr);
        sleep(Duration::from_millis(100)).await;

        // Prepare audio data for both requests
        let audio_data_remote = std::fs::read(&audio_path_1).expect("Failed to read audio file 1");
        let audio_data_local = std::fs::read(&audio_path_2).expect("Failed to read audio file 2");

        // Spawn "remote client" request (HTTP)
        let addr_clone = addr;
        let remote_handle = tokio::spawn(async move {
            println!("[Remote] Starting HTTP transcription request...");
            let client = reqwest::Client::new();
            let transcribe_url = format!("http://{}/api/v1/transcribe", addr_clone);
            let response = client
                .post(&transcribe_url)
                .header("Content-Type", "audio/wav")
                .body(audio_data_remote)
                .timeout(Duration::from_secs(120))
                .send()
                .await
                .expect("[Remote] Failed to send request");

            let status = response.status();
            println!("[Remote] Response status: {}", status);
            assert!(
                status.is_success(),
                "[Remote] Transcription failed with status: {}",
                status
            );

            let json: serde_json::Value = response
                .json()
                .await
                .expect("[Remote] Failed to parse JSON");
            println!("[Remote] Transcription completed: {:?}", json);
            json
        });

        // Spawn "local host" transcription (direct context access)
        let local_handle = tokio::spawn(async move {
            println!("[Local] Starting direct transcription...");

            // Access the shared context directly (simulating local transcription on host)
            let ctx = context_for_local.lock().await;
            let result = ctx.transcribe(&audio_data_local);
            drop(ctx); // Release lock

            match result {
                Ok(response) => {
                    println!(
                        "[Local] Transcription completed: text='{}', duration={}ms, model='{}'",
                        response.text, response.duration_ms, response.model
                    );
                    Ok(response)
                }
                Err(e) => {
                    println!("[Local] Transcription failed: {}", e);
                    Err(e)
                }
            }
        });

        // Wait for both to complete
        println!("Waiting for both transcriptions to complete...");
        let (remote_result, local_result) = tokio::join!(remote_handle, local_handle);

        // Verify remote request succeeded
        let remote_json = remote_result.expect("[Remote] Task panicked");
        assert!(
            remote_json["text"].is_string(),
            "[Remote] Missing 'text' in response"
        );
        assert!(
            remote_json["model"].is_string(),
            "[Remote] Missing 'model' in response"
        );

        // Verify local transcription succeeded
        let local_response = local_result.expect("[Local] Task panicked");
        assert!(
            local_response.is_ok(),
            "[Local] Transcription failed: {:?}",
            local_response.err()
        );
        let local_response = local_response.unwrap();
        assert!(!local_response.model.is_empty(), "[Local] Empty model name");

        // Shutdown server
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;

        println!("✓ Concurrent local + remote transcription test passed!");
        println!("  Both transcriptions completed without crashes or errors.");
    }

    /// Level 3 Integration Test: Multiple concurrent remote requests
    ///
    /// This test simulates multiple clients sending requests simultaneously,
    /// verifying the server can queue and handle them correctly.
    #[tokio::test]
    #[ignore]
    async fn test_multiple_concurrent_remote_requests() {
        let model_path = match get_tiny_model_path() {
            Some(p) => p,
            None => {
                eprintln!("SKIPPED: tiny.en model not found");
                return;
            }
        };

        println!("Using model: {:?}", model_path);

        // Create test audio
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let audio_path = temp_dir.path().join("test.wav");
        create_test_wav(&audio_path).expect("Failed to create test WAV");
        let audio_data = std::fs::read(&audio_path).expect("Failed to read audio file");
        println!("Created test audio");

        // Create server
        let config = TranscriptionServerConfig {
            server_name: "Multi-Client Test Server".to_string(),
            password: None,
            model_name: "tiny.en".to_string(),
            model_path,
        };

        let context = Arc::new(Mutex::new(RealTranscriptionContext::new(config)));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(context);

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        println!("Server started at: http://{}", addr);
        sleep(Duration::from_millis(100)).await;

        // Number of concurrent requests to send
        const NUM_REQUESTS: usize = 3;

        // Spawn multiple concurrent HTTP requests
        let mut handles = Vec::new();
        for i in 0..NUM_REQUESTS {
            let audio_data_clone = audio_data.clone();
            let addr_clone = addr;

            handles.push(tokio::spawn(async move {
                println!("[Client {}] Starting transcription request...", i);
                let start = Instant::now();

                let client = reqwest::Client::new();
                let transcribe_url = format!("http://{}/api/v1/transcribe", addr_clone);
                let response = client
                    .post(&transcribe_url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data_clone)
                    .timeout(Duration::from_secs(180)) // Allow time for queued requests
                    .send()
                    .await
                    .expect(&format!("[Client {}] Failed to send request", i));

                let status = response.status();
                let elapsed = start.elapsed();
                println!(
                    "[Client {}] Response status: {} (took {:?})",
                    i, status, elapsed
                );

                assert!(
                    status.is_success(),
                    "[Client {}] Failed with status: {}",
                    i,
                    status
                );

                let json: serde_json::Value = response
                    .json()
                    .await
                    .expect(&format!("[Client {}] Failed to parse JSON", i));

                println!("[Client {}] Completed successfully", i);
                (i, json, elapsed)
            }));
        }

        // Wait for all requests to complete
        println!(
            "Waiting for {} concurrent requests to complete...",
            NUM_REQUESTS
        );
        let results = join_all(handles).await;

        // Verify all succeeded
        let mut total_duration = Duration::ZERO;
        for result in results {
            let (client_id, json, elapsed) = result.expect("Task panicked");
            assert!(
                json["text"].is_string(),
                "[Client {}] Missing 'text' in response",
                client_id
            );
            assert!(
                json["model"].is_string(),
                "[Client {}] Missing 'model' in response",
                client_id
            );
            total_duration += elapsed;
        }

        // Shutdown server
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;

        println!("✓ Multiple concurrent remote requests test passed!");
        println!(
            "  All {} requests completed without crashes or errors.",
            NUM_REQUESTS
        );
        println!(
            "  Average response time: {:?}",
            total_duration / NUM_REQUESTS as u32
        );
    }
}
