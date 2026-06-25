//! Remote transcription settings storage
//!
//! Manages storage and retrieval of remote server configurations
//! and saved connections.

use serde::{ser::SerializeStruct, Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use super::server::RemoteServerConfig;

/// Connection status from last check
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ConnectionStatus {
    #[default]
    Unknown,
    Online,
    Offline,
    AuthFailed,
    /// This server is actually this machine (can't use self)
    SelfConnection,
}

/// A saved connection with metadata
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SavedConnection {
    /// Unique identifier for this connection
    pub id: String,
    /// Hostname or IP address
    pub host: String,
    /// Port number
    pub port: u16,
    /// Optional password for authentication
    #[serde(default)]
    pub password: Option<String>,
    /// Optional friendly name
    pub name: Option<String>,
    /// Timestamp when this connection was added (unix timestamp ms)
    pub created_at: u64,
    /// Model being served by this server (cached from last status check)
    #[serde(default)]
    pub model: Option<String>,
    /// Capabilities advertised by this server (cached from last successful status check)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<crate::remote::server::RemoteCapabilities>,
    /// Cached connection status from last check
    #[serde(default)]
    pub status: ConnectionStatus,
    /// Timestamp of last status check (unix timestamp ms)
    #[serde(default)]
    pub last_checked: u64,
}

impl SavedConnection {
    /// Get a display name for this connection
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("{}:{}", self.host, self.port))
    }
}

/// All remote transcription settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RemoteSettings {
    /// Server configuration (for sharing this machine's transcription)
    pub server_config: RemoteServerConfig,
    /// Saved remote server connections
    pub saved_connections: Vec<SavedConnection>,
    /// Currently active connection ID (if using remote transcription)
    pub active_connection_id: Option<String>,
    /// Flag indicating sharing was auto-disabled when switching to remote model
    /// Used to auto-restore sharing when returning to a local model
    #[serde(default)]
    pub sharing_was_active: bool,
}

impl RemoteSettings {
    /// Add a new connection and return it
    pub fn add_connection(
        &mut self,
        host: String,
        port: u16,
        password: Option<String>,
        name: Option<String>,
        model: Option<String>,
    ) -> SavedConnection {
        let id = generate_id();
        let created_at = current_timestamp();

        let saved = SavedConnection {
            id,
            host,
            port,
            password,
            name,
            created_at,
            model,
            capabilities: None,
            status: ConnectionStatus::Unknown,
            last_checked: 0,
        };

        self.saved_connections.push(saved.clone());
        saved
    }

    /// Update a connection after a status probe.
    ///
    /// `model` overwrites only when `Some`. `capabilities` uses an explicit
    /// two-level option: outer `None` leaves the cached capabilities untouched
    /// (use on error paths), while `Some(value)` is authoritative — it sets the
    /// cache to `value`, INCLUDING clearing it when `value` is `None`. A
    /// successful probe from a host that no longer advertises context support
    /// must clear stale capabilities so the client fails safe.
    pub fn update_connection_status(
        &mut self,
        id: &str,
        status: ConnectionStatus,
        model: Option<String>,
        capabilities: Option<Option<crate::remote::server::RemoteCapabilities>>,
    ) {
        if let Some(conn) = self.saved_connections.iter_mut().find(|c| c.id == id) {
            conn.status = status;
            conn.last_checked = current_timestamp();
            if model.is_some() {
                conn.model = model;
            }
            if let Some(new_capabilities) = capabilities {
                conn.capabilities = new_capabilities;
            }
        }
    }

    /// Remove a connection by ID
    pub fn remove_connection(&mut self, id: &str) -> Result<(), String> {
        let initial_len = self.saved_connections.len();
        self.saved_connections.retain(|c| c.id != id);

        if self.saved_connections.len() == initial_len {
            return Err(format!("Connection '{}' not found", id));
        }

        // Clear active connection if it was the removed one
        if self.active_connection_id.as_deref() == Some(id) {
            self.active_connection_id = None;
        }

        Ok(())
    }

    /// Get a connection by ID
    pub fn get_connection(&self, id: &str) -> Option<&SavedConnection> {
        self.saved_connections.iter().find(|c| c.id == id)
    }

    /// Set the active connection ID
    pub fn set_active_connection(&mut self, id: Option<String>) -> Result<(), String> {
        // Validate the ID exists if provided
        if let Some(ref conn_id) = id {
            if self.get_connection(conn_id).is_none() {
                return Err(format!("Connection '{}' not found", conn_id));
            }
        }
        self.active_connection_id = id;
        Ok(())
    }

    /// Get the currently active connection
    pub fn get_active_connection(&self) -> Option<&SavedConnection> {
        self.active_connection_id
            .as_ref()
            .and_then(|id| self.get_connection(id))
    }

    /// List all saved connections
    pub fn list_connections(&self) -> Vec<SavedConnection> {
        self.saved_connections.clone()
    }
}

impl Serialize for SavedConnection {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct(
            "SavedConnection",
            if self.capabilities.is_some() { 10 } else { 9 },
        )?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("host", &self.host)?;
        state.serialize_field("port", &self.port)?;
        state.serialize_field(
            "has_password",
            &self.password.as_ref().is_some_and(|p| !p.is_empty()),
        )?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("created_at", &self.created_at)?;
        state.serialize_field("model", &self.model)?;
        state.serialize_field("status", &self.status)?;
        state.serialize_field("last_checked", &self.last_checked)?;
        if let Some(capabilities) = &self.capabilities {
            state.serialize_field("capabilities", capabilities)?;
        }
        state.end()
    }
}

/// Generate a unique ID for a connection
fn generate_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let timestamp = current_timestamp();
    let counter = COUNTER.fetch_add(1, Ordering::SeqCst);

    format!("conn_{}_{}", timestamp, counter)
}

/// Get current timestamp in milliseconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_unique_ids() {
        let id1 = generate_id();
        let id2 = generate_id();
        assert_ne!(id1, id2);
    }
    #[test]
    fn saved_connection_capabilities_round_trip_and_skip_when_none() {
        let capabilities = crate::remote::server::RemoteCapabilities {
            supports_initial_prompt: true,
            accepts_request_context: true,
            max_context_bytes: 900,
            acceleration: vec!["metal".to_string()],
            ..Default::default()
        };
        let saved = SavedConnection {
            id: "server-1".to_string(),
            host: "192.168.1.10".to_string(),
            port: 47842,
            password: Some("secret".to_string()),
            name: Some("Office".to_string()),
            created_at: 42,
            model: Some("whisper".to_string()),
            capabilities: Some(capabilities.clone()),
            status: ConnectionStatus::Online,
            last_checked: 99,
        };

        let serialized = serde_json::to_value(&saved).unwrap();
        assert_eq!(
            serialized.get("capabilities").cloned(),
            Some(serde_json::to_value(&capabilities).unwrap())
        );
        assert_eq!(
            serialized.get("has_password").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert!(serialized.get("password").is_none());

        let round_tripped: SavedConnection = serde_json::from_value(serialized).unwrap();
        assert_eq!(round_tripped.capabilities, Some(capabilities));
        assert!(round_tripped.password.is_none());

        let without_capabilities = SavedConnection {
            capabilities: None,
            ..saved
        };
        let serialized_without = serde_json::to_value(&without_capabilities).unwrap();
        assert!(serialized_without.get("capabilities").is_none());
    }

    #[test]
    fn update_connection_status_only_overwrites_capabilities_when_present() {
        let mut settings = RemoteSettings::default();
        let conn = settings.add_connection(
            "192.168.1.10".to_string(),
            47842,
            None,
            Some("Office".to_string()),
            None,
        );
        let capabilities = crate::remote::server::RemoteCapabilities {
            supports_initial_prompt: true,
            accepts_request_context: true,
            max_context_bytes: 900,
            ..Default::default()
        };

        settings.update_connection_status(
            &conn.id,
            ConnectionStatus::Online,
            Some("whisper".to_string()),
            Some(Some(capabilities.clone())),
        );
        assert_eq!(
            settings
                .get_connection(&conn.id)
                .and_then(|connection| connection.capabilities.clone()),
            Some(capabilities.clone())
        );

        settings.update_connection_status(&conn.id, ConnectionStatus::Offline, None, None);
        assert_eq!(
            settings
                .get_connection(&conn.id)
                .and_then(|connection| connection.capabilities.clone()),
            Some(capabilities)
        );

        // Authoritative clear: a successful probe with no advertised capabilities
        // wipes the stale cache so the client fails safe.
        settings.update_connection_status(&conn.id, ConnectionStatus::Online, None, Some(None));
        assert_eq!(
            settings
                .get_connection(&conn.id)
                .and_then(|connection| connection.capabilities.clone()),
            None
        );
    }
}
