//! Remote transcription server
//!
//! HTTP server that allows other VoiceTypr instances to use this machine's
//! transcription capabilities.

use serde::{ser::SerializeStruct, Deserialize, Serialize};

/// Response from the /api/v1/status endpoint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusResponse {
    pub status: String,
    pub version: String,
    pub model: String,
    pub name: String,
    /// Unique machine identifier to prevent self-connection
    pub machine_id: String,
}

/// Response from the /api/v1/transcribe endpoint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscribeResponse {
    pub text: String,
    pub duration_ms: u64,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_language: Option<String>,
}

/// Error response for API endpoints
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorResponse {
    pub error: String,
}

/// Configuration for the remote transcription server
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RemoteServerConfig {
    /// Port to listen on (default: 47842)
    pub port: u16,
    /// Optional password for authentication
    #[serde(default)]
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

impl Serialize for RemoteServerConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("RemoteServerConfig", 3)?;
        state.serialize_field("port", &self.port)?;
        state.serialize_field(
            "has_password",
            &self.password.as_ref().is_some_and(|p| !p.is_empty()),
        )?;
        state.serialize_field("enabled", &self.enabled)?;
        state.end()
    }
}

#[cfg(test)]
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
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub enum ServerStatus {
    /// Server is not running
    Idle,
    /// Server is running and accepting connections
    Running { port: u16, connections: usize },
}

#[cfg(test)]
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

    #[test]
    fn status_response_serializes_current_contract() {
        let response = StatusResponse {
            status: "ok".to_string(),
            version: "1.2.3".to_string(),
            model: "base.en".to_string(),
            name: "desk".to_string(),
            machine_id: "machine".to_string(),
        };

        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["status"], "ok");
        assert_eq!(value["version"], "1.2.3");
        assert_eq!(value["model"], "base.en");
        assert_eq!(value["name"], "desk");
        assert_eq!(value["machine_id"], "machine");
        assert_eq!(value.as_object().unwrap().len(), 5);
    }

    #[test]
    fn transcribe_response_omits_absent_transcript_language() {
        let value = serde_json::to_value(TranscribeResponse {
            text: "hello".to_string(),
            duration_ms: 42,
            model: "base.en".to_string(),
            transcript_language: None,
        })
        .unwrap();

        assert_eq!(value["text"], "hello");
        assert_eq!(value["duration_ms"], 42);
        assert_eq!(value["model"], "base.en");
        assert!(value.get("transcript_language").is_none());
    }
}
