//! HTTP server implementation for remote transcription
//!
//! Uses warp to create REST API endpoints for status and transcription.

use log::{info, warn};
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
    let server_name = ctx.get_server_name();

    info!("[Remote Server] Status request received on '{}'", server_name);

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
    let machine_id = crate::license::device::get_device_hash()
        .unwrap_or_else(|_| "unknown".to_string());

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
    body: bytes::Bytes,
    ctx: Arc<Mutex<T>>,
) -> Result<impl Reply, Rejection> {
    let ctx = ctx.lock().await;
    let server_name = ctx.get_server_name();
    let model_name = ctx.get_model_name();
    let audio_size_kb = body.len() as f64 / 1024.0;

    info!(
        "🎙️ [Remote Server] Transcription request received on '{}': {:.1} KB audio, content-type='{}'",
        server_name, audio_size_kb, content_type
    );

    // Check authentication
    if let Some(required_password) = ctx.get_password() {
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

    // Perform transcription
    match ctx.transcribe(&body) {
        Ok(response) => {
            info!(
                "🎯 [Remote Server] Transcription COMPLETED on '{}': {} chars in {}ms using '{}'",
                server_name,
                response.text.len(),
                response.duration_ms,
                response.model
            );
            // Log the actual transcription text (truncated for long texts)
            let preview = if response.text.len() > 100 {
                format!("{}...", &response.text[..100])
            } else {
                response.text.clone()
            };
            info!("📝 [Remote Server] Transcription result: \"{}\"", preview);
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
        fn transcribe(&self, _audio_data: &[u8]) -> Result<TranscribeResponse, String> {
            Ok(TranscribeResponse {
                text: "mock transcription".to_string(),
                duration_ms: 100,
                model: "mock-model".to_string(),
            })
        }
    }

    /// Mock context with configurable delay to simulate transcription time
    struct DelayedMockContext {
        delay_ms: u64,
        request_counter: AtomicU32,
    }

    impl DelayedMockContext {
        fn new(delay_ms: u64) -> Self {
            Self {
                delay_ms,
                request_counter: AtomicU32::new(0),
            }
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
        fn transcribe(&self, audio_data: &[u8]) -> Result<TranscribeResponse, String> {
            // Increment request counter
            let request_num = self.request_counter.fetch_add(1, Ordering::SeqCst) + 1;

            // Simulate transcription delay (blocking, as real transcription would be)
            std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));

            // Return response with request number for verification
            Ok(TranscribeResponse {
                text: format!("transcription-{}-len-{}", request_num, audio_data.len()),
                duration_ms: self.delay_ms,
                model: "delayed-mock-model".to_string(),
            })
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
        fn transcribe(&self, _audio_data: &[u8]) -> Result<TranscribeResponse, String> {
            let request_num = self.request_counter.fetch_add(1, Ordering::SeqCst) + 1;

            if self.fail_on_requests.contains(&request_num) {
                return Err(format!("Simulated failure on request {}", request_num));
            }

            Ok(TranscribeResponse {
                text: format!("success-{}", request_num),
                duration_ms: 10,
                model: "failing-mock-model".to_string(),
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

    // ============================================================================
    // Rapid Sequential Requests Tests (Issue #2)
    // ============================================================================

    /// Test that multiple rapid requests all complete successfully
    /// Verifies request queuing behavior under load
    #[tokio::test]
    async fn test_rapid_sequential_requests_all_complete() {
        // Create context with small delay to simulate work
        let context = Arc::new(Mutex::new(DelayedMockContext::new(10)));

        // Start server
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(server_context);

            let (addr, server) = warp::serve(routes)
                .bind_with_graceful_shutdown(addr, async {
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
        let ctx = context.lock().await;
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
        let context = Arc::new(Mutex::new(DelayedMockContext::new(5)));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(server_context);

            let (addr, server) = warp::serve(routes)
                .bind_with_graceful_shutdown(addr, async {
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
        let context = Arc::new(Mutex::new(FailingMockContext::new(vec![2, 4])));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(server_context);

            let (addr, server) = warp::serve(routes)
                .bind_with_graceful_shutdown(addr, async {
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
        let context = Arc::new(Mutex::new(DelayedMockContext::new(2)));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(server_context);

            let (addr, server) = warp::serve(routes)
                .bind_with_graceful_shutdown(addr, async {
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
        let ctx = context.lock().await;
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
        let context = Arc::new(Mutex::new(DelayedMockContext::new(50)));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(server_context);

            let (addr, server) = warp::serve(routes)
                .bind_with_graceful_shutdown(addr, async {
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
        let ctx = context.lock().await;
        assert_eq!(
            ctx.request_counter.load(Ordering::SeqCst),
            num_requests as u32,
            "All requests should be processed (none dropped)"
        );

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }
}
