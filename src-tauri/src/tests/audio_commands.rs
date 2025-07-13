#[cfg(test)]
mod tests {
    use crate::{AppState, RecordingState};
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn test_recording_state_default() {
        let state = RecordingState::default();
        assert_eq!(state, RecordingState::Idle);
    }

    #[test]
    fn test_recording_state_serialization() {
        // Test all state values serialize correctly
        let states = vec![
            RecordingState::Idle,
            RecordingState::Starting,
            RecordingState::Recording,
            RecordingState::Stopping,
            RecordingState::Transcribing,
            RecordingState::Error,
        ];

        for state in states {
            let serialized = serde_json::to_string(&state).unwrap();
            assert!(!serialized.is_empty());

            // Verify the JSON output
            let expected = match state {
                RecordingState::Idle => "\"Idle\"",
                RecordingState::Starting => "\"Starting\"",
                RecordingState::Recording => "\"Recording\"",
                RecordingState::Stopping => "\"Stopping\"",
                RecordingState::Transcribing => "\"Transcribing\"",
                RecordingState::Error => "\"Error\"",
            };
            assert_eq!(serialized, expected);
        }
    }

    #[test]
    fn test_app_state_new() {
        let app_state = AppState::new();

        // Verify initial state
        {
            let recording_state = app_state.get_current_state();
            assert_eq!(recording_state, RecordingState::Idle);
        }

        {
            let shortcut = app_state.recording_shortcut.lock().unwrap();
            assert!(shortcut.is_none());
        }

        {
            let path = app_state.current_recording_path.lock().unwrap();
            assert!(path.is_none());
        }

        {
            let task = app_state.transcription_task.lock().unwrap();
            assert!(task.is_none());
        }
    }

    #[test]
    fn test_app_state_recording_state_transitions() {
        let app_state = AppState::new();

        // Test state transitions
        let transitions = vec![
            RecordingState::Starting,
            RecordingState::Recording,
            RecordingState::Stopping,
            RecordingState::Transcribing,
            RecordingState::Idle,
        ];

        for expected_state in transitions {
            // Force set the state since we're testing direct state changes
            app_state.recording_state.force_set(expected_state).unwrap();
            
            // Verify state was set
            let state = app_state.get_current_state();
            assert_eq!(state, expected_state);
        }
    }

    #[test]
    fn test_app_state_path_management() {
        let app_state = AppState::new();
        let test_path = PathBuf::from("/tmp/test_recording.wav");

        // Set path
        {
            let mut path = app_state.current_recording_path.lock().unwrap();
            *path = Some(test_path.clone());
        }

        // Verify path is set
        {
            let path = app_state.current_recording_path.lock().unwrap();
            assert_eq!(path.as_ref().unwrap(), &test_path);
        }

        // Take path (should remove it)
        let taken_path = {
            let mut path = app_state.current_recording_path.lock().unwrap();
            path.take()
        };

        assert_eq!(taken_path.unwrap(), test_path);

        // Verify path is now None
        {
            let path = app_state.current_recording_path.lock().unwrap();
            assert!(path.is_none());
        }
    }

    #[test]
    fn test_app_state_concurrent_access() {
        let app_state = Arc::new(AppState::new());
        let mut handles = vec![];

        // Spawn multiple threads to test concurrent access
        for i in 0..10 {
            let state_clone = app_state.clone();
            let handle = std::thread::spawn(move || {
                // Each thread tries to update the state
                let new_state = if i % 2 == 0 {
                    RecordingState::Recording
                } else {
                    RecordingState::Idle
                };

                // Force set state in concurrent test
                state_clone.recording_state.force_set(new_state).unwrap();
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // State should be valid (either Recording or Idle)
        let final_state = app_state.get_current_state();
        assert!(matches!(
            final_state,
            RecordingState::Recording | RecordingState::Idle
        ));
    }

    #[test]
    fn test_recording_state_equality() {
        assert_eq!(RecordingState::Idle, RecordingState::Idle);
        assert_ne!(RecordingState::Idle, RecordingState::Recording);
        assert_ne!(RecordingState::Starting, RecordingState::Stopping);
    }

    #[test]
    fn test_app_state_error_handling() {
        let app_state = AppState::new();

        // Set to error state
        app_state.recording_state.force_set(RecordingState::Error).unwrap();
        
        // Verify error state
        let state = app_state.get_current_state();
        assert_eq!(state, RecordingState::Error);
        
        // Reset to idle
        app_state.recording_state.force_set(RecordingState::Idle).unwrap();
        
        // Verify idle state
        let state = app_state.get_current_state();
        assert_eq!(state, RecordingState::Idle);
    }

    // Test for the audio recorder functionality would require mocking
    // the actual audio recording hardware, which is complex.
    // These tests focus on the state management aspects.

    #[test]
    fn test_recording_path_validation() {
        let app_state = AppState::new();

        // Test various path scenarios
        let test_paths = vec![
            PathBuf::from("/tmp/recording_123.wav"),
            PathBuf::from("/var/tmp/audio.wav"),
            PathBuf::from("./recordings/test.wav"),
        ];

        for test_path in test_paths {
            {
                let mut path = app_state.current_recording_path.lock().unwrap();
                *path = Some(test_path.clone());
            }

            {
                let path = app_state.current_recording_path.lock().unwrap();
                assert_eq!(path.as_ref().unwrap(), &test_path);
            }
        }
    }

    #[tokio::test]
    async fn test_transcription_task_management() {
        let app_state = AppState::new();

        // Create a dummy task
        let task = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        });

        // Store the task
        {
            let mut task_guard = app_state.transcription_task.lock().unwrap();
            *task_guard = Some(task);
        }

        // Verify task is stored
        {
            let task_guard = app_state.transcription_task.lock().unwrap();
            assert!(task_guard.is_some());
        }

        // Take and await the task
        let task = {
            let mut task_guard = app_state.transcription_task.lock().unwrap();
            task_guard.take()
        };

        if let Some(task) = task {
            // Task should complete successfully
            assert!(task.await.is_ok());
        }

        // Verify task is now None
        {
            let task_guard = app_state.transcription_task.lock().unwrap();
            assert!(task_guard.is_none());
        }
    }

    #[tokio::test]
    async fn test_task_cancellation() {
        let app_state = AppState::new();

        // Create a long-running task
        let task = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        });

        // Store the task
        {
            let mut task_guard = app_state.transcription_task.lock().unwrap();
            *task_guard = Some(task);
        }

        // Cancel the task
        {
            let mut task_guard = app_state.transcription_task.lock().unwrap();
            if let Some(task) = task_guard.take() {
                task.abort();
            }
        }

        // Verify task is cancelled and removed
        {
            let task_guard = app_state.transcription_task.lock().unwrap();
            assert!(task_guard.is_none());
        }
    }
}
