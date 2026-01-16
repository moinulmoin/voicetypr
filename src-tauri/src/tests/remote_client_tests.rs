//! Tests for the remote transcription client module
//!
//! These tests verify the HTTP client functionality for connecting to
//! other VoiceTypr instances for remote transcription.

use crate::remote::client::{
    calculate_timeout_ms, RemoteServerConnection, TranscriptionRequest, TranscriptionSource,
};

/// Test timeout calculation for live recordings (30 seconds)
#[test]
fn test_timeout_calculation_live_30s() {
    let timeout = calculate_timeout_ms(30_000, TranscriptionSource::LiveRecording);
    // Live recording: base 30s + 2x duration
    // 30_000 + 2 * 30_000 = 90_000ms = 90 seconds
    assert_eq!(timeout, 90_000);
}

/// Test timeout calculation for live recordings (60 seconds) - hits cap
#[test]
fn test_timeout_calculation_live_60s() {
    let timeout = calculate_timeout_ms(60_000, TranscriptionSource::LiveRecording);
    // 30_000 + 2 * 60_000 = 150_000ms, but capped at 2 minutes (120_000ms)
    assert_eq!(timeout, 120_000);
}

/// Test timeout calculation for live recordings caps at 2 minutes
#[test]
fn test_timeout_calculation_live_caps_at_2min() {
    let timeout = calculate_timeout_ms(120_000, TranscriptionSource::LiveRecording);
    // Would be 30_000 + 2 * 120_000 = 270_000ms but caps at 120_000ms
    assert_eq!(timeout, 120_000);
}

/// Test timeout calculation for uploaded files (1 minute audio)
#[test]
fn test_timeout_calculation_upload_1min() {
    let timeout = calculate_timeout_ms(60_000, TranscriptionSource::Upload);
    // Upload: base 60s + 3x duration
    // 60_000 + 3 * 60_000 = 240_000ms = 4 minutes
    assert_eq!(timeout, 240_000);
}

/// Test timeout calculation for uploaded files (10 minute audio)
#[test]
fn test_timeout_calculation_upload_10min() {
    let timeout = calculate_timeout_ms(600_000, TranscriptionSource::Upload);
    // 60_000 + 3 * 600_000 = 1_860_000ms = 31 minutes
    assert_eq!(timeout, 1_860_000);
}

/// Test timeout calculation for uploaded files (1 hour audio)
#[test]
fn test_timeout_calculation_upload_1hour() {
    let timeout = calculate_timeout_ms(3_600_000, TranscriptionSource::Upload);
    // 60_000 + 3 * 3_600_000 = 10_860_000ms = ~3 hours
    assert_eq!(timeout, 10_860_000);
}

/// Test minimum timeout is always at least the base
#[test]
fn test_timeout_calculation_minimum() {
    let timeout = calculate_timeout_ms(1_000, TranscriptionSource::LiveRecording);
    // 30_000 + 2 * 1_000 = 32_000ms
    assert_eq!(timeout, 32_000);
}

/// Test remote server connection creation
#[test]
fn test_remote_server_connection_creation() {
    let conn = RemoteServerConnection::new(
        "192.168.1.100".to_string(),
        47842,
        Some("mypassword".to_string()),
    );

    assert_eq!(conn.host, "192.168.1.100");
    assert_eq!(conn.port, 47842);
    assert_eq!(conn.password, Some("mypassword".to_string()));
}

/// Test remote server connection without password
#[test]
fn test_remote_server_connection_no_password() {
    let conn = RemoteServerConnection::new("10.0.0.5".to_string(), 8080, None);

    assert_eq!(conn.host, "10.0.0.5");
    assert_eq!(conn.port, 8080);
    assert!(conn.password.is_none());
}

/// Test URL generation for status endpoint
#[test]
fn test_remote_server_status_url() {
    let conn = RemoteServerConnection::new("192.168.1.100".to_string(), 47842, None);

    assert_eq!(conn.status_url(), "http://192.168.1.100:47842/api/v1/status");
}

/// Test URL generation for transcribe endpoint
#[test]
fn test_remote_server_transcribe_url() {
    let conn = RemoteServerConnection::new("192.168.1.100".to_string(), 47842, None);

    assert_eq!(
        conn.transcribe_url(),
        "http://192.168.1.100:47842/api/v1/transcribe"
    );
}

/// Test URL generation with different port
#[test]
fn test_remote_server_urls_custom_port() {
    let conn = RemoteServerConnection::new("localhost".to_string(), 9999, None);

    assert_eq!(conn.status_url(), "http://localhost:9999/api/v1/status");
    assert_eq!(
        conn.transcribe_url(),
        "http://localhost:9999/api/v1/transcribe"
    );
}

/// Test transcription request creation for live recording
#[test]
fn test_transcription_request_live() {
    let audio_data = vec![0u8; 1000];
    let request = TranscriptionRequest::new(audio_data.clone(), TranscriptionSource::LiveRecording);

    assert_eq!(request.audio_data, audio_data);
    assert_eq!(request.source, TranscriptionSource::LiveRecording);
}

/// Test transcription request creation for upload
#[test]
fn test_transcription_request_upload() {
    let audio_data = vec![1u8; 5000];
    let request = TranscriptionRequest::new(audio_data.clone(), TranscriptionSource::Upload);

    assert_eq!(request.audio_data, audio_data);
    assert_eq!(request.source, TranscriptionSource::Upload);
}

/// Test connection serialization for settings storage
#[test]
fn test_remote_server_connection_serialization() {
    let conn = RemoteServerConnection::new(
        "192.168.1.100".to_string(),
        47842,
        Some("secret".to_string()),
    );

    let json = serde_json::to_string(&conn).unwrap();
    let restored: RemoteServerConnection = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.host, conn.host);
    assert_eq!(restored.port, conn.port);
    assert_eq!(restored.password, conn.password);
}

/// Test TranscriptionSource serialization
#[test]
fn test_transcription_source_serialization() {
    let live = TranscriptionSource::LiveRecording;
    let upload = TranscriptionSource::Upload;

    let live_json = serde_json::to_string(&live).unwrap();
    let upload_json = serde_json::to_string(&upload).unwrap();

    assert_eq!(live_json, "\"LiveRecording\"");
    assert_eq!(upload_json, "\"Upload\"");

    let restored_live: TranscriptionSource = serde_json::from_str(&live_json).unwrap();
    let restored_upload: TranscriptionSource = serde_json::from_str(&upload_json).unwrap();

    assert_eq!(restored_live, TranscriptionSource::LiveRecording);
    assert_eq!(restored_upload, TranscriptionSource::Upload);
}

/// Test connection display name generation
#[test]
fn test_remote_server_connection_display_name() {
    let conn = RemoteServerConnection::new("192.168.1.100".to_string(), 47842, None);
    assert_eq!(conn.display_name(), "192.168.1.100:47842");

    let conn2 = RemoteServerConnection::new("my-desktop.local".to_string(), 8080, None);
    assert_eq!(conn2.display_name(), "my-desktop.local:8080");
}

// ============================================================================
// Edge Case Tests
// ============================================================================

/// Test timeout calculation with zero duration
#[test]
fn test_timeout_calculation_zero_duration() {
    let live_timeout = calculate_timeout_ms(0, TranscriptionSource::LiveRecording);
    // Base 30s + 2 * 0 = 30_000ms
    assert_eq!(live_timeout, 30_000);

    let upload_timeout = calculate_timeout_ms(0, TranscriptionSource::Upload);
    // Base 60s + 3 * 0 = 60_000ms
    assert_eq!(upload_timeout, 60_000);
}

/// Test timeout calculation with very small duration
#[test]
fn test_timeout_calculation_small_duration() {
    let timeout = calculate_timeout_ms(100, TranscriptionSource::LiveRecording);
    // 30_000 + 2 * 100 = 30_200ms
    assert_eq!(timeout, 30_200);
}

/// Test timeout calculation with maximum u64 value for upload (no cap)
#[test]
fn test_timeout_calculation_upload_large_duration() {
    // Test with 24 hours of audio
    let duration_24h = 24 * 60 * 60 * 1000; // 86_400_000ms
    let timeout = calculate_timeout_ms(duration_24h, TranscriptionSource::Upload);
    // 60_000 + 3 * 86_400_000 = 259_260_000ms (~72 hours)
    assert_eq!(timeout, 259_260_000);
}

/// Test URL generation with IPv6 address
#[test]
fn test_remote_server_urls_ipv6() {
    let conn = RemoteServerConnection::new("::1".to_string(), 47842, None);
    assert_eq!(conn.status_url(), "http://::1:47842/api/v1/status");
    assert_eq!(conn.transcribe_url(), "http://::1:47842/api/v1/transcribe");
    assert_eq!(conn.display_name(), "::1:47842");
}

/// Test URL generation with full IPv6 address
#[test]
fn test_remote_server_urls_ipv6_full() {
    let conn =
        RemoteServerConnection::new("2001:db8:85a3::8a2e:370:7334".to_string(), 8080, None);
    assert_eq!(
        conn.status_url(),
        "http://2001:db8:85a3::8a2e:370:7334:8080/api/v1/status"
    );
}

/// Test connection with port boundary values
#[test]
fn test_remote_server_connection_port_boundaries() {
    // Minimum valid port (excluding 0 which is typically invalid)
    let conn_min = RemoteServerConnection::new("localhost".to_string(), 1, None);
    assert_eq!(conn_min.port, 1);
    assert_eq!(conn_min.status_url(), "http://localhost:1/api/v1/status");

    // Maximum valid port
    let conn_max = RemoteServerConnection::new("localhost".to_string(), 65535, None);
    assert_eq!(conn_max.port, 65535);
    assert_eq!(
        conn_max.status_url(),
        "http://localhost:65535/api/v1/status"
    );
}

/// Test connection with empty host (edge case)
#[test]
fn test_remote_server_connection_empty_host() {
    let conn = RemoteServerConnection::new(String::new(), 47842, None);
    assert_eq!(conn.host, "");
    assert_eq!(conn.status_url(), "http://:47842/api/v1/status");
    assert_eq!(conn.display_name(), ":47842");
}

/// Test connection with empty password (Some("") vs None)
#[test]
fn test_remote_server_connection_empty_password() {
    let conn_empty = RemoteServerConnection::new(
        "localhost".to_string(),
        47842,
        Some(String::new()),
    );
    assert_eq!(conn_empty.password, Some(String::new()));

    let conn_none = RemoteServerConnection::new("localhost".to_string(), 47842, None);
    assert!(conn_none.password.is_none());

    // They should not be equal
    assert_ne!(conn_empty.password, conn_none.password);
}

/// Test PartialEq implementation for RemoteServerConnection
#[test]
fn test_remote_server_connection_equality() {
    let conn1 = RemoteServerConnection::new(
        "192.168.1.100".to_string(),
        47842,
        Some("password".to_string()),
    );
    let conn2 = RemoteServerConnection::new(
        "192.168.1.100".to_string(),
        47842,
        Some("password".to_string()),
    );
    let conn3 = RemoteServerConnection::new(
        "192.168.1.100".to_string(),
        47842,
        Some("different".to_string()),
    );

    assert_eq!(conn1, conn2);
    assert_ne!(conn1, conn3);
}

/// Test Clone implementation for RemoteServerConnection
#[test]
fn test_remote_server_connection_clone() {
    let original = RemoteServerConnection::new(
        "192.168.1.100".to_string(),
        47842,
        Some("secret".to_string()),
    );
    let cloned = original.clone();

    assert_eq!(original, cloned);
    assert_eq!(original.host, cloned.host);
    assert_eq!(original.port, cloned.port);
    assert_eq!(original.password, cloned.password);
}

/// Test Debug implementation for RemoteServerConnection
#[test]
fn test_remote_server_connection_debug() {
    let conn = RemoteServerConnection::new(
        "localhost".to_string(),
        47842,
        Some("password".to_string()),
    );
    let debug_str = format!("{:?}", conn);

    assert!(debug_str.contains("RemoteServerConnection"));
    assert!(debug_str.contains("localhost"));
    assert!(debug_str.contains("47842"));
}

/// Test TranscriptionSource equality
#[test]
fn test_transcription_source_equality() {
    assert_eq!(TranscriptionSource::LiveRecording, TranscriptionSource::LiveRecording);
    assert_eq!(TranscriptionSource::Upload, TranscriptionSource::Upload);
    assert_ne!(TranscriptionSource::LiveRecording, TranscriptionSource::Upload);
}

/// Test TranscriptionSource clone
#[test]
fn test_transcription_source_clone() {
    let live = TranscriptionSource::LiveRecording;
    let upload = TranscriptionSource::Upload;

    assert_eq!(live.clone(), TranscriptionSource::LiveRecording);
    assert_eq!(upload.clone(), TranscriptionSource::Upload);
}

/// Test TranscriptionSource debug output
#[test]
fn test_transcription_source_debug() {
    let live_debug = format!("{:?}", TranscriptionSource::LiveRecording);
    let upload_debug = format!("{:?}", TranscriptionSource::Upload);

    assert_eq!(live_debug, "LiveRecording");
    assert_eq!(upload_debug, "Upload");
}

/// Test TranscriptionRequest with empty audio data
#[test]
fn test_transcription_request_empty_audio() {
    let request = TranscriptionRequest::new(vec![], TranscriptionSource::LiveRecording);
    assert!(request.audio_data.is_empty());
    assert_eq!(request.source, TranscriptionSource::LiveRecording);
}

/// Test TranscriptionRequest with large audio data
#[test]
fn test_transcription_request_large_audio() {
    // Simulate ~1MB of audio data
    let large_audio = vec![0u8; 1_000_000];
    let request = TranscriptionRequest::new(large_audio.clone(), TranscriptionSource::Upload);

    assert_eq!(request.audio_data.len(), 1_000_000);
    assert_eq!(request.source, TranscriptionSource::Upload);
}

/// Test connection deserialization from JSON
#[test]
fn test_remote_server_connection_deserialization() {
    let json = r#"{"host":"192.168.1.50","port":9000,"password":"test123"}"#;
    let conn: RemoteServerConnection = serde_json::from_str(json).unwrap();

    assert_eq!(conn.host, "192.168.1.50");
    assert_eq!(conn.port, 9000);
    assert_eq!(conn.password, Some("test123".to_string()));
}

/// Test connection deserialization without password
#[test]
fn test_remote_server_connection_deserialization_no_password() {
    let json = r#"{"host":"localhost","port":47842,"password":null}"#;
    let conn: RemoteServerConnection = serde_json::from_str(json).unwrap();

    assert_eq!(conn.host, "localhost");
    assert_eq!(conn.port, 47842);
    assert!(conn.password.is_none());
}

/// Test TranscriptionSource copy semantics
#[test]
fn test_transcription_source_copy() {
    let source = TranscriptionSource::LiveRecording;
    let copied = source; // Copy, not move

    // Both should be usable (Copy trait)
    assert_eq!(source, TranscriptionSource::LiveRecording);
    assert_eq!(copied, TranscriptionSource::LiveRecording);
}

/// Test timeout calculation consistency
#[test]
fn test_timeout_calculation_consistency() {
    // Same input should always produce same output
    let duration = 45_000;
    let t1 = calculate_timeout_ms(duration, TranscriptionSource::LiveRecording);
    let t2 = calculate_timeout_ms(duration, TranscriptionSource::LiveRecording);
    let t3 = calculate_timeout_ms(duration, TranscriptionSource::LiveRecording);

    assert_eq!(t1, t2);
    assert_eq!(t2, t3);
}

/// Test all URL methods return valid HTTP URLs
#[test]
fn test_url_format_validity() {
    let conn = RemoteServerConnection::new("test.example.com".to_string(), 8443, None);

    let status_url = conn.status_url();
    let transcribe_url = conn.transcribe_url();

    // All URLs should start with http://
    assert!(status_url.starts_with("http://"));
    assert!(transcribe_url.starts_with("http://"));

    // All URLs should contain the host
    assert!(status_url.contains("test.example.com"));
    assert!(transcribe_url.contains("test.example.com"));

    // All URLs should contain the port
    assert!(status_url.contains("8443"));
    assert!(transcribe_url.contains("8443"));
}
