//! Tests for remote transcription module
//!
//! These tests verify the SharedServerState and RealTranscriptionContext
//! functionality for the remote transcription server.

use crate::remote::http::ServerContext;
use crate::remote::transcription::{
    RealTranscriptionContext, SharedServerState, TranscriptionServerConfig,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

// ============================================================================
// SharedServerState Tests
// ============================================================================

/// Test SharedServerState creation with initial values
#[test]
fn test_shared_server_state_new() {
    let state = SharedServerState::new(
        "test-model".to_string(),
        PathBuf::from("/models/test.bin"),
        "whisper".to_string(),
    );

    assert_eq!(state.get_model_name(), "test-model");
    assert_eq!(state.get_model_path(), PathBuf::from("/models/test.bin"));
    assert_eq!(state.get_engine(), "whisper");
}

/// Test SharedServerState update_model method
#[test]
fn test_shared_server_state_update_model() {
    let state = SharedServerState::new(
        "old-model".to_string(),
        PathBuf::from("/models/old.bin"),
        "whisper".to_string(),
    );

    state.update_model(
        "new-model".to_string(),
        PathBuf::from("/models/new.bin"),
        "parakeet".to_string(),
    );

    assert_eq!(state.get_model_name(), "new-model");
    assert_eq!(state.get_model_path(), PathBuf::from("/models/new.bin"));
    assert_eq!(state.get_engine(), "parakeet");
}

/// Test SharedServerState get_model_name returns empty string on default
#[test]
fn test_shared_server_state_empty_model_name() {
    let state = SharedServerState::new(
        "".to_string(),
        PathBuf::new(),
        "whisper".to_string(),
    );

    assert_eq!(state.get_model_name(), "");
}

/// Test SharedServerState get_model_path returns empty path on default
#[test]
fn test_shared_server_state_empty_model_path() {
    let state = SharedServerState::new(
        "test".to_string(),
        PathBuf::new(),
        "whisper".to_string(),
    );

    assert_eq!(state.get_model_path(), PathBuf::new());
}

/// Test SharedServerState Clone implementation
#[test]
fn test_shared_server_state_clone() {
    let state = SharedServerState::new(
        "original".to_string(),
        PathBuf::from("/models/original.bin"),
        "whisper".to_string(),
    );

    let cloned = state.clone();

    // Both should have same values
    assert_eq!(cloned.get_model_name(), "original");
    assert_eq!(cloned.get_model_path(), PathBuf::from("/models/original.bin"));

    // Update original - cloned should also see the update (shared Arc)
    state.update_model(
        "updated".to_string(),
        PathBuf::from("/models/updated.bin"),
        "whisper".to_string(),
    );

    assert_eq!(cloned.get_model_name(), "updated");
    assert_eq!(cloned.get_model_path(), PathBuf::from("/models/updated.bin"));
}

/// Test SharedServerState is thread-safe
#[test]
fn test_shared_server_state_thread_safety() {
    let state = Arc::new(SharedServerState::new(
        "initial".to_string(),
        PathBuf::from("/models/initial.bin"),
        "whisper".to_string(),
    ));

    let state_clone = Arc::clone(&state);

    // Spawn a thread to update the model
    let handle = thread::spawn(move || {
        state_clone.update_model(
            "from-thread".to_string(),
            PathBuf::from("/models/from-thread.bin"),
            "parakeet".to_string(),
        );
    });

    handle.join().expect("Thread panicked");

    // Main thread should see the update
    assert_eq!(state.get_model_name(), "from-thread");
    assert_eq!(state.get_model_path(), PathBuf::from("/models/from-thread.bin"));
    assert_eq!(state.get_engine(), "parakeet");
}

/// Test SharedServerState concurrent reads
#[test]
fn test_shared_server_state_concurrent_reads() {
    let state = Arc::new(SharedServerState::new(
        "concurrent-test".to_string(),
        PathBuf::from("/models/concurrent.bin"),
        "whisper".to_string(),
    ));

    let mut handles = vec![];

    // Spawn multiple threads to read concurrently
    for _ in 0..10 {
        let state_clone = Arc::clone(&state);
        let handle = thread::spawn(move || {
            for _ in 0..100 {
                let name = state_clone.get_model_name();
                let path = state_clone.get_model_path();
                let engine = state_clone.get_engine();
                assert!(!name.is_empty());
                assert!(!path.as_os_str().is_empty());
                assert!(!engine.is_empty());
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}

// ============================================================================
// RealTranscriptionContext Tests
// ============================================================================

/// Test RealTranscriptionContext creation with shared state
#[test]
fn test_new_with_shared_state() {
    let shared_state = SharedServerState::new(
        "shared-model".to_string(),
        PathBuf::from("/models/shared.bin"),
        "whisper".to_string(),
    );

    let ctx = RealTranscriptionContext::new_with_shared_state(
        "Shared Server".to_string(),
        Some("password".to_string()),
        shared_state.clone(),
    );

    assert_eq!(ctx.get_model_name(), "shared-model");
    assert_eq!(ctx.get_server_name(), "Shared Server");
    assert_eq!(ctx.get_password(), Some("password".to_string()));
    assert_eq!(ctx.get_model_path(), PathBuf::from("/models/shared.bin"));
}

/// Test RealTranscriptionContext get_model_path method
#[test]
fn test_context_get_model_path() {
    let config = TranscriptionServerConfig {
        server_name: "Test".to_string(),
        password: None,
        model_path: PathBuf::from("/custom/path/model.bin"),
        model_name: "custom".to_string(),
    };

    let ctx = RealTranscriptionContext::new(config);

    assert_eq!(ctx.get_model_path(), PathBuf::from("/custom/path/model.bin"));
}

/// Test RealTranscriptionContext implements ServerContext trait
#[test]
fn test_server_context_trait_implementation() {
    let config = TranscriptionServerConfig {
        server_name: "Trait Test Server".to_string(),
        password: Some("trait-pass".to_string()),
        model_path: PathBuf::from("/models/trait.bin"),
        model_name: "trait-model".to_string(),
    };

    let ctx = RealTranscriptionContext::new(config);

    // Test trait methods
    let ctx_ref: &dyn ServerContext = &ctx;
    assert_eq!(ctx_ref.get_model_name(), "trait-model");
    assert_eq!(ctx_ref.get_server_name(), "Trait Test Server");
    assert_eq!(ctx_ref.get_password(), Some("trait-pass".to_string()));
}

/// Test RealTranscriptionContext shared state updates propagate
#[test]
fn test_context_shared_state_updates_propagate() {
    let shared_state = SharedServerState::new(
        "initial-model".to_string(),
        PathBuf::from("/models/initial.bin"),
        "whisper".to_string(),
    );

    let ctx = RealTranscriptionContext::new_with_shared_state(
        "Server".to_string(),
        None,
        shared_state.clone(),
    );

    // Verify initial state
    assert_eq!(ctx.get_model_name(), "initial-model");

    // Update via shared state
    shared_state.update_model(
        "updated-model".to_string(),
        PathBuf::from("/models/updated.bin"),
        "parakeet".to_string(),
    );

    // Context should see the update
    assert_eq!(ctx.get_model_name(), "updated-model");
    assert_eq!(ctx.get_model_path(), PathBuf::from("/models/updated.bin"));
}

/// Test RealTranscriptionContext update_model affects shared state
#[test]
fn test_context_update_model_affects_shared_state() {
    let shared_state = SharedServerState::new(
        "start".to_string(),
        PathBuf::from("/models/start.bin"),
        "whisper".to_string(),
    );

    let mut ctx = RealTranscriptionContext::new_with_shared_state(
        "Server".to_string(),
        None,
        shared_state.clone(),
    );

    // Update via context
    ctx.update_model(
        PathBuf::from("/models/end.bin"),
        "end".to_string(),
        "parakeet".to_string(),
    );

    // Both should see the update
    assert_eq!(ctx.get_model_name(), "end");
    assert_eq!(shared_state.get_model_name(), "end");
    assert_eq!(shared_state.get_engine(), "parakeet");
}

/// Test TranscriptionServerConfig creation
#[test]
fn test_transcription_server_config_creation() {
    let config = TranscriptionServerConfig {
        server_name: "Config Test".to_string(),
        password: Some("secret123".to_string()),
        model_path: PathBuf::from("/path/to/model.bin"),
        model_name: "large-v3".to_string(),
    };

    assert_eq!(config.server_name, "Config Test");
    assert_eq!(config.password, Some("secret123".to_string()));
    assert_eq!(config.model_path, PathBuf::from("/path/to/model.bin"));
    assert_eq!(config.model_name, "large-v3");
}

/// Test TranscriptionServerConfig Clone
#[test]
fn test_transcription_server_config_clone() {
    let config = TranscriptionServerConfig {
        server_name: "Original".to_string(),
        password: None,
        model_path: PathBuf::from("/models/original.bin"),
        model_name: "original".to_string(),
    };

    let cloned = config.clone();

    assert_eq!(cloned.server_name, "Original");
    assert_eq!(cloned.password, None);
    assert_eq!(cloned.model_path, PathBuf::from("/models/original.bin"));
    assert_eq!(cloned.model_name, "original");
}

/// Test RealTranscriptionContext without password
#[test]
fn test_context_without_password() {
    let config = TranscriptionServerConfig {
        server_name: "No Auth Server".to_string(),
        password: None,
        model_path: PathBuf::from("/models/noauth.bin"),
        model_name: "noauth".to_string(),
    };

    let ctx = RealTranscriptionContext::new(config);

    assert!(ctx.get_password().is_none());
}

/// Test multiple password updates
#[test]
fn test_multiple_password_updates() {
    let config = TranscriptionServerConfig {
        server_name: "Password Test".to_string(),
        password: Some("first".to_string()),
        model_path: PathBuf::from("/models/test.bin"),
        model_name: "test".to_string(),
    };

    let mut ctx = RealTranscriptionContext::new(config);

    assert_eq!(ctx.get_password(), Some("first".to_string()));

    ctx.update_password(Some("second".to_string()));
    assert_eq!(ctx.get_password(), Some("second".to_string()));

    ctx.update_password(Some("third".to_string()));
    assert_eq!(ctx.get_password(), Some("third".to_string()));

    ctx.update_password(None);
    assert!(ctx.get_password().is_none());
}

/// Test SharedServerState with different engine types
#[test]
fn test_shared_server_state_engine_types() {
    let state = SharedServerState::new(
        "model".to_string(),
        PathBuf::from("/models/model.bin"),
        "whisper".to_string(),
    );

    assert_eq!(state.get_engine(), "whisper");

    state.update_model(
        "model".to_string(),
        PathBuf::from("/models/model.bin"),
        "parakeet".to_string(),
    );

    assert_eq!(state.get_engine(), "parakeet");

    state.update_model(
        "model".to_string(),
        PathBuf::from("/models/model.bin"),
        "custom-engine".to_string(),
    );

    assert_eq!(state.get_engine(), "custom-engine");
}
