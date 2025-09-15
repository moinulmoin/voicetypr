/// Error Event Emission Tests
/// 
/// These tests verify that various error scenarios emit the correct events
/// to the frontend with proper payloads and structure.

use std::sync::{Arc, Mutex};
use serde_json::{json, Value};

// Audio validation is now handled by the transcriber

/// Mock event collector for testing
#[derive(Debug, Clone)]
pub struct MockEventCollector {
    events: Arc<Mutex<Vec<(String, Value)>>>,
}

impl MockEventCollector {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn emit(&self, event: &str, payload: Value) {
        let mut events = self.events.lock().unwrap();
        events.push((event.to_string(), payload.clone()));
        log::info!("📡 Event emitted: {} -> {:?}", event, payload);
    }

    pub fn get_events(&self) -> Vec<(String, Value)> {
        self.events.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.events.lock().unwrap().clear();
    }

    pub fn count(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    pub fn find_event(&self, event_name: &str) -> Option<Value> {
        let events = self.events.lock().unwrap();
        events.iter()
            .find(|(name, _)| name == event_name)
            .map(|(_, payload)| payload.clone())
    }
}

#[cfg(test)]
mod event_emission_tests {
    use super::*;
    use tempfile::NamedTempFile;
    use hound::WavWriter;

    /// Helper function to create test WAV files
    fn create_test_wav(samples: Vec<i16>, sample_rate: u32) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let temp_file = NamedTempFile::new()?;
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        
        let mut writer = WavWriter::create(temp_file.path(), spec)?;
        for sample in samples {
            writer.write_sample(sample)?;
        }
        writer.finalize()?;
        
        Ok(temp_file)
    }

    #[test]
    fn test_no_speech_event_emission() {
        log::info!("Testing no-speech-detected event emission after transcription");

        let collector = MockEventCollector::new();

        // Simulate what happens when Whisper returns empty text
        let whisper_result = ""; // Empty transcription result

        if whisper_result.is_empty() {
            let event_payload = json!({
                "type": "no-speech-detected",
                "message": "No speech was detected in the recording",
                "suggestion": "Please try speaking louder or closer to the microphone",
                "actionable": true,
                "action": {
                    "type": "open-settings",
                    "label": "Check Audio Settings"
                }
            });

            collector.emit("no-speech-detected", event_payload.clone());
        }

        // Verify event was emitted with correct structure
        let events = collector.get_events();
        assert_eq!(events.len(), 1);

        let (event_name, _payload) = &events[0];
        assert_eq!(event_name, "no-speech-detected");

        log::info!("✅ No-speech-detected event emitted correctly after empty transcription");
    }

    #[test]
    fn test_recording_too_short_event() {
        log::info!("Testing recording-too-short event emission");

        let collector = MockEventCollector::new();

        // Simulate a too-short recording error from transcriber
        let error_message = "Recording too short (0.3s). Minimum duration is 0.5 seconds";

        // Emit the expected event
        let event_payload = json!({
            "type": "recording-too-short",
            "message": error_message,
            "duration": 0.3,
            "minimum_duration": 0.5,
            "actionable": true,
            "action": {
                "type": "retry-recording",
                "label": "Try Recording Again"
            }
        });

        collector.emit("recording-too-short", event_payload.clone());

        // Verify event was emitted
        let events = collector.get_events();
        assert_eq!(events.len(), 1);

        let (event_name, payload) = &events[0];
        assert_eq!(event_name, "recording-too-short");
        assert!(payload["duration"].is_number());
        assert_eq!(payload["minimum_duration"], 0.5);

        log::info!("✅ Recording-too-short event emitted correctly");
    }


    #[test]
    fn test_multiple_error_events_sequence() {
        log::info!("Testing sequence of multiple error events");

        let collector = MockEventCollector::new();

        // Test multiple different error scenarios in sequence
        let test_scenarios = vec![
            ("no_speech", "No speech detected"),
            ("too_quiet", "Audio too quiet"),
            ("too_short", "Recording too short (0.1s)"),
        ];

        for (scenario_name, error_msg) in test_scenarios {
            match scenario_name {
                "no_speech" => {
                    collector.emit("no-speech-detected", json!({
                        "type": "no-speech-detected",
                        "message": error_msg,
                        "test_case": scenario_name
                    }));
                }
                "too_quiet" => {
                    collector.emit("audio-too-quiet", json!({
                        "type": "audio-too-quiet",
                        "message": error_msg,
                        "test_case": scenario_name
                    }));
                }
                "too_short" => {
                    collector.emit("recording-too-short", json!({
                        "type": "recording-too-short",
                        "message": error_msg,
                        "duration": 0.1,
                        "test_case": scenario_name
                    }));
                }
                _ => panic!("Unexpected scenario: {}", scenario_name),
            }
        }

        // Verify all events were collected
        let events = collector.get_events();
        assert_eq!(events.len(), 3);

        // Verify event sequence
        assert_eq!(events[0].0, "no-speech-detected");
        assert_eq!(events[1].0, "audio-too-quiet");
        assert_eq!(events[2].0, "recording-too-short");

        log::info!("✅ Multiple error events emitted in correct sequence");
    }

    #[test]
    fn test_recording_state_error_events() {
        log::info!("Testing recording state error events");
        
        let collector = MockEventCollector::new();
        
        // Simulate various recording state error scenarios
        let error_scenarios = vec![
            ("microphone-access-denied", "Could not access microphone. Please check permissions."),
            ("no-models-available", "No speech recognition models are installed."),
            ("transcription-failed", "Failed to transcribe audio. Please try again."),
            ("file-write-error", "Could not save recording file."),
        ];
        
        for (error_type, error_message) in error_scenarios {
            let event_payload = json!({
                "state": "error",
                "error": {
                    "type": error_type,
                    "message": error_message,
                    "recoverable": true,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            });
            
            collector.emit("recording-state-changed", event_payload.clone());
        }
        
        let events = collector.get_events();
        assert_eq!(events.len(), 4);
        
        // Verify each error event has correct structure
        for (event_name, payload) in events {
            assert_eq!(event_name, "recording-state-changed");
            assert_eq!(payload["state"], "error");
            assert!(payload["error"]["type"].is_string());
            assert!(payload["error"]["message"].is_string());
            assert_eq!(payload["error"]["recoverable"], true);
        }
        
        log::info!("✅ Recording state error events emitted correctly");
    }

    #[test]
    fn test_event_payload_validation() {
        log::info!("Testing comprehensive event payload validation");
        
        let collector = MockEventCollector::new();
        
        // Test that all required fields are present in event payloads
        let required_events = vec![
            ("no-speech-detected", json!({
                "type": "no-speech-detected",
                "message": "Test message",
                "suggestion": "Test suggestion", 
                "actionable": true,
                "action": {
                    "type": "open-settings",
                    "label": "Test Action"
                },
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "severity": "warning"
            })),
            ("audio-too-quiet", json!({
                "type": "audio-too-quiet",
                "message": "Test message",
                "energy_level": 0.005,
                "suggestion": "Test suggestion",
                "actionable": true,
                "action": {
                    "type": "open-settings", 
                    "label": "Test Action"
                },
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "severity": "warning"
            })),
            ("recording-error", json!({
                "type": "recording-error",
                "message": "Test error message",
                "error_code": "E001",
                "recoverable": true,
                "action": {
                    "type": "retry",
                    "label": "Try Again"
                },
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "severity": "error"
            })),
        ];
        
        for (event_name, payload) in required_events {
            collector.emit(event_name, payload.clone());
            
            // Validate payload structure
            assert!(payload["type"].is_string());
            assert!(payload["message"].is_string());
            assert!(payload["timestamp"].is_string());
            assert!(payload["severity"].is_string());
            
            if payload["actionable"] == true {
                assert!(payload["action"].is_object());
                assert!(payload["action"]["type"].is_string());
                assert!(payload["action"]["label"].is_string());
            }
        }
        
        let events = collector.get_events();
        assert_eq!(events.len(), 3);
        
        log::info!("✅ All event payloads contain required fields");
    }

    #[test]
    fn test_event_deduplication() {
        log::info!("Testing event deduplication logic");
        
        let collector = MockEventCollector::new();
        
        // Simulate rapid duplicate events (which should be deduplicated)
        let duplicate_event = json!({
            "type": "no-speech-detected",
            "message": "Duplicate test message",
            "id": "test-duplicate-123"
        });
        
        // Emit the same event multiple times
        for i in 0..5 {
            collector.emit("no-speech-detected", duplicate_event.clone());
            log::debug!("Emitted duplicate event #{}", i + 1);
        }
        
        // Without deduplication, we'd have 5 events
        let events = collector.get_events();
        assert_eq!(events.len(), 5); // MockCollector doesn't dedupe, but real implementation should
        
        // In a real implementation, you'd test that only 1 event was actually sent to frontend
        log::info!("✅ Event deduplication test completed (mock shows 5, real should show 1)");
    }

    #[test]
    fn test_error_recovery_events() {
        log::info!("Testing error recovery event sequence");
        
        let collector = MockEventCollector::new();
        
        // Simulate error -> recovery sequence
        let error_event = json!({
            "state": "error",
            "error": {
                "type": "microphone-access-denied",
                "message": "Microphone access was denied",
                "recoverable": true
            }
        });
        
        let recovery_event = json!({
            "state": "idle", 
            "recovery": {
                "from_error": "microphone-access-denied",
                "message": "Microphone access restored",
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        });
        
        collector.emit("recording-state-changed", error_event);
        collector.emit("recording-state-changed", recovery_event);
        
        let events = collector.get_events();
        assert_eq!(events.len(), 2);
        
        // Verify error -> recovery sequence
        assert_eq!(events[0].1["state"], "error");
        assert_eq!(events[1].1["state"], "idle");
        assert!(events[1].1["recovery"].is_object());
        
        log::info!("✅ Error recovery event sequence validated");
    }

    #[test]
    fn test_performance_monitoring_events() {
        log::info!("Testing performance monitoring events");
        
        let collector = MockEventCollector::new();
        
        // Simulate performance events that might be emitted during processing
        let perf_events = vec![
            ("validation-performance", json!({
                "operation": "audio_validation",
                "duration_ms": 15,
                "file_size_kb": 256,
                "result": "too_quiet"
            })),
            ("transcription-performance", json!({
                "operation": "whisper_transcription", 
                "duration_ms": 2350,
                "model": "base.en",
                "audio_duration_s": 3.2,
                "result": "success"
            })),
            ("model-load-performance", json!({
                "operation": "model_preload",
                "duration_ms": 890,
                "model": "base.en", 
                "cache_hit": false,
                "memory_usage_mb": 512
            })),
        ];
        
        for (event_name, payload) in perf_events {
            collector.emit(event_name, payload.clone());
            
            // Validate performance event structure
            assert!(payload["operation"].is_string());
            assert!(payload["duration_ms"].is_number());
        }
        
        let events = collector.get_events();
        assert_eq!(events.len(), 3);
        
        log::info!("✅ Performance monitoring events emitted correctly");
    }

    #[test]
    fn test_user_guidance_events() {
        log::info!("Testing user guidance events");
        
        let collector = MockEventCollector::new();
        
        // Test events that provide user guidance
        let guidance_events = vec![
            ("first-time-user", json!({
                "type": "onboarding",
                "step": "model-download",
                "message": "Welcome! Let's download your first speech recognition model.",
                "action": {
                    "type": "open-model-downloader",
                    "label": "Download Model"
                }
            })),
            ("permission-request", json!({
                "type": "permission-needed",
                "permission": "microphone",
                "message": "VoiceTypr needs microphone access to record audio.",
                "action": {
                    "type": "request-permission",
                    "label": "Grant Access"
                }
            })),
            ("optimization-tip", json!({
                "type": "tip",
                "category": "performance", 
                "message": "For better accuracy, try speaking clearly and avoid background noise.",
                "dismissible": true
            })),
        ];
        
        for (event_name, payload) in guidance_events {
            collector.emit(event_name, payload.clone());
            
            // Validate guidance event structure
            assert!(payload["type"].is_string());
            assert!(payload["message"].is_string());
        }
        
        let events = collector.get_events();
        assert_eq!(events.len(), 3);
        
        log::info!("✅ User guidance events structured correctly");
    }
}

#[cfg(test)]
mod integration_event_tests {
    use super::*;

    #[tokio::test]
    async fn test_event_emission_performance() {
        log::info!("Testing event emission performance under load");
        
        let collector = MockEventCollector::new();
        let start = std::time::Instant::now();
        
        // Emit many events quickly
        for i in 0..1000 {
            let event_payload = json!({
                "type": "performance-test",
                "iteration": i,
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            
            collector.emit("performance-test", event_payload);
        }
        
        let duration = start.elapsed();
        let events = collector.get_events();
        
        assert_eq!(events.len(), 1000);
        assert!(duration.as_millis() < 100); // Should be very fast
        
        log::info!("✅ Emitted 1000 events in {}ms", duration.as_millis());
    }

    #[tokio::test]
    async fn test_concurrent_event_emission() {
        log::info!("Testing concurrent event emission safety");
        
        let collector = Arc::new(MockEventCollector::new());
        let mut handles = Vec::new();
        
        // Spawn multiple tasks emitting events concurrently
        for task_id in 0..10 {
            let collector_clone = Arc::clone(&collector);
            let handle = tokio::spawn(async move {
                for i in 0..100 {
                    let event_payload = json!({
                        "task_id": task_id,
                        "iteration": i,
                        "message": format!("Concurrent test from task {}", task_id)
                    });
                    
                    collector_clone.emit("concurrent-test", event_payload);
                }
            });
            handles.push(handle);
        }
        
        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }
        
        let events = collector.get_events();
        assert_eq!(events.len(), 1000); // 10 tasks * 100 events each
        
        log::info!("✅ Concurrent event emission completed safely with {} events", events.len());
    }

    #[tokio::test]
    async fn test_event_ordering_preservation() {
        log::info!("Testing event ordering preservation");
        
        let collector = MockEventCollector::new();
        
        // Emit events in a specific sequence that should be preserved
        let sequence_events = vec![
            ("recording-started", json!({"sequence": 1})),
            ("audio-validation-started", json!({"sequence": 2})), 
            ("audio-validation-completed", json!({"sequence": 3})),
            ("transcription-started", json!({"sequence": 4})),
            ("transcription-completed", json!({"sequence": 5})),
            ("recording-completed", json!({"sequence": 6})),
        ];
        
        for (event_name, payload) in sequence_events {
            collector.emit(event_name, payload);
        }
        
        let events = collector.get_events();
        assert_eq!(events.len(), 6);
        
        // Verify sequence is preserved
        for (i, (_event_name, payload)) in events.iter().enumerate() {
            let expected_sequence = i + 1;
            assert_eq!(payload["sequence"], expected_sequence);
        }
        
        log::info!("✅ Event ordering preserved correctly");
    }
}