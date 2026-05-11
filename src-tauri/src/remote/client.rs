//! Remote transcription client
//!
//! HTTP client for connecting to other VoiceTypr instances
//! to use their transcription capabilities.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

use reqwest::StatusCode;

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
        format!("{}/api/v1/status", format_base_url(&self.host, self.port))
    }

    /// Get the URL for the transcribe endpoint
    pub fn transcribe_url(&self) -> String {
        format!(
            "{}/api/v1/transcribe",
            format_base_url(&self.host, self.port)
        )
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteEndpoint {
    Status,
    Transcribe,
}

impl fmt::Display for RemoteEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemoteEndpoint::Status => f.write_str("status"),
            RemoteEndpoint::Transcribe => f.write_str("transcription"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteClientError {
    AuthFailed {
        endpoint: RemoteEndpoint,
        body: Option<String>,
    },
    Timeout {
        endpoint: RemoteEndpoint,
        timeout_ms: u64,
        detail: String,
    },
    ConnectFailed {
        endpoint: RemoteEndpoint,
        detail: String,
    },
    HttpStatus {
        endpoint: RemoteEndpoint,
        status: StatusCode,
        body: Option<String>,
    },
    ResponseDecode {
        endpoint: RemoteEndpoint,
        detail: String,
        body: Option<String>,
    },
    ResponseSchema {
        endpoint: RemoteEndpoint,
        detail: String,
        body: Option<String>,
    },
    RequestBuild {
        endpoint: RemoteEndpoint,
        detail: String,
    },
    JoinFailed {
        endpoint: RemoteEndpoint,
        detail: String,
    },
}

impl fmt::Display for RemoteClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthFailed { .. } => f.write_str("Authentication failed"),
            Self::Timeout { timeout_ms, detail, .. } => {
                if detail.trim().is_empty() {
                    write!(f, "Request timed out after {}ms", timeout_ms)
                } else {
                    write!(f, "Request timed out after {}ms: {}", timeout_ms, detail)
                }
            }
            Self::ConnectFailed { detail, .. } => write!(f, "Failed to connect: {}", detail),
            Self::HttpStatus { status, .. } => write!(f, "Server error: {}", status),
            Self::ResponseDecode { detail, .. } => write!(f, "Failed to parse response: {}", detail),
            Self::ResponseSchema { detail, .. } => write!(f, "Invalid response: {}", detail),
            Self::RequestBuild { detail, .. } => {
                write!(f, "Failed to create HTTP client: {}", detail)
            }
            Self::JoinFailed { detail, .. } => write!(f, "Task join error: {}", detail),
        }
    }
}

impl std::error::Error for RemoteClientError {}

impl RemoteClientError {
    pub fn endpoint(&self) -> RemoteEndpoint {
        match self {
            Self::AuthFailed { endpoint, .. }
            | Self::Timeout { endpoint, .. }
            | Self::ConnectFailed { endpoint, .. }
            | Self::HttpStatus { endpoint, .. }
            | Self::ResponseDecode { endpoint, .. }
            | Self::ResponseSchema { endpoint, .. }
            | Self::RequestBuild { endpoint, .. }
            | Self::JoinFailed { endpoint, .. } => *endpoint,
        }
    }

    pub fn is_auth_failure(&self) -> bool {
        matches!(self, Self::AuthFailed { .. })
    }

    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout { .. })
    }

    pub fn status_code(&self) -> Option<StatusCode> {
        match self {
            Self::HttpStatus { status, .. } => Some(*status),
            _ => None,
        }
    }

    pub fn server_error_body(&self) -> Option<&str> {
        match self {
            Self::AuthFailed { body, .. }
            | Self::HttpStatus { body, .. }
            | Self::ResponseDecode { body, .. }
            | Self::ResponseSchema { body, .. } => body.as_deref(),
            _ => None,
        }
    }
}

fn format_base_url(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("http://[{}]:{}", host, port)
    } else {
        format!("http://{}:{}", host, port)
    }
}

fn lossy_body(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn classify_reqwest_error(
    endpoint: RemoteEndpoint,
    error: reqwest::Error,
    timeout_ms: u64,
) -> RemoteClientError {
    if error.is_timeout() {
        RemoteClientError::Timeout {
            endpoint,
            timeout_ms,
            detail: error.to_string(),
        }
    } else {
        RemoteClientError::ConnectFailed {
            endpoint,
            detail: error.to_string(),
        }
    }
}

fn parse_json_value<T>(
    endpoint: RemoteEndpoint,
    body: &[u8],
) -> Result<T, RemoteClientError>
where
    T: serde::de::DeserializeOwned,
{
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|e| {
        RemoteClientError::ResponseDecode {
            endpoint,
            detail: e.to_string(),
            body: Some(lossy_body(body)),
        }
    })?;

    serde_json::from_value(value).map_err(|e| RemoteClientError::ResponseSchema {
        endpoint,
        detail: e.to_string(),
        body: Some(lossy_body(body)),
    })
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
            let timeout = 30_000u64.saturating_add(audio_duration_ms.saturating_mul(2));
            timeout.min(120_000)
        }
        TranscriptionSource::Upload => {
            // Base 60s + 3x duration, no cap
            60_000u64.saturating_add(audio_duration_ms.saturating_mul(3))
        }
    }
}

pub fn timeout_ms_for_wav_file(audio_path: &str, source: TranscriptionSource) -> u64 {
    let base_timeout_ms = calculate_timeout_ms(0, source);

    let reader = match hound::WavReader::open(audio_path) {
        Ok(reader) => reader,
        Err(e) => {
            log::warn!(
                "[Remote Client] Could not inspect audio duration for '{}': {}",
                audio_path,
                e
            );
            return base_timeout_ms;
        }
    };

    let spec = reader.spec();
    if spec.channels == 0 || spec.sample_rate == 0 {
        log::warn!(
            "[Remote Client] Invalid WAV metadata for '{}'; using base timeout",
            audio_path
        );
        return base_timeout_ms;
    }

    let frames = reader.duration() as u64 / spec.channels as u64;
    let duration_ms = frames.saturating_mul(1000) / spec.sample_rate as u64;
    calculate_timeout_ms(duration_ms, source)
}

/// Test connection to a remote server using the Intel-Mac-safe blocking probe path.
pub async fn test_connection(
    connection: &RemoteServerConnection,
) -> Result<crate::remote::server::StatusResponse, RemoteClientError> {
    const STATUS_TIMEOUT_MS: u64 = 10_000;

    let connection = connection.clone();
    let endpoint = RemoteEndpoint::Status;
    let url = connection.status_url();
    let password = connection.password.clone();

    let status = tokio::task::spawn_blocking(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(STATUS_TIMEOUT_MS))
            .build()
            .map_err(|e| RemoteClientError::RequestBuild {
                endpoint,
                detail: e.to_string(),
            })?;

        let mut request = client.get(&url);
        if let Some(pwd) = password.as_ref() {
            request = request.header("X-VoiceTypr-Key", pwd);
        }

        let response = request.send().map_err(|e| classify_reqwest_error(endpoint, e, STATUS_TIMEOUT_MS))?;
        let status = response.status();
        let body = response.bytes().map_err(|e| classify_reqwest_error(endpoint, e, STATUS_TIMEOUT_MS))?;

        if status == StatusCode::UNAUTHORIZED {
            return Err(RemoteClientError::AuthFailed {
                endpoint,
                body: Some(lossy_body(body.as_ref())),
            });
        }

        if !status.is_success() {
            return Err(RemoteClientError::HttpStatus {
                endpoint,
                status,
                body: Some(lossy_body(body.as_ref())),
            });
        }

        parse_json_value(endpoint, body.as_ref())
    })
    .await
    .map_err(|e| RemoteClientError::JoinFailed {
        endpoint,
        detail: e.to_string(),
    })??;

    Ok(status)
}

/// Submit a remote transcription request with a caller-provided timeout.
pub async fn transcribe_audio(
    connection: &RemoteServerConnection,
    request: TranscriptionRequest,
    timeout_ms: u64,
) -> Result<crate::remote::server::TranscribeResponse, RemoteClientError> {
    let endpoint = RemoteEndpoint::Transcribe;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .map_err(|e| RemoteClientError::RequestBuild {
            endpoint,
            detail: e.to_string(),
        })?;

    let mut request_builder = client
        .post(connection.transcribe_url())
        .header("Content-Type", "audio/wav")
        .body(request.audio_data);

    if let Some(pwd) = connection.password.as_ref() {
        request_builder = request_builder.header("X-VoiceTypr-Key", pwd);
    }

    let response = request_builder
        .send()
        .await
        .map_err(|e| classify_reqwest_error(endpoint, e, timeout_ms))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|e| classify_reqwest_error(endpoint, e, timeout_ms))?;

    if status == StatusCode::UNAUTHORIZED {
        return Err(RemoteClientError::AuthFailed {
            endpoint,
            body: Some(lossy_body(body.as_ref())),
        });
    }

    if !status.is_success() {
        return Err(RemoteClientError::HttpStatus {
            endpoint,
            status,
            body: Some(lossy_body(body.as_ref())),
        });
    }

    parse_json_value(endpoint, body.as_ref())
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
