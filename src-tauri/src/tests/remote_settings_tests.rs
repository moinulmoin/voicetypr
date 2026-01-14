//! Tests for remote transcription settings storage
//!
//! These tests verify that remote server configurations and connections
//! can be properly stored and retrieved.

use crate::remote::settings::RemoteSettings;

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

    let saved = settings.add_connection("192.168.1.100".to_string(), 47842, None, None);

    assert_eq!(settings.saved_connections.len(), 1);
    assert!(settings.get_connection(&saved.id).is_some());

    let retrieved = settings.get_connection(&saved.id).unwrap();
    assert_eq!(retrieved.host, "192.168.1.100");
    assert_eq!(retrieved.port, 47842);
}

/// Test adding a connection with a custom name
#[test]
fn test_add_connection_with_name() {
    let mut settings = RemoteSettings::default();

    let saved = settings.add_connection(
        "192.168.1.100".to_string(),
        47842,
        None,
        Some("Living Room Desktop".to_string()),
    );

    assert_eq!(saved.name, Some("Living Room Desktop".to_string()));
    assert_eq!(saved.display_name(), "Living Room Desktop");
}

/// Test removing a connection
#[test]
fn test_remove_connection() {
    let mut settings = RemoteSettings::default();

    let saved = settings.add_connection("192.168.1.100".to_string(), 47842, None, None);

    assert_eq!(settings.saved_connections.len(), 1);

    let result = settings.remove_connection(&saved.id);
    assert!(result.is_ok());
    assert!(settings.saved_connections.is_empty());
}

/// Test removing a non-existent connection
#[test]
fn test_remove_nonexistent_connection() {
    let mut settings = RemoteSettings::default();

    let result = settings.remove_connection("nonexistent-id");
    assert!(result.is_err());
}

/// Test removing the active connection clears active_connection_id
#[test]
fn test_remove_active_connection_clears_active() {
    let mut settings = RemoteSettings::default();

    let saved = settings.add_connection("192.168.1.100".to_string(), 47842, None, None);

    settings.set_active_connection(Some(saved.id.clone())).unwrap();
    assert_eq!(settings.active_connection_id, Some(saved.id.clone()));

    settings.remove_connection(&saved.id).unwrap();
    assert!(settings.active_connection_id.is_none());
}

/// Test setting active connection
#[test]
fn test_set_active_connection() {
    let mut settings = RemoteSettings::default();

    let saved1 = settings.add_connection("192.168.1.100".to_string(), 47842, None, None);
    let saved2 = settings.add_connection("192.168.1.101".to_string(), 47842, None, None);

    settings.set_active_connection(Some(saved1.id.clone())).unwrap();
    assert_eq!(settings.active_connection_id, Some(saved1.id.clone()));

    settings.set_active_connection(Some(saved2.id.clone())).unwrap();
    assert_eq!(settings.active_connection_id, Some(saved2.id));

    settings.set_active_connection(None).unwrap();
    assert!(settings.active_connection_id.is_none());
}

/// Test getting the active connection
#[test]
fn test_get_active_connection() {
    let mut settings = RemoteSettings::default();

    // No active connection
    assert!(settings.get_active_connection().is_none());

    let saved = settings.add_connection("192.168.1.100".to_string(), 47842, None, None);

    settings.set_active_connection(Some(saved.id.clone())).unwrap();

    let active = settings.get_active_connection();
    assert!(active.is_some());
    assert_eq!(active.unwrap().host, "192.168.1.100");
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
    let saved1 = settings.add_connection(
        "192.168.1.100".to_string(),
        47842,
        None,
        Some("Desktop".to_string()),
    );
    let _saved2 = settings.add_connection(
        "192.168.1.101".to_string(),
        47842,
        Some("pass".to_string()),
        None,
    );

    settings.set_active_connection(Some(saved1.id.clone())).unwrap();

    // Serialize and deserialize
    let json = serde_json::to_string(&settings).unwrap();
    let restored: RemoteSettings = serde_json::from_str(&json).unwrap();

    // Verify
    assert!(restored.server_config.enabled);
    assert_eq!(restored.server_config.port, 8080);
    assert_eq!(restored.server_config.password, Some("secret123".to_string()));
    assert_eq!(restored.saved_connections.len(), 2);
    assert_eq!(restored.active_connection_id, Some(saved1.id));
}

/// Test SavedConnection includes timestamp
#[test]
fn test_saved_connection_has_timestamp() {
    let mut settings = RemoteSettings::default();

    let saved = settings.add_connection("192.168.1.100".to_string(), 47842, None, None);

    // Timestamp should be set to a reasonable recent value
    assert!(saved.created_at > 0);
}

/// Test listing all connections
#[test]
fn test_list_connections() {
    let mut settings = RemoteSettings::default();

    settings.add_connection("192.168.1.100".to_string(), 47842, None, Some("A".to_string()));
    settings.add_connection("192.168.1.101".to_string(), 47842, None, Some("B".to_string()));
    settings.add_connection("192.168.1.102".to_string(), 47842, None, Some("C".to_string()));

    let all = settings.list_connections();
    assert_eq!(all.len(), 3);
}

/// Test connection ID generation is unique
#[test]
fn test_connection_ids_are_unique() {
    let mut settings = RemoteSettings::default();

    let saved1 = settings.add_connection("192.168.1.100".to_string(), 47842, None, None);
    let saved2 = settings.add_connection("192.168.1.100".to_string(), 47842, None, None);

    assert_ne!(saved1.id, saved2.id);
}

/// Test SavedConnection display_name
#[test]
fn test_saved_connection_display_name() {
    let mut settings = RemoteSettings::default();

    // With custom name
    let saved1 = settings.add_connection(
        "192.168.1.100".to_string(),
        47842,
        None,
        Some("My Desktop".to_string()),
    );
    assert_eq!(saved1.display_name(), "My Desktop");

    // Without custom name - should use host:port
    let saved2 = settings.add_connection("192.168.1.101".to_string(), 8080, None, None);
    assert_eq!(saved2.display_name(), "192.168.1.101:8080");
}

/// Test set_active_connection validates connection exists
#[test]
fn test_set_active_connection_validates() {
    let mut settings = RemoteSettings::default();

    // Try to set non-existent connection as active
    let result = settings.set_active_connection(Some("nonexistent".to_string()));
    assert!(result.is_err());

    // Setting None should always work
    let result = settings.set_active_connection(None);
    assert!(result.is_ok());
}
