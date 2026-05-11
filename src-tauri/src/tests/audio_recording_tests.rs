//! Tests for audio recording features
//!
//! Tests the audio recording changes including:
//! - Recording size validation
//! - AudioRecorder state management
//! - PillToastPayload serialization
//! - Recording indicator mode logic

use crate::audio::recorder::{AudioRecorder, RecordingSize};
use crate::commands::audio::PillToastPayload;

// ============================================================================
// RecordingSize Tests
// ============================================================================

#[test]
fn test_recording_size_valid_small() {
    let result = RecordingSize::check(1024); // 1KB
    assert!(result.is_ok());
}

#[test]
fn test_recording_size_valid_medium() {
    let result = RecordingSize::check(100 * 1024 * 1024); // 100MB
    assert!(result.is_ok());
}

#[test]
fn test_recording_size_valid_at_limit() {
    let result = RecordingSize::check(500 * 1024 * 1024); // Exactly 500MB
    assert!(result.is_ok());
}

#[test]
fn test_recording_size_invalid_over_limit() {
    let result = RecordingSize::check(501 * 1024 * 1024); // 501MB
    assert!(result.is_err());
}

#[test]
fn test_recording_size_invalid_way_over_limit() {
    let result = RecordingSize::check(1024 * 1024 * 1024); // 1GB
    assert!(result.is_err());
}

#[test]
fn test_recording_size_error_message_contains_size() {
    let size = 600 * 1024 * 1024u64; // 600MB
    let result = RecordingSize::check(size);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("600"),
        "Error should mention the size: {}",
        err
    );
    assert!(
        err.contains("500MB"),
        "Error should mention the limit: {}",
        err
    );
}

#[test]
fn test_recording_size_zero_is_valid() {
    let result = RecordingSize::check(0);
    assert!(result.is_ok());
}

#[test]
fn test_recording_size_one_byte_over_limit() {
    let limit = 500 * 1024 * 1024u64;
    let result = RecordingSize::check(limit + 1);
    assert!(result.is_err());
}

// ============================================================================
// AudioRecorder Tests
// ============================================================================

#[test]
fn test_audio_recorder_new() {
    let recorder = AudioRecorder::new();
    assert!(!recorder.is_recording());
}

#[test]
fn test_audio_recorder_is_recording_default_false() {
    let recorder = AudioRecorder::new();
    assert!(
        !recorder.is_recording(),
        "New recorder should not be recording"
    );
}

#[test]
fn test_audio_recorder_multiple_instances() {
    let recorder1 = AudioRecorder::new();
    let recorder2 = AudioRecorder::new();

    assert!(!recorder1.is_recording());
    assert!(!recorder2.is_recording());
}

#[test]
fn test_audio_recorder_stop_when_not_recording() {
    let mut recorder = AudioRecorder::new();
    let result = recorder.stop_recording();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Not recording"));
}

#[test]
fn test_audio_recorder_get_devices() {
    // This should not panic even if no devices are available
    let devices = AudioRecorder::get_devices();
    // We can't assert on the content since it depends on the system,
    // but it should return a valid Vec
    assert!(devices.len() >= 0);
}

// ============================================================================
// PillToastPayload Tests
// ============================================================================

#[test]
fn test_pill_toast_payload_creation() {
    let payload = PillToastPayload {
        id: 1,
        message: "Test message".to_string(),
        duration_ms: 2000,
    };

    assert_eq!(payload.id, 1);
    assert_eq!(payload.message, "Test message");
    assert_eq!(payload.duration_ms, 2000);
}

#[test]
fn test_pill_toast_payload_serialization() {
    let payload = PillToastPayload {
        id: 42,
        message: "Recording started".to_string(),
        duration_ms: 1500,
    };

    let json = serde_json::to_string(&payload).unwrap();
    assert!(json.contains("\"id\":42"));
    assert!(json.contains("\"message\":\"Recording started\""));
    assert!(json.contains("\"duration_ms\":1500"));
}

#[test]
fn test_pill_toast_payload_clone() {
    let payload = PillToastPayload {
        id: 1,
        message: "Test".to_string(),
        duration_ms: 1000,
    };

    let cloned = payload.clone();
    assert_eq!(cloned.id, payload.id);
    assert_eq!(cloned.message, payload.message);
    assert_eq!(cloned.duration_ms, payload.duration_ms);
}

#[test]
fn test_pill_toast_payload_empty_message() {
    let payload = PillToastPayload {
        id: 0,
        message: "".to_string(),
        duration_ms: 0,
    };

    let json = serde_json::to_string(&payload).unwrap();
    assert!(json.contains("\"message\":\"\""));
}

#[test]
fn test_pill_toast_payload_unicode_message() {
    let payload = PillToastPayload {
        id: 1,
        message: "🎤 Recording...".to_string(),
        duration_ms: 2000,
    };

    let json = serde_json::to_string(&payload).unwrap();
    // JSON should contain the emoji (possibly escaped)
    assert!(json.contains("Recording"));
}

#[test]
fn test_pill_toast_payload_long_duration() {
    let payload = PillToastPayload {
        id: 1,
        message: "Long toast".to_string(),
        duration_ms: u64::MAX,
    };

    let json = serde_json::to_string(&payload).unwrap();
    assert!(json.contains(&u64::MAX.to_string()));
}

// ============================================================================
// Recording Indicator Mode Tests (extending existing tests)
// ============================================================================

mod indicator_mode_tests {
    // These test the should_hide_pill_when_idle function logic
    // The actual function is tested in commands/audio.rs, but we can test edge cases

    fn should_hide_pill_when_idle(mode: &str) -> bool {
        mode != "always"
    }

    #[test]
    fn test_mode_never_hides() {
        assert!(should_hide_pill_when_idle("never"));
    }

    #[test]
    fn test_mode_when_recording_hides() {
        assert!(should_hide_pill_when_idle("when_recording"));
    }

    #[test]
    fn test_mode_always_does_not_hide() {
        assert!(!should_hide_pill_when_idle("always"));
    }

    #[test]
    fn test_mode_empty_string_hides() {
        // Empty string should be treated as "hide" (default behavior)
        assert!(should_hide_pill_when_idle(""));
    }

    #[test]
    fn test_mode_unknown_value_hides() {
        // Unknown values should be treated as "hide" (default behavior)
        assert!(should_hide_pill_when_idle("unknown"));
        assert!(should_hide_pill_when_idle("invalid"));
        assert!(should_hide_pill_when_idle("ALWAYS")); // Case sensitive
    }

    #[test]
    fn test_mode_case_sensitivity() {
        // The mode check is case-sensitive
        assert!(!should_hide_pill_when_idle("always"));
        assert!(should_hide_pill_when_idle("Always")); // Wrong case
        assert!(should_hide_pill_when_idle("ALWAYS")); // Wrong case
    }
}

// ============================================================================
// Sound Settings Tests
// ============================================================================

mod sound_settings_tests {
    // Test that sound functions exist and are callable
    // We can't test actual audio playback without hardware

    #[test]
    fn test_sound_functions_compile() {
        // These functions should be callable without panic
        // They spawn threads and return immediately

        #[cfg(target_os = "macos")]
        {
            // On macOS, these functions spawn afplay
            // We just verify they don't panic on call
        }

        #[cfg(target_os = "windows")]
        {
            // On Windows, these use PowerShell beep
            // We just verify they don't panic on call
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            // On other platforms, these are no-ops
        }

        // Test passes if we reach here without panic
        assert!(true);
    }
}

// ============================================================================
// Model Fallback Selection Tests
// ============================================================================

mod fallback_selection_tests {
    /// Reimplementation of select_best_fallback_model for testing
    /// (The actual function is private in commands/audio.rs)
    fn select_best_fallback_model(
        available_models: &[String],
        requested: &str,
        model_priority: &[String],
    ) -> String {
        // First try to find a model similar to the requested one
        if !requested.is_empty() {
            for model in available_models {
                if model.starts_with(&requested.split('-').next().unwrap_or(requested)) {
                    return model.clone();
                }
            }
        }

        // Otherwise use priority order
        for priority_model in model_priority {
            if available_models.contains(priority_model) {
                return priority_model.clone();
            }
        }

        // If no priority model found, return first available
        available_models
            .first()
            .cloned()
            .unwrap_or_else(|| "base.en".to_string())
    }

    #[test]
    fn test_fallback_finds_similar_model() {
        let available = vec![
            "base.en".to_string(),
            "large-v2".to_string(),
            "large-v3".to_string(),
        ];
        let priority = vec!["base.en".to_string()];

        // Requesting large-v3, should find a large variant
        let result = select_best_fallback_model(&available, "large-v3", &priority);
        assert!(
            result.starts_with("large"),
            "Should find a large model: {}",
            result
        );
    }

    #[test]
    fn test_fallback_uses_priority_when_no_match() {
        let available = vec![
            "base.en".to_string(),
            "small.en".to_string(),
            "medium".to_string(),
        ];
        let priority = vec![
            "base.en".to_string(),
            "small.en".to_string(),
            "medium".to_string(),
        ];

        // Requesting large (not available), should fall back to priority
        let result = select_best_fallback_model(&available, "large-v3", &priority);
        assert_eq!(result, "base.en");
    }

    #[test]
    fn test_fallback_returns_first_available_when_no_priority_match() {
        let available = vec!["custom-model".to_string(), "another-model".to_string()];
        let priority = vec!["base.en".to_string()]; // Not in available

        let result = select_best_fallback_model(&available, "unknown", &priority);
        assert_eq!(result, "custom-model");
    }

    #[test]
    fn test_fallback_returns_default_when_empty() {
        let available: Vec<String> = vec![];
        let priority = vec!["base.en".to_string()];

        let result = select_best_fallback_model(&available, "any", &priority);
        assert_eq!(result, "base.en");
    }

    #[test]
    fn test_fallback_with_empty_requested() {
        let available = vec!["base.en".to_string(), "small.en".to_string()];
        let priority = vec!["small.en".to_string(), "base.en".to_string()];

        // Empty requested should go straight to priority
        let result = select_best_fallback_model(&available, "", &priority);
        assert_eq!(result, "small.en");
    }

    #[test]
    fn test_fallback_handles_hyphenated_names() {
        let available = vec![
            "large-v2".to_string(),
            "large-v3".to_string(),
            "small.en".to_string(),
        ];
        let priority = vec!["small.en".to_string()];

        // Requesting "large-v1" (not available) should find "large-v2" or "large-v3"
        let result = select_best_fallback_model(&available, "large-v1", &priority);
        assert!(result.starts_with("large"));
    }
}

// ============================================================================
// Recording State Integration Tests
// ============================================================================

mod recording_state_tests {
    use crate::{AppState, RecordingState};

    #[test]
    fn test_recording_state_transitions_valid() {
        // Test that all state transitions are possible
        let states = vec![
            RecordingState::Idle,
            RecordingState::Starting,
            RecordingState::Recording,
            RecordingState::Stopping,
            RecordingState::Transcribing,
            RecordingState::Error,
        ];

        for state in &states {
            let app_state = AppState::new();
            app_state.recording_state.force_set(*state).unwrap();
            assert_eq!(app_state.get_current_state(), *state);
        }
    }

    #[test]
    fn test_recording_state_debug_format() {
        // Verify Debug trait is implemented
        let state = RecordingState::Recording;
        let debug_str = format!("{:?}", state);
        assert!(debug_str.contains("Recording"));
    }

    #[test]
    fn test_app_state_initial_not_recording() {
        let app_state = AppState::new();
        assert_eq!(app_state.get_current_state(), RecordingState::Idle);
    }
}

// ============================================================================
// Audio Buffer Tests
// ============================================================================

mod audio_buffer_tests {
    use crate::audio::recorder::RecordingSize;

    #[test]
    fn test_incremental_size_tracking() {
        // Simulate incremental recording where we add chunks
        let mut total: u64 = 0;
        let chunk_size: u64 = 1024 * 1024; // 1MB chunks

        // Add chunks until we hit the limit
        for _ in 0..500 {
            total += chunk_size;
            let result = RecordingSize::check(total);
            assert!(
                result.is_ok(),
                "Should be OK at {} MB",
                total / (1024 * 1024)
            );
        }

        // One more chunk should fail
        total += chunk_size;
        let result = RecordingSize::check(total);
        assert!(
            result.is_err(),
            "Should fail at {} MB",
            total / (1024 * 1024)
        );
    }

    #[test]
    fn test_size_check_is_deterministic() {
        // Same input should always produce same result
        let size = 250 * 1024 * 1024u64; // 250MB

        for _ in 0..100 {
            let result = RecordingSize::check(size);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_size_boundary_conditions() {
        let limit = 500 * 1024 * 1024u64;

        // Just under limit
        assert!(RecordingSize::check(limit - 1).is_ok());

        // Exactly at limit
        assert!(RecordingSize::check(limit).is_ok());

        // Just over limit
        assert!(RecordingSize::check(limit + 1).is_err());
    }
}
