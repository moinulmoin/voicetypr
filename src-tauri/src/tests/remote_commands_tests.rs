//! Tests for remote transcription Tauri commands
//!
//! Tests the command handlers in `commands/remote.rs`.
//! Since many commands require Tauri AppHandle and State which are difficult
//! to mock in unit tests, we focus on:
//! - Standalone functions that don't require app state
//! - Struct serialization/deserialization
//! - Helper function logic
//! - Constants and configurations

use crate::commands::remote::{
    get_firewall_status, get_local_ips, get_local_machine_id, open_firewall_settings,
    FirewallStatus, DEFAULT_PORT,
};
use crate::remote::settings::{RemoteSettings, SavedConnection};

// ============================================================================
// Constants Tests
// ============================================================================

#[test]
fn test_default_port_value() {
    assert_eq!(DEFAULT_PORT, 47842);
}

#[test]
fn test_default_port_is_valid() {
    // Port should be in the dynamic/private port range (49152-65535)
    // or registered port range (1024-49151)
    assert!(DEFAULT_PORT > 1023, "Port should not be a well-known port");
    assert!(DEFAULT_PORT < 65536, "Port should be valid TCP port");
}

// ============================================================================
// get_local_ips Tests
// ============================================================================

#[test]
fn test_get_local_ips_returns_ok() {
    let result = get_local_ips();
    assert!(result.is_ok(), "get_local_ips should return Ok");
}

#[test]
fn test_get_local_ips_returns_non_empty() {
    let result = get_local_ips().unwrap();
    assert!(
        !result.is_empty(),
        "Should return at least one IP or fallback message"
    );
}

#[test]
fn test_get_local_ips_no_loopback() {
    let result = get_local_ips().unwrap();
    for ip_str in &result {
        // Skip the "No network connection" fallback message
        if ip_str == "No network connection" {
            continue;
        }
        // IP strings are formatted as "IP (interface_name)"
        // Loopback addresses start with 127.
        assert!(
            !ip_str.starts_with("127."),
            "Should not include loopback addresses: {}",
            ip_str
        );
    }
}

#[test]
fn test_get_local_ips_no_link_local() {
    let result = get_local_ips().unwrap();
    for ip_str in &result {
        // Skip the "No network connection" fallback message
        if ip_str == "No network connection" {
            continue;
        }
        // Link-local addresses start with 169.254.
        assert!(
            !ip_str.starts_with("169.254."),
            "Should not include link-local addresses: {}",
            ip_str
        );
    }
}

#[test]
fn test_get_local_ips_format() {
    let result = get_local_ips().unwrap();
    for ip_str in &result {
        // Skip the "No network connection" fallback message
        if ip_str == "No network connection" {
            continue;
        }
        // IP strings should be formatted as "IP (interface_name)"
        assert!(
            ip_str.contains(" (") && ip_str.ends_with(")"),
            "IP should be formatted as 'IP (interface)': {}",
            ip_str
        );
    }
}

// ============================================================================
// FirewallStatus Tests
// ============================================================================

#[test]
fn test_firewall_status_struct_creation() {
    let status = FirewallStatus {
        firewall_enabled: true,
        app_allowed: false,
        may_be_blocked: true,
    };
    assert!(status.firewall_enabled);
    assert!(!status.app_allowed);
    assert!(status.may_be_blocked);
}

#[test]
fn test_firewall_status_serialization() {
    let status = FirewallStatus {
        firewall_enabled: true,
        app_allowed: true,
        may_be_blocked: false,
    };
    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("\"firewall_enabled\":true"));
    assert!(json.contains("\"app_allowed\":true"));
    assert!(json.contains("\"may_be_blocked\":false"));
}

#[test]
fn test_firewall_status_no_firewall_means_allowed() {
    // When firewall is disabled, app should be allowed and not blocked
    let status = FirewallStatus {
        firewall_enabled: false,
        app_allowed: true,
        may_be_blocked: false,
    };
    assert!(!status.firewall_enabled);
    assert!(status.app_allowed);
    assert!(!status.may_be_blocked);
}

#[test]
fn test_firewall_status_firewall_enabled_not_allowed() {
    // When firewall is enabled but app not allowed, may be blocked
    let status = FirewallStatus {
        firewall_enabled: true,
        app_allowed: false,
        may_be_blocked: true,
    };
    assert!(status.firewall_enabled);
    assert!(!status.app_allowed);
    assert!(status.may_be_blocked);
}

#[test]
fn test_firewall_status_firewall_enabled_allowed() {
    // When firewall is enabled and app allowed, not blocked
    let status = FirewallStatus {
        firewall_enabled: true,
        app_allowed: true,
        may_be_blocked: false,
    };
    assert!(status.firewall_enabled);
    assert!(status.app_allowed);
    assert!(!status.may_be_blocked);
}

// ============================================================================
// get_firewall_status Tests
// ============================================================================

#[test]
fn test_get_firewall_status_returns_valid_struct() {
    let status = get_firewall_status();
    // On non-macOS platforms, firewall is disabled by default
    #[cfg(not(target_os = "macos"))]
    {
        assert!(!status.firewall_enabled);
        assert!(status.app_allowed);
        assert!(!status.may_be_blocked);
    }
    // On macOS, just verify the struct is valid
    #[cfg(target_os = "macos")]
    {
        // If firewall is disabled, app should be allowed
        if !status.firewall_enabled {
            assert!(status.app_allowed);
            assert!(!status.may_be_blocked);
        }
        // If firewall is enabled and app not allowed, may be blocked
        if status.firewall_enabled && !status.app_allowed {
            assert!(status.may_be_blocked);
        }
    }
}

#[test]
fn test_get_firewall_status_consistency() {
    let status = get_firewall_status();
    // may_be_blocked should be inverse of app_allowed when firewall is enabled
    if status.firewall_enabled {
        assert_eq!(
            status.may_be_blocked, !status.app_allowed,
            "may_be_blocked should equal !app_allowed when firewall enabled"
        );
    } else {
        // When firewall disabled, should never be blocked
        assert!(
            !status.may_be_blocked,
            "should not be blocked when firewall disabled"
        );
    }
}

// ============================================================================
// open_firewall_settings Tests
// ============================================================================

#[test]
fn test_open_firewall_settings_returns_ok() {
    // This function spawns a process, so we just verify it returns Ok
    // We can't actually test that it opens the settings UI
    let result = open_firewall_settings();
    // On supported platforms (macOS, Windows), should return Ok
    // On unsupported platforms, should return Err
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        assert!(result.is_ok(), "Should succeed on macOS/Windows");
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        assert!(result.is_err(), "Should fail on unsupported platforms");
    }
}

// ============================================================================
// get_local_machine_id Tests
// ============================================================================

#[test]
fn test_get_local_machine_id_returns_result() {
    let result = get_local_machine_id();
    // Should return some result (Ok or Err depending on platform)
    match result {
        Ok(id) => {
            assert!(!id.is_empty(), "Machine ID should not be empty");
        }
        Err(_) => {
            // Some platforms may not support getting machine ID
        }
    }
}

#[test]
fn test_get_local_machine_id_consistent() {
    // If it succeeds, it should return the same ID each time
    let result1 = get_local_machine_id();
    let result2 = get_local_machine_id();

    if let (Ok(id1), Ok(id2)) = (result1, result2) {
        assert_eq!(id1, id2, "Machine ID should be consistent across calls");
    }
}

// ============================================================================
// RemoteSettings Integration Tests (using the settings module)
// ============================================================================

#[test]
fn test_remote_settings_default() {
    let settings = RemoteSettings::default();
    assert!(settings.saved_connections.is_empty());
    assert!(settings.active_connection_id.is_none());
    assert!(!settings.sharing_was_active);
}

#[test]
fn test_remote_settings_add_connection() {
    let mut settings = RemoteSettings::default();
    let conn = settings.add_connection(
        "192.168.1.100".to_string(),
        47842,
        Some("password123".to_string()),
        Some("Test Server".to_string()),
        Some("whisper-base".to_string()),
    );

    assert_eq!(conn.host, "192.168.1.100");
    assert_eq!(conn.port, 47842);
    assert_eq!(conn.password, Some("password123".to_string()));
    assert_eq!(conn.name, Some("Test Server".to_string()));
    assert_eq!(conn.model, Some("whisper-base".to_string()));
    assert!(!conn.id.is_empty());
    assert!(conn.created_at > 0);
}

#[test]
fn test_remote_settings_add_connection_no_optional_fields() {
    let mut settings = RemoteSettings::default();
    let conn = settings.add_connection("10.0.0.1".to_string(), 8080, None, None, None);

    assert_eq!(conn.host, "10.0.0.1");
    assert_eq!(conn.port, 8080);
    assert!(conn.password.is_none());
    assert!(conn.name.is_none());
    assert!(conn.model.is_none());
}

#[test]
fn test_remote_settings_remove_connection() {
    let mut settings = RemoteSettings::default();
    let conn = settings.add_connection("192.168.1.100".to_string(), 47842, None, None, None);
    let id = conn.id.clone();

    assert_eq!(settings.saved_connections.len(), 1);

    let result = settings.remove_connection(&id);
    assert!(result.is_ok());
    assert!(settings.saved_connections.is_empty());
}

#[test]
fn test_remote_settings_remove_nonexistent_connection() {
    let mut settings = RemoteSettings::default();
    let result = settings.remove_connection("nonexistent_id");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[test]
fn test_remote_settings_remove_clears_active_connection() {
    let mut settings = RemoteSettings::default();
    let conn = settings.add_connection("192.168.1.100".to_string(), 47842, None, None, None);
    let id = conn.id.clone();

    // Set this connection as active
    settings.set_active_connection(Some(id.clone())).unwrap();
    assert_eq!(settings.active_connection_id, Some(id.clone()));

    // Remove the connection
    settings.remove_connection(&id).unwrap();

    // Active connection should be cleared
    assert!(settings.active_connection_id.is_none());
}

#[test]
fn test_remote_settings_get_connection() {
    let mut settings = RemoteSettings::default();
    let conn = settings.add_connection(
        "192.168.1.100".to_string(),
        47842,
        Some("secret".to_string()),
        Some("My Server".to_string()),
        None,
    );
    let id = conn.id.clone();

    let retrieved = settings.get_connection(&id);
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.host, "192.168.1.100");
    assert_eq!(retrieved.port, 47842);
    assert_eq!(retrieved.password, Some("secret".to_string()));
}

#[test]
fn test_remote_settings_get_nonexistent_connection() {
    let settings = RemoteSettings::default();
    let result = settings.get_connection("nonexistent");
    assert!(result.is_none());
}

#[test]
fn test_remote_settings_set_active_connection() {
    let mut settings = RemoteSettings::default();
    let conn = settings.add_connection("192.168.1.100".to_string(), 47842, None, None, None);
    let id = conn.id.clone();

    let result = settings.set_active_connection(Some(id.clone()));
    assert!(result.is_ok());
    assert_eq!(settings.active_connection_id, Some(id));
}

#[test]
fn test_remote_settings_set_active_connection_none() {
    let mut settings = RemoteSettings::default();
    let conn = settings.add_connection("192.168.1.100".to_string(), 47842, None, None, None);
    let id = conn.id.clone();

    // Set active
    settings.set_active_connection(Some(id)).unwrap();
    assert!(settings.active_connection_id.is_some());

    // Clear active
    settings.set_active_connection(None).unwrap();
    assert!(settings.active_connection_id.is_none());
}

#[test]
fn test_remote_settings_set_active_nonexistent() {
    let mut settings = RemoteSettings::default();
    let result = settings.set_active_connection(Some("nonexistent".to_string()));
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[test]
fn test_remote_settings_get_active_connection() {
    let mut settings = RemoteSettings::default();
    let conn = settings.add_connection(
        "192.168.1.100".to_string(),
        47842,
        None,
        Some("Active Server".to_string()),
        None,
    );
    let id = conn.id.clone();

    settings.set_active_connection(Some(id)).unwrap();

    let active = settings.get_active_connection();
    assert!(active.is_some());
    assert_eq!(active.unwrap().name, Some("Active Server".to_string()));
}

#[test]
fn test_remote_settings_get_active_connection_none() {
    let settings = RemoteSettings::default();
    let active = settings.get_active_connection();
    assert!(active.is_none());
}

#[test]
fn test_remote_settings_list_connections() {
    let mut settings = RemoteSettings::default();

    // Add multiple connections
    settings.add_connection("192.168.1.1".to_string(), 1000, None, None, None);
    settings.add_connection("192.168.1.2".to_string(), 2000, None, None, None);
    settings.add_connection("192.168.1.3".to_string(), 3000, None, None, None);

    let list = settings.list_connections();
    assert_eq!(list.len(), 3);

    // Verify they're all different
    let hosts: Vec<&str> = list.iter().map(|c| c.host.as_str()).collect();
    assert!(hosts.contains(&"192.168.1.1"));
    assert!(hosts.contains(&"192.168.1.2"));
    assert!(hosts.contains(&"192.168.1.3"));
}

// ============================================================================
// SavedConnection Tests
// ============================================================================

#[test]
fn test_saved_connection_display_name_with_name() {
    let conn = SavedConnection {
        id: "test_id".to_string(),
        host: "192.168.1.100".to_string(),
        port: 47842,
        password: None,
        name: Some("My Server".to_string()),
        created_at: 0,
        model: None,
    };
    assert_eq!(conn.display_name(), "My Server");
}

#[test]
fn test_saved_connection_display_name_without_name() {
    let conn = SavedConnection {
        id: "test_id".to_string(),
        host: "192.168.1.100".to_string(),
        port: 47842,
        password: None,
        name: None,
        created_at: 0,
        model: None,
    };
    assert_eq!(conn.display_name(), "192.168.1.100:47842");
}

#[test]
fn test_saved_connection_serialization() {
    let conn = SavedConnection {
        id: "conn_123".to_string(),
        host: "10.0.0.1".to_string(),
        port: 8080,
        password: Some("secret".to_string()),
        name: Some("Test".to_string()),
        created_at: 1234567890,
        model: Some("whisper-large".to_string()),
    };

    let json = serde_json::to_string(&conn).unwrap();
    assert!(json.contains("\"id\":\"conn_123\""));
    assert!(json.contains("\"host\":\"10.0.0.1\""));
    assert!(json.contains("\"port\":8080"));
    assert!(json.contains("\"password\":\"secret\""));
    assert!(json.contains("\"model\":\"whisper-large\""));
}

#[test]
fn test_saved_connection_deserialization() {
    let json = r#"{
        "id": "conn_456",
        "host": "server.local",
        "port": 9000,
        "password": null,
        "name": "Local Server",
        "created_at": 9876543210,
        "model": "whisper-base"
    }"#;

    let conn: SavedConnection = serde_json::from_str(json).unwrap();
    assert_eq!(conn.id, "conn_456");
    assert_eq!(conn.host, "server.local");
    assert_eq!(conn.port, 9000);
    assert!(conn.password.is_none());
    assert_eq!(conn.name, Some("Local Server".to_string()));
    assert_eq!(conn.model, Some("whisper-base".to_string()));
}

#[test]
fn test_saved_connection_deserialization_missing_model() {
    // Test backwards compatibility - model field should default to None
    let json = r#"{
        "id": "conn_old",
        "host": "192.168.1.1",
        "port": 5000,
        "password": null,
        "name": null,
        "created_at": 1000
    }"#;

    let conn: SavedConnection = serde_json::from_str(json).unwrap();
    assert_eq!(conn.id, "conn_old");
    assert!(conn.model.is_none());
}

#[test]
fn test_saved_connection_equality() {
    let conn1 = SavedConnection {
        id: "same_id".to_string(),
        host: "192.168.1.1".to_string(),
        port: 8080,
        password: None,
        name: None,
        created_at: 1000,
        model: None,
    };

    let conn2 = SavedConnection {
        id: "same_id".to_string(),
        host: "192.168.1.1".to_string(),
        port: 8080,
        password: None,
        name: None,
        created_at: 1000,
        model: None,
    };

    assert_eq!(conn1, conn2);
}

#[test]
fn test_saved_connection_inequality() {
    let conn1 = SavedConnection {
        id: "id1".to_string(),
        host: "192.168.1.1".to_string(),
        port: 8080,
        password: None,
        name: None,
        created_at: 1000,
        model: None,
    };

    let conn2 = SavedConnection {
        id: "id2".to_string(),
        host: "192.168.1.1".to_string(),
        port: 8080,
        password: None,
        name: None,
        created_at: 1000,
        model: None,
    };

    assert_ne!(conn1, conn2);
}

// ============================================================================
// RemoteSettings Serialization Tests
// ============================================================================

#[test]
fn test_remote_settings_serialization() {
    let mut settings = RemoteSettings::default();
    settings.add_connection(
        "192.168.1.100".to_string(),
        47842,
        Some("pass".to_string()),
        Some("Server".to_string()),
        Some("model".to_string()),
    );
    settings.sharing_was_active = true;

    let json = serde_json::to_string(&settings).unwrap();
    assert!(json.contains("saved_connections"));
    assert!(json.contains("192.168.1.100"));
    assert!(json.contains("\"sharing_was_active\":true"));
}

#[test]
fn test_remote_settings_deserialization() {
    let json = r#"{
        "server_config": {
            "enabled": false,
            "port": 47842,
            "password": null
        },
        "saved_connections": [],
        "active_connection_id": null,
        "sharing_was_active": false
    }"#;

    let settings: RemoteSettings = serde_json::from_str(json).unwrap();
    assert!(settings.saved_connections.is_empty());
    assert!(settings.active_connection_id.is_none());
    assert!(!settings.sharing_was_active);
}

#[test]
fn test_remote_settings_roundtrip() {
    let mut original = RemoteSettings::default();
    original.add_connection(
        "test.local".to_string(),
        9999,
        Some("mypass".to_string()),
        Some("Test".to_string()),
        Some("model-v2".to_string()),
    );
    original.sharing_was_active = true;

    let json = serde_json::to_string(&original).unwrap();
    let restored: RemoteSettings = serde_json::from_str(&json).unwrap();

    assert_eq!(
        original.saved_connections.len(),
        restored.saved_connections.len()
    );
    assert_eq!(original.sharing_was_active, restored.sharing_was_active);
}

// ============================================================================
// Multiple Connections Tests
// ============================================================================

#[test]
fn test_multiple_connections_unique_ids() {
    let mut settings = RemoteSettings::default();

    let conn1 = settings.add_connection("host1".to_string(), 1000, None, None, None);
    let conn2 = settings.add_connection("host2".to_string(), 2000, None, None, None);
    let conn3 = settings.add_connection("host3".to_string(), 3000, None, None, None);

    // All IDs should be unique
    assert_ne!(conn1.id, conn2.id);
    assert_ne!(conn2.id, conn3.id);
    assert_ne!(conn1.id, conn3.id);
}

#[test]
fn test_remove_middle_connection() {
    let mut settings = RemoteSettings::default();

    let conn1 = settings.add_connection("host1".to_string(), 1000, None, None, None);
    let conn2 = settings.add_connection("host2".to_string(), 2000, None, None, None);
    let conn3 = settings.add_connection("host3".to_string(), 3000, None, None, None);

    // Remove middle connection
    settings.remove_connection(&conn2.id).unwrap();

    // Should have 2 connections left
    assert_eq!(settings.saved_connections.len(), 2);

    // conn1 and conn3 should still exist
    assert!(settings.get_connection(&conn1.id).is_some());
    assert!(settings.get_connection(&conn3.id).is_some());
    assert!(settings.get_connection(&conn2.id).is_none());
}

#[test]
fn test_switch_active_connection() {
    let mut settings = RemoteSettings::default();

    let conn1 = settings.add_connection(
        "host1".to_string(),
        1000,
        None,
        Some("First".to_string()),
        None,
    );
    let conn2 = settings.add_connection(
        "host2".to_string(),
        2000,
        None,
        Some("Second".to_string()),
        None,
    );

    // Set first as active
    settings
        .set_active_connection(Some(conn1.id.clone()))
        .unwrap();
    assert_eq!(
        settings.get_active_connection().unwrap().name,
        Some("First".to_string())
    );

    // Switch to second
    settings
        .set_active_connection(Some(conn2.id.clone()))
        .unwrap();
    assert_eq!(
        settings.get_active_connection().unwrap().name,
        Some("Second".to_string())
    );

    // Clear active
    settings.set_active_connection(None).unwrap();
    assert!(settings.get_active_connection().is_none());
}
