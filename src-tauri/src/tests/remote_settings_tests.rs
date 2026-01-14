//! Tests for remote transcription settings storage
//!
//! These tests verify that remote server configurations and connections
//! can be properly stored and retrieved.

use crate::remote::client::RemoteServerConnection;
use crate::remote::server::RemoteServerConfig;
use crate::remote::settings::{RemoteSettings, SavedConnection};

/// Test creating default remote settings
#[test]
fn test_remote_settings_default() {
    let settings = RemoteSettings::default();

    assert!(!settings.server_config.enabled);
    assert_eq!(settings.server_config.port, 47842);
    assert!(settings.server_config.password.is_none());
    assert!(settings.saved_connections.is_empty());
    assert!(settings.active_connection_id.is_none());
}

/// Test adding a connection to settings
#[test]
fn test_add_connection() {
    let mut settings = RemoteSettings::default();

    let conn = RemoteServerConnection::new("192.168.1.100".to_string(), 47842, None);

    let id = settings.add_connection(conn.clone(), None);

    assert_eq!(settings.saved_connections.len(), 1);
    assert!(settings.get_connection(&id).is_some());

    let saved = settings.get_connection(&id).unwrap();
    assert_eq!(saved.connection.host, "192.168.1.100");
    assert_eq!(saved.connection.port, 47842);
}

/// Test adding a connection with a custom name
#[test]
fn test_add_connection_with_name() {
    let mut settings = RemoteSettings::default();

    let conn = RemoteServerConnection::new("192.168.1.100".to_string(), 47842, None);

    let id = settings.add_connection(conn, Some("Living Room Desktop".to_string()));

    let saved = settings.get_connection(&id).unwrap();
    assert_eq!(saved.name, Some("Living Room Desktop".to_string()));
}

/// Test removing a connection
#[test]
fn test_remove_connection() {
    let mut settings = RemoteSettings::default();

    let conn = RemoteServerConnection::new("192.168.1.100".to_string(), 47842, None);
    let id = settings.add_connection(conn, None);

    assert_eq!(settings.saved_connections.len(), 1);

    let removed = settings.remove_connection(&id);
    assert!(removed);
    assert!(settings.saved_connections.is_empty());
}

/// Test removing a non-existent connection
#[test]
fn test_remove_nonexistent_connection() {
    let mut settings = RemoteSettings::default();

    let removed = settings.remove_connection("nonexistent-id");
    assert!(!removed);
}

/// Test removing the active connection clears active_connection_id
#[test]
fn test_remove_active_connection_clears_active() {
    let mut settings = RemoteSettings::default();

    let conn = RemoteServerConnection::new("192.168.1.100".to_string(), 47842, None);
    let id = settings.add_connection(conn, None);

    settings.set_active_connection(Some(id.clone()));
    assert_eq!(settings.active_connection_id, Some(id.clone()));

    settings.remove_connection(&id);
    assert!(settings.active_connection_id.is_none());
}

/// Test setting active connection
#[test]
fn test_set_active_connection() {
    let mut settings = RemoteSettings::default();

    let conn1 = RemoteServerConnection::new("192.168.1.100".to_string(), 47842, None);
    let conn2 = RemoteServerConnection::new("192.168.1.101".to_string(), 47842, None);

    let id1 = settings.add_connection(conn1, None);
    let id2 = settings.add_connection(conn2, None);

    settings.set_active_connection(Some(id1.clone()));
    assert_eq!(settings.active_connection_id, Some(id1));

    settings.set_active_connection(Some(id2.clone()));
    assert_eq!(settings.active_connection_id, Some(id2));

    settings.set_active_connection(None);
    assert!(settings.active_connection_id.is_none());
}

/// Test getting the active connection
#[test]
fn test_get_active_connection() {
    let mut settings = RemoteSettings::default();

    // No active connection
    assert!(settings.get_active_connection().is_none());

    let conn = RemoteServerConnection::new("192.168.1.100".to_string(), 47842, None);
    let id = settings.add_connection(conn, None);

    settings.set_active_connection(Some(id.clone()));

    let active = settings.get_active_connection();
    assert!(active.is_some());
    assert_eq!(active.unwrap().connection.host, "192.168.1.100");
}

/// Test serializing and deserializing settings
#[test]
fn test_remote_settings_serialization() {
    let mut settings = RemoteSettings::default();

    // Configure server
    settings.server_config.enabled = true;
    settings.server_config.port = 8080;
    settings.server_config.password = Some("secret123".to_string());

    // Add connections
    let conn1 = RemoteServerConnection::new("192.168.1.100".to_string(), 47842, None);
    let conn2 = RemoteServerConnection::new(
        "192.168.1.101".to_string(),
        47842,
        Some("pass".to_string()),
    );

    let id1 = settings.add_connection(conn1, Some("Desktop".to_string()));
    let _id2 = settings.add_connection(conn2, None);

    settings.set_active_connection(Some(id1.clone()));

    // Serialize and deserialize
    let json = serde_json::to_string(&settings).unwrap();
    let restored: RemoteSettings = serde_json::from_str(&json).unwrap();

    // Verify
    assert!(restored.server_config.enabled);
    assert_eq!(restored.server_config.port, 8080);
    assert_eq!(restored.server_config.password, Some("secret123".to_string()));
    assert_eq!(restored.saved_connections.len(), 2);
    assert_eq!(restored.active_connection_id, Some(id1));
}

/// Test SavedConnection includes timestamp
#[test]
fn test_saved_connection_has_timestamp() {
    let mut settings = RemoteSettings::default();

    let conn = RemoteServerConnection::new("192.168.1.100".to_string(), 47842, None);
    let id = settings.add_connection(conn, None);

    let saved = settings.get_connection(&id).unwrap();

    // Timestamp should be set to a reasonable recent value
    assert!(saved.created_at > 0);
}

/// Test listing all connections
#[test]
fn test_list_connections() {
    let mut settings = RemoteSettings::default();

    let conn1 = RemoteServerConnection::new("192.168.1.100".to_string(), 47842, None);
    let conn2 = RemoteServerConnection::new("192.168.1.101".to_string(), 47842, None);
    let conn3 = RemoteServerConnection::new("192.168.1.102".to_string(), 47842, None);

    settings.add_connection(conn1, Some("A".to_string()));
    settings.add_connection(conn2, Some("B".to_string()));
    settings.add_connection(conn3, Some("C".to_string()));

    let all = settings.list_connections();
    assert_eq!(all.len(), 3);
}

/// Test connection ID generation is unique
#[test]
fn test_connection_ids_are_unique() {
    let mut settings = RemoteSettings::default();

    let conn1 = RemoteServerConnection::new("192.168.1.100".to_string(), 47842, None);
    let conn2 = RemoteServerConnection::new("192.168.1.100".to_string(), 47842, None);

    let id1 = settings.add_connection(conn1, None);
    let id2 = settings.add_connection(conn2, None);

    assert_ne!(id1, id2);
}
