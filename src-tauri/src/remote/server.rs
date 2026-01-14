//! Remote transcription server
//!
//! HTTP server that allows other VoiceTypr instances to use this machine's
//! transcription capabilities.

use serde::{Deserialize, Serialize};

/// Response from the /api/v1/status endpoint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusResponse {
    pub status: String,
    pub version: String,
    pub model: String,
    pub name: String,
}

/// Response from the /api/v1/transcribe endpoint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscribeResponse {
    pub text: String,
    pub duration_ms: u64,
    pub model: String,
}

/// Error response for API endpoints
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorResponse {
    pub error: String,
}

/// Configuration for the remote transcription server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteServerConfig {
    /// Port to listen on (default: 47842)
    pub port: u16,
    /// Optional password for authentication
    pub password: Option<String>,
    /// Whether sharing is enabled
    pub enabled: bool,
}

impl Default for RemoteServerConfig {
    fn default() -> Self {
        Self {
            port: 47842,
            password: None,
            enabled: false,
        }
    }
}

impl RemoteServerConfig {
    /// Validate a password against the configured password
    ///
    /// Returns true if:
    /// - No password is required (self.password is None)
    /// - The provided password matches the configured password
    pub fn validate_password(&self, provided: Option<&str>) -> bool {
        match &self.password {
            None => true, // No password required
            Some(required) => {
                // Password required - check if provided matches
                provided.map(|p| p == required).unwrap_or(false)
            }
        }
    }
}

/// Current status of the remote server
#[derive(Debug, Clone, PartialEq)]
pub enum ServerStatus {
    /// Server is not running
    Idle,
    /// Server is running and accepting connections
    Running {
        port: u16,
        connections: usize,
    },
}

impl ServerStatus {
    /// Check if the server is currently running
    pub fn is_running(&self) -> bool {
        matches!(self, ServerStatus::Running { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RemoteServerConfig::default();
        assert_eq!(config.port, 47842);
        assert!(config.password.is_none());
        assert!(!config.enabled);
    }
}
