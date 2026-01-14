//! Remote transcription settings storage
//!
//! Manages storage and retrieval of remote server configurations
//! and saved connections.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use super::client::RemoteServerConnection;
use super::server::RemoteServerConfig;

/// A saved connection with metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedConnection {
    /// Unique identifier for this connection
    pub id: String,
    /// The connection details
    pub connection: RemoteServerConnection,
    /// Optional friendly name
    pub name: Option<String>,
    /// Timestamp when this connection was added (unix timestamp ms)
    pub created_at: u64,
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
}

impl Default for RemoteSettings {
    fn default() -> Self {
        Self {
            server_config: RemoteServerConfig::default(),
            saved_connections: Vec::new(),
            active_connection_id: None,
        }
    }
}

impl RemoteSettings {
    /// Add a new connection and return its ID
    pub fn add_connection(
        &mut self,
        connection: RemoteServerConnection,
        name: Option<String>,
    ) -> String {
        let id = generate_id();
        let created_at = current_timestamp();

        let saved = SavedConnection {
            id: id.clone(),
            connection,
            name,
            created_at,
        };

        self.saved_connections.push(saved);
        id
    }

    /// Remove a connection by ID, returns true if removed
    pub fn remove_connection(&mut self, id: &str) -> bool {
        let initial_len = self.saved_connections.len();
        self.saved_connections.retain(|c| c.id != id);

        let removed = self.saved_connections.len() < initial_len;

        // Clear active connection if it was the removed one
        if removed && self.active_connection_id.as_deref() == Some(id) {
            self.active_connection_id = None;
        }

        removed
    }

    /// Get a connection by ID
    pub fn get_connection(&self, id: &str) -> Option<&SavedConnection> {
        self.saved_connections.iter().find(|c| c.id == id)
    }

    /// Set the active connection ID
    pub fn set_active_connection(&mut self, id: Option<String>) {
        self.active_connection_id = id;
    }

    /// Get the currently active connection
    pub fn get_active_connection(&self) -> Option<&SavedConnection> {
        self.active_connection_id
            .as_ref()
            .and_then(|id| self.get_connection(id))
    }

    /// List all saved connections
    pub fn list_connections(&self) -> &[SavedConnection] {
        &self.saved_connections
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
