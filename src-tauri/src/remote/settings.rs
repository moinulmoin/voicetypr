//! Remote transcription settings storage
//!
//! Manages storage and retrieval of remote server configurations
//! and saved connections.

use serde::{Deserialize, Serialize};
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedConnection {
    /// Unique identifier for this connection
    pub id: String,
    /// Hostname or IP address
    pub host: String,
    /// Port number
    pub port: u16,
    /// Optional password for authentication
    pub password: Option<String>,
    /// Optional friendly name
    pub name: Option<String>,
    /// Timestamp when this connection was added (unix timestamp ms)
    pub created_at: u64,
    /// Model being served by this server (cached from last status check)
    #[serde(default)]
    pub model: Option<String>,
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

impl Default for RemoteSettings {
    fn default() -> Self {
        Self {
            server_config: RemoteServerConfig::default(),
            saved_connections: Vec::new(),
            active_connection_id: None,
            sharing_was_active: false,
        }
    }
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
            status: ConnectionStatus::Unknown,
            last_checked: 0,
        };

        self.saved_connections.push(saved.clone());
        saved
    }

    /// Update the status of a connection after a status check
    pub fn update_connection_status(
        &mut self,
        id: &str,
        status: ConnectionStatus,
        model: Option<String>,
    ) {
        if let Some(conn) = self.saved_connections.iter_mut().find(|c| c.id == id) {
            conn.status = status;
            conn.last_checked = current_timestamp();
            if model.is_some() {
                conn.model = model;
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
}
