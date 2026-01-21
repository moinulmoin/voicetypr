//! Tests for the remote server lifecycle module
//!
//! Comprehensive tests covering server startup, shutdown, state transitions,
//! configuration management, and error handling.

use std::path::PathBuf;
use std::net::TcpListener;

use crate::remote::lifecycle::{RemoteServerManager, SharingStatus};
use crate::remote::transcription::SharedServerState;

// ============================================================================
// Server Manager Creation Tests
// ============================================================================

#[test]
fn test_server_manager_default_impl() {
    let manager = RemoteServerManager::default();
    assert!(!manager.is_running());
    assert!(manager.get_port().is_none());
    assert!(manager.get_config().is_none());
}

#[test]
fn test_server_manager_new_is_not_running() {
    let manager = RemoteServerManager::new();
    assert!(!manager.is_running());
}

#[test]
fn test_server_manager_new_has_no_port() {
    let manager = RemoteServerManager::new();
    assert!(manager.get_port().is_none());
}

#[test]
fn test_server_manager_new_has_no_config() {
    let manager = RemoteServerManager::new();
    assert!(manager.get_config().is_none());
}

// ============================================================================
// Sharing Status Tests (when server is not running)
// ============================================================================

#[test]
fn test_sharing_status_disabled_by_default() {
    let manager = RemoteServerManager::new();
    let status = manager.get_status();

    assert!(!status.enabled);
}

#[test]
fn test_sharing_status_no_port_when_disabled() {
    let manager = RemoteServerManager::new();
    let status = manager.get_status();

    assert!(status.port.is_none());
}

#[test]
fn test_sharing_status_no_model_when_disabled() {
    let manager = RemoteServerManager::new();
    let status = manager.get_status();

    assert!(status.model_name.is_none());
}

#[test]
fn test_sharing_status_no_server_name_when_disabled() {
    let manager = RemoteServerManager::new();
    let status = manager.get_status();

    assert!(status.server_name.is_none());
}

#[test]
fn test_sharing_status_zero_connections_when_disabled() {
    let manager = RemoteServerManager::new();
    let status = manager.get_status();

    assert_eq!(status.active_connections, 0);
}

#[test]
fn test_sharing_status_no_password_when_disabled() {
    let manager = RemoteServerManager::new();
    let status = manager.get_status();

    assert!(status.password.is_none());
}

// ============================================================================
// Server Startup Sequence Tests
// ============================================================================

#[tokio::test]
async fn test_server_starts_successfully() {
    let mut manager = RemoteServerManager::new();

    let result = manager
        .start(
            47850, // Use unique port for test
            None,
            "Test Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test-model".to_string(),
            "whisper".to_string(),
        )
        .await;

    assert!(result.is_ok());
    manager.stop();
}

#[tokio::test]
async fn test_server_is_running_after_start() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47851,
            None,
            "Test Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    assert!(manager.is_running());
    manager.stop();
}

#[tokio::test]
async fn test_server_reports_correct_port_after_start() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47852,
            None,
            "Test Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    assert_eq!(manager.get_port(), Some(47852));
    manager.stop();
}

#[tokio::test]
async fn test_server_stores_config_after_start() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47853,
            Some("secretpass".to_string()),
            "My Server".to_string(),
            PathBuf::from("/models/large.bin"),
            "large-v3".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    let config = manager.get_config();
    assert!(config.is_some());

    let config = config.unwrap();
    assert_eq!(config.server_name, "My Server");
    assert_eq!(config.password, Some("secretpass".to_string()));
    assert_eq!(config.model_name, "large-v3");
    assert_eq!(config.model_path, PathBuf::from("/models/large.bin"));

    manager.stop();
}

#[tokio::test]
async fn test_server_status_enabled_after_start() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47854,
            None,
            "Test Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    let status = manager.get_status();
    assert!(status.enabled);
    manager.stop();
}

#[tokio::test]
async fn test_server_status_has_correct_port() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47855,
            None,
            "Test Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    let status = manager.get_status();
    assert_eq!(status.port, Some(47855));
    manager.stop();
}

#[tokio::test]
async fn test_server_status_has_correct_model_name() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47856,
            None,
            "Test Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "my-whisper-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    let status = manager.get_status();
    assert_eq!(status.model_name, Some("my-whisper-model".to_string()));
    manager.stop();
}

#[tokio::test]
async fn test_server_status_has_correct_server_name() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47857,
            None,
            "My Desktop PC".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    let status = manager.get_status();
    assert_eq!(status.server_name, Some("My Desktop PC".to_string()));
    manager.stop();
}

#[tokio::test]
async fn test_server_status_has_password_when_set() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47858,
            Some("mypassword123".to_string()),
            "Secure Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    let status = manager.get_status();
    assert_eq!(status.password, Some("mypassword123".to_string()));
    manager.stop();
}

#[tokio::test]
async fn test_server_status_no_password_when_not_set() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47859,
            None,
            "Open Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    let status = manager.get_status();
    assert!(status.password.is_none());
    manager.stop();
}

// ============================================================================
// Server Shutdown Handling Tests
// ============================================================================

#[tokio::test]
async fn test_server_stop_sets_not_running() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47860,
            None,
            "Test Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    manager.stop();
    assert!(!manager.is_running());
}

#[tokio::test]
async fn test_server_stop_clears_port() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47861,
            None,
            "Test Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    manager.stop();
    assert!(manager.get_port().is_none());
}

#[tokio::test]
async fn test_server_stop_clears_config() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47862,
            Some("pass".to_string()),
            "Test Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    manager.stop();
    assert!(manager.get_config().is_none());
}

#[tokio::test]
async fn test_server_status_disabled_after_stop() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47863,
            None,
            "Test Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    manager.stop();
    let status = manager.get_status();
    assert!(!status.enabled);
}

#[tokio::test]
async fn test_stop_on_not_running_server_is_safe() {
    let mut manager = RemoteServerManager::new();

    // Should not panic or error
    manager.stop();
    manager.stop();
    manager.stop();

    assert!(!manager.is_running());
}

// ============================================================================
// Port Binding and Release Tests
// ============================================================================

#[tokio::test]
async fn test_port_can_be_reused_after_stop() {
    let mut manager = RemoteServerManager::new();

    // Start and stop
    manager
        .start(
            47864,
            None,
            "First Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "model1".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();
    manager.stop();

    // Give tokio a moment to fully release the port
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Start again on same port
    let result = manager
        .start(
            47864,
            None,
            "Second Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "model2".to_string(),
            "whisper".to_string(),
        )
        .await;

    assert!(result.is_ok());
    manager.stop();
}

#[tokio::test]
async fn test_different_ports_work() {
    let mut manager1 = RemoteServerManager::new();
    let mut manager2 = RemoteServerManager::new();

    manager1
        .start(
            47865,
            None,
            "Server 1".to_string(),
            PathBuf::from("/fake/model.bin"),
            "model1".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    manager2
        .start(
            47866,
            None,
            "Server 2".to_string(),
            PathBuf::from("/fake/model.bin"),
            "model2".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    assert_eq!(manager1.get_port(), Some(47865));
    assert_eq!(manager2.get_port(), Some(47866));

    manager1.stop();
    manager2.stop();
}

// ============================================================================
// Configuration Changes While Running (update_model) Tests
// ============================================================================

#[tokio::test]
async fn test_update_model_changes_model_name() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47867,
            None,
            "Test Server".to_string(),
            PathBuf::from("/models/old.bin"),
            "old-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    manager.update_model(
        PathBuf::from("/models/new.bin"),
        "new-model".to_string(),
        "whisper".to_string(),
    );

    let config = manager.get_config().unwrap();
    assert_eq!(config.model_name, "new-model");
    manager.stop();
}

#[tokio::test]
async fn test_update_model_changes_model_path() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47868,
            None,
            "Test Server".to_string(),
            PathBuf::from("/models/old.bin"),
            "old-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    manager.update_model(
        PathBuf::from("/models/updated.bin"),
        "new-model".to_string(),
        "whisper".to_string(),
    );

    let config = manager.get_config().unwrap();
    assert_eq!(config.model_path, PathBuf::from("/models/updated.bin"));
    manager.stop();
}

#[tokio::test]
async fn test_update_model_status_reflects_change() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47869,
            None,
            "Test Server".to_string(),
            PathBuf::from("/models/old.bin"),
            "old-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    assert_eq!(manager.get_status().model_name, Some("old-model".to_string()));

    manager.update_model(
        PathBuf::from("/models/new.bin"),
        "updated-model".to_string(),
        "parakeet".to_string(),
    );

    // Note: get_status reads from config, not shared_state
    // The status should reflect the updated config
    assert_eq!(manager.get_status().model_name, Some("updated-model".to_string()));
    manager.stop();
}

#[tokio::test]
async fn test_update_model_when_not_running_is_safe() {
    let mut manager = RemoteServerManager::new();

    // Should not panic when server is not running
    manager.update_model(
        PathBuf::from("/models/new.bin"),
        "new-model".to_string(),
        "whisper".to_string(),
    );

    // Config should still be None since server never started
    assert!(manager.get_config().is_none());
}

#[tokio::test]
async fn test_update_model_with_different_engine() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47870,
            None,
            "Test Server".to_string(),
            PathBuf::from("/models/whisper.bin"),
            "whisper-model".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    // Switch to parakeet engine
    manager.update_model(
        PathBuf::from("/models/parakeet"),
        "parakeet-model".to_string(),
        "parakeet".to_string(),
    );

    let config = manager.get_config().unwrap();
    assert_eq!(config.model_name, "parakeet-model");
    manager.stop();
}

// ============================================================================
// Multiple Start/Stop Cycles Tests
// ============================================================================

#[tokio::test]
async fn test_three_consecutive_start_stop_cycles() {
    let mut manager = RemoteServerManager::new();

    for i in 0..3 {
        let port = 47871 + i;
        manager
            .start(
                port,
                None,
                format!("Server Cycle {}", i),
                PathBuf::from("/fake/model.bin"),
                format!("model-{}", i),
                "whisper".to_string(),
            )
            .await
            .unwrap();

        assert!(manager.is_running());
        assert_eq!(manager.get_port(), Some(port));

        manager.stop();

        assert!(!manager.is_running());
        assert!(manager.get_port().is_none());
        assert!(manager.get_config().is_none());
    }
}

#[tokio::test]
async fn test_start_while_running_stops_previous() {
    let mut manager = RemoteServerManager::new();

    // Start first server
    manager
        .start(
            47874,
            None,
            "Server 1".to_string(),
            PathBuf::from("/models/model1.bin"),
            "model1".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    assert_eq!(manager.get_port(), Some(47874));
    assert_eq!(manager.get_status().model_name, Some("model1".to_string()));

    // Start second server without explicit stop
    manager
        .start(
            47875,
            Some("newpass".to_string()),
            "Server 2".to_string(),
            PathBuf::from("/models/model2.bin"),
            "model2".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    // Should now be running on new port with new config
    assert!(manager.is_running());
    assert_eq!(manager.get_port(), Some(47875));
    assert_eq!(manager.get_status().model_name, Some("model2".to_string()));
    assert_eq!(manager.get_status().password, Some("newpass".to_string()));

    manager.stop();
}

#[tokio::test]
async fn test_rapid_start_stop_cycles() {
    let mut manager = RemoteServerManager::new();

    // Rapidly start and stop 5 times
    for i in 0..5 {
        manager
            .start(
                47876 + i,
                None,
                format!("Rapid Server {}", i),
                PathBuf::from("/fake/model.bin"),
                "test".to_string(),
                "whisper".to_string(),
            )
            .await
            .unwrap();
        manager.stop();
    }

    // Manager should be in clean state
    assert!(!manager.is_running());
    assert!(manager.get_port().is_none());
    assert!(manager.get_config().is_none());
}

// ============================================================================
// State Transition Tests
// ============================================================================

#[tokio::test]
async fn test_state_transition_new_to_running() {
    let mut manager = RemoteServerManager::new();

    // Initial state
    assert!(!manager.is_running());
    let status1 = manager.get_status();
    assert!(!status1.enabled);

    // After start
    manager
        .start(
            47881,
            None,
            "Test".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    assert!(manager.is_running());
    let status2 = manager.get_status();
    assert!(status2.enabled);

    manager.stop();
}

#[tokio::test]
async fn test_state_transition_running_to_stopped() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47882,
            None,
            "Test".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    // Running state
    assert!(manager.is_running());
    assert!(manager.get_status().enabled);

    // Stop
    manager.stop();

    // Stopped state
    assert!(!manager.is_running());
    assert!(!manager.get_status().enabled);
}

#[tokio::test]
async fn test_state_transition_running_to_running_different_config() {
    let mut manager = RemoteServerManager::new();

    // Start with config A
    manager
        .start(
            47883,
            None,
            "Config A".to_string(),
            PathBuf::from("/models/a.bin"),
            "model-a".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    assert!(manager.is_running());
    assert_eq!(manager.get_status().server_name, Some("Config A".to_string()));

    // Start with config B (implicit stop + start)
    manager
        .start(
            47884,
            Some("pass".to_string()),
            "Config B".to_string(),
            PathBuf::from("/models/b.bin"),
            "model-b".to_string(),
            "parakeet".to_string(),
        )
        .await
        .unwrap();

    // Still running but with new config
    assert!(manager.is_running());
    assert_eq!(manager.get_status().server_name, Some("Config B".to_string()));
    assert_eq!(manager.get_status().port, Some(47884));

    manager.stop();
}

// ============================================================================
// SharedServerState Tests
// ============================================================================

#[test]
fn test_shared_server_state_creation() {
    let state = SharedServerState::new(
        "test-model".to_string(),
        PathBuf::from("/test/path.bin"),
        "whisper".to_string(),
    );

    assert_eq!(state.get_model_name(), "test-model");
    assert_eq!(state.get_model_path(), PathBuf::from("/test/path.bin"));
    assert_eq!(state.get_engine(), "whisper");
}

#[test]
fn test_shared_server_state_update() {
    let state = SharedServerState::new(
        "old-model".to_string(),
        PathBuf::from("/old/path.bin"),
        "whisper".to_string(),
    );

    state.update_model(
        "new-model".to_string(),
        PathBuf::from("/new/path.bin"),
        "parakeet".to_string(),
    );

    assert_eq!(state.get_model_name(), "new-model");
    assert_eq!(state.get_model_path(), PathBuf::from("/new/path.bin"));
    assert_eq!(state.get_engine(), "parakeet");
}

#[test]
fn test_shared_server_state_clone_shares_data() {
    let state1 = SharedServerState::new(
        "original".to_string(),
        PathBuf::from("/original.bin"),
        "whisper".to_string(),
    );

    let state2 = state1.clone();

    // Update through state1
    state1.update_model(
        "updated".to_string(),
        PathBuf::from("/updated.bin"),
        "parakeet".to_string(),
    );

    // state2 should see the update (Arc sharing)
    assert_eq!(state2.get_model_name(), "updated");
    assert_eq!(state2.get_model_path(), PathBuf::from("/updated.bin"));
    assert_eq!(state2.get_engine(), "parakeet");
}

// ============================================================================
// SharingStatus Serialization Tests
// ============================================================================

#[test]
fn test_sharing_status_serializes_correctly() {
    let status = SharingStatus {
        enabled: true,
        port: Some(47842),
        model_name: Some("large-v3".to_string()),
        server_name: Some("Test Server".to_string()),
        active_connections: 5,
        password: Some("secret".to_string()),
    };

    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("\"enabled\":true"));
    assert!(json.contains("\"port\":47842"));
    assert!(json.contains("\"model_name\":\"large-v3\""));
    assert!(json.contains("\"server_name\":\"Test Server\""));
    assert!(json.contains("\"active_connections\":5"));
    assert!(json.contains("\"password\":\"secret\""));
}

#[test]
fn test_sharing_status_disabled_serializes_correctly() {
    let status = SharingStatus {
        enabled: false,
        port: None,
        model_name: None,
        server_name: None,
        active_connections: 0,
        password: None,
    };

    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("\"enabled\":false"));
    assert!(json.contains("\"port\":null"));
    assert!(json.contains("\"model_name\":null"));
    assert!(json.contains("\"active_connections\":0"));
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_start_on_occupied_port_behavior() {
    // First, bind to a port using TcpListener
    let listener = TcpListener::bind("127.0.0.1:47890").expect("Failed to bind test port");

    let mut manager = RemoteServerManager::new();

    // Try to start server on the same port
    // Note: warp binds to 0.0.0.0 while our listener binds to 127.0.0.1
    // so this may or may not fail depending on OS behavior
    let _result = manager
        .start(
            47890,
            None,
            "Test".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test".to_string(),
            "whisper".to_string(),
        )
        .await;

    // Clean up regardless of result
    drop(listener);
    manager.stop();

    // The test passes either way - we're just ensuring no panic occurs
    // and the manager handles the situation gracefully
}

#[tokio::test]
async fn test_empty_server_name_works() {
    let mut manager = RemoteServerManager::new();

    let result = manager
        .start(
            47891,
            None,
            "".to_string(), // Empty server name
            PathBuf::from("/fake/model.bin"),
            "test".to_string(),
            "whisper".to_string(),
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(manager.get_status().server_name, Some("".to_string()));
    manager.stop();
}

#[tokio::test]
async fn test_empty_model_name_works() {
    let mut manager = RemoteServerManager::new();

    let result = manager
        .start(
            47892,
            None,
            "Test Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "".to_string(), // Empty model name
            "whisper".to_string(),
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(manager.get_status().model_name, Some("".to_string()));
    manager.stop();
}

#[tokio::test]
async fn test_empty_password_string_is_some() {
    let mut manager = RemoteServerManager::new();

    manager
        .start(
            47893,
            Some("".to_string()), // Empty string password
            "Test Server".to_string(),
            PathBuf::from("/fake/model.bin"),
            "test".to_string(),
            "whisper".to_string(),
        )
        .await
        .unwrap();

    // Empty string should be stored as Some("")
    assert_eq!(manager.get_status().password, Some("".to_string()));
    manager.stop();
}
