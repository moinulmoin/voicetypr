//! Remote transcription client
//!
//! HTTP client for connecting to other VoiceTypr instances
//! to use their transcription capabilities.

use serde::{Deserialize, Serialize};

/// Source of audio for transcription (affects timeout calculation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptionSource {
    /// Live recording from microphone
    LiveRecording,
    /// Uploaded audio/video file
    Upload,
}

/// Connection configuration for a remote VoiceTypr server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteServerConnection {
    /// Hostname or IP address
    pub host: String,
    /// Port number
    pub port: u16,
    /// Optional password for authentication
    pub password: Option<String>,
}

impl RemoteServerConnection {
    /// Create a new remote server connection
    pub fn new(host: String, port: u16, password: Option<String>) -> Self {
        Self {
            host,
            port,
            password,
        }
    }

    /// Get the URL for the status endpoint
    pub fn status_url(&self) -> String {
        format!("http://{}:{}/api/v1/status", self.host, self.port)
    }

    /// Get the URL for the transcribe endpoint
    pub fn transcribe_url(&self) -> String {
        format!("http://{}:{}/api/v1/transcribe", self.host, self.port)
    }

    /// Get a display name for this connection
    pub fn display_name(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Request to transcribe audio
#[derive(Debug, Clone)]
pub struct TranscriptionRequest {
    /// Raw audio data (WAV format)
    pub audio_data: Vec<u8>,
    /// Source of the audio (affects timeout)
    pub source: TranscriptionSource,
}

impl TranscriptionRequest {
    /// Create a new transcription request
    pub fn new(audio_data: Vec<u8>, source: TranscriptionSource) -> Self {
        Self { audio_data, source }
    }
}

/// Calculate timeout in milliseconds based on audio duration and source
///
/// For live recordings:
/// - Base: 30 seconds
/// - Plus: 2x the audio duration
/// - Maximum: 2 minutes (120 seconds)
///
/// For uploads:
/// - Base: 60 seconds
/// - Plus: 3x the audio duration
/// - No maximum (long files need long timeouts)
pub fn calculate_timeout_ms(audio_duration_ms: u64, source: TranscriptionSource) -> u64 {
    match source {
        TranscriptionSource::LiveRecording => {
            // Base 30s + 2x duration, capped at 2 minutes
            let timeout = 30_000 + (audio_duration_ms * 2);
            timeout.min(120_000)
        }
        TranscriptionSource::Upload => {
            // Base 60s + 3x duration, no cap
            60_000 + (audio_duration_ms * 3)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_urls() {
        let conn = RemoteServerConnection::new("localhost".to_string(), 47842, None);
        assert!(conn.status_url().contains("/api/v1/status"));
        assert!(conn.transcribe_url().contains("/api/v1/transcribe"));
    }
}
