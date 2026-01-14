//! Tests for the remote transcription server module
//!
//! These tests verify the HTTP server functionality for sharing
//! transcription capabilities with other VoiceTypr instances.

use crate::remote::server::{RemoteServerConfig, ServerStatus, StatusResponse, TranscribeResponse};

/// Test that StatusResponse serializes correctly
#[test]
fn test_status_response_serialization() {
    let response = StatusResponse {
        status: "ok".to_string(),
        version: "1.11.2".to_string(),
        model: "large-v3-turbo".to_string(),
        name: "Desktop-PC".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"status\":\"ok\""));
    assert!(json.contains("\"version\":\"1.11.2\""));
    assert!(json.contains("\"model\":\"large-v3-turbo\""));
    assert!(json.contains("\"name\":\"Desktop-PC\""));
}

/// Test that StatusResponse deserializes correctly
#[test]
fn test_status_response_deserialization() {
    let json = r#"{
        "status": "ok",
        "version": "1.11.2",
        "model": "large-v3-turbo",
        "name": "Desktop-PC"
    }"#;

    let response: StatusResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.status, "ok");
    assert_eq!(response.version, "1.11.2");
    assert_eq!(response.model, "large-v3-turbo");
    assert_eq!(response.name, "Desktop-PC");
}

/// Test that TranscribeResponse serializes correctly
#[test]
fn test_transcribe_response_serialization() {
    let response = TranscribeResponse {
        text: "Hello, world!".to_string(),
        duration_ms: 3500,
        model: "large-v3-turbo".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"text\":\"Hello, world!\""));
    assert!(json.contains("\"duration_ms\":3500"));
    assert!(json.contains("\"model\":\"large-v3-turbo\""));
}

/// Test that TranscribeResponse deserializes correctly
#[test]
fn test_transcribe_response_deserialization() {
    let json = r#"{
        "text": "This is a test transcription.",
        "duration_ms": 2500,
        "model": "base.en"
    }"#;

    let response: TranscribeResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.text, "This is a test transcription.");
    assert_eq!(response.duration_ms, 2500);
    assert_eq!(response.model, "base.en");
}

/// Test default server configuration
#[test]
fn test_server_config_defaults() {
    let config = RemoteServerConfig::default();
    assert_eq!(config.port, 47842);
    assert!(config.password.is_none());
    assert!(!config.enabled);
}

/// Test server configuration with custom values
#[test]
fn test_server_config_custom() {
    let config = RemoteServerConfig {
        port: 8080,
        password: Some("secret123".to_string()),
        enabled: true,
    };

    assert_eq!(config.port, 8080);
    assert_eq!(config.password, Some("secret123".to_string()));
    assert!(config.enabled);
}

/// Test server configuration serialization for settings storage
#[test]
fn test_server_config_serialization() {
    let config = RemoteServerConfig {
        port: 47842,
        password: Some("mypassword".to_string()),
        enabled: true,
    };

    let json = serde_json::to_string(&config).unwrap();
    let restored: RemoteServerConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.port, config.port);
    assert_eq!(restored.password, config.password);
    assert_eq!(restored.enabled, config.enabled);
}

/// Test password validation - correct password should succeed
#[test]
fn test_password_validation_correct() {
    let config = RemoteServerConfig {
        port: 47842,
        password: Some("secret123".to_string()),
        enabled: true,
    };

    assert!(config.validate_password(Some("secret123")));
}

/// Test password validation - wrong password should fail
#[test]
fn test_password_validation_wrong() {
    let config = RemoteServerConfig {
        port: 47842,
        password: Some("secret123".to_string()),
        enabled: true,
    };

    assert!(!config.validate_password(Some("wrongpassword")));
}

/// Test password validation - no password provided when required should fail
#[test]
fn test_password_validation_missing_when_required() {
    let config = RemoteServerConfig {
        port: 47842,
        password: Some("secret123".to_string()),
        enabled: true,
    };

    assert!(!config.validate_password(None));
}

/// Test password validation - no password required, none provided should succeed
#[test]
fn test_password_validation_not_required() {
    let config = RemoteServerConfig {
        port: 47842,
        password: None,
        enabled: true,
    };

    assert!(config.validate_password(None));
}

/// Test password validation - no password required, password provided should still succeed
#[test]
fn test_password_validation_not_required_but_provided() {
    let config = RemoteServerConfig {
        port: 47842,
        password: None,
        enabled: true,
    };

    // Even if client sends a password, it should be accepted when server doesn't require one
    assert!(config.validate_password(Some("anything")));
}

/// Test server status representation
#[test]
fn test_server_status_idle() {
    let status = ServerStatus::Idle;
    assert!(!status.is_running());
}

/// Test server status when running
#[test]
fn test_server_status_running() {
    let status = ServerStatus::Running {
        port: 47842,
        connections: 0,
    };
    assert!(status.is_running());
}

/// Test server status with active connections
#[test]
fn test_server_status_with_connections() {
    let status = ServerStatus::Running {
        port: 47842,
        connections: 3,
    };

    if let ServerStatus::Running { connections, .. } = status {
        assert_eq!(connections, 3);
    } else {
        panic!("Expected Running status");
    }
}

/// Test error response format
#[test]
fn test_error_response_serialization() {
    use crate::remote::server::ErrorResponse;

    let error = ErrorResponse {
        error: "unauthorized".to_string(),
    };

    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("\"error\":\"unauthorized\""));
}
