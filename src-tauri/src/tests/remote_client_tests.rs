//! Tests for the remote transcription client module
//!
//! These tests verify the HTTP client functionality for connecting to
//! other VoiceTypr instances for remote transcription.

use crate::remote::client::{
    calculate_timeout_ms, RemoteServerConnection, TranscriptionRequest, TranscriptionSource,
};
use crate::remote::server::StatusResponse;

/// Test timeout calculation for live recordings (30 seconds)
#[test]
fn test_timeout_calculation_live_30s() {
    let timeout = calculate_timeout_ms(30_000, TranscriptionSource::LiveRecording);
    // Live recording: base 30s + 2x duration
    // 30_000 + 2 * 30_000 = 90_000ms = 90 seconds
    assert_eq!(timeout, 90_000);
}

/// Test timeout calculation for live recordings (60 seconds)
#[test]
fn test_timeout_calculation_live_60s() {
    let timeout = calculate_timeout_ms(60_000, TranscriptionSource::LiveRecording);
    // 30_000 + 2 * 60_000 = 150_000ms = 2.5 minutes
    assert_eq!(timeout, 150_000);
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
