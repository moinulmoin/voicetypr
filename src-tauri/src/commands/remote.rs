//! Tauri commands for remote transcription
//!
//! These commands allow the frontend to control remote transcription
//! server mode and client mode.

use std::path::PathBuf;

use tauri::{
    async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock},
    AppHandle, Emitter, Manager, State,
};
use tauri_plugin_store::StoreExt;

use crate::remote::client::RemoteServerConnection;
use crate::remote::lifecycle::{RemoteServerManager, SharingStatus};
use crate::remote::server::StatusResponse;
use crate::remote::settings::{RemoteSettings, SavedConnection};
use crate::whisper::manager::WhisperManager;

/// Default port for remote transcription
pub const DEFAULT_PORT: u16 = 47842;

// ============================================================================
// Server Mode Commands
// ============================================================================

/// Start sharing the currently selected model with other VoiceTypr instances
#[tauri::command]
pub async fn start_sharing(
    app: AppHandle,
    port: Option<u16>,
    password: Option<String>,
    server_name: Option<String>,
    server_manager: State<'_, AsyncMutex<RemoteServerManager>>,
    whisper_manager: State<'_, AsyncRwLock<WhisperManager>>,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<(), String> {
    let port = port.unwrap_or(DEFAULT_PORT);

    // Get server name from hostname if not provided
    let server_name = server_name.unwrap_or_else(|| {
        hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "VoiceTypr Server".to_string())
    });

    // Get current model and engine from store
    // NOTE: Must use "settings" store to match save_settings command
    let store = app
        .store("settings")
        .map_err(|e| format!("Failed to access store: {}", e))?;

    let stored_model = store
        .get("current_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    let stored_engine = store
        .get("current_model_engine")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "whisper".to_string());

    // Get model path based on engine
    let (current_model, model_path, engine) = {
        let manager = whisper_manager.read().await;

        // If no model explicitly selected, find first downloaded model (whisper only for auto-select)
        let model_name = if stored_model.is_empty() {
            manager
                .get_first_downloaded_model()
                .ok_or("No model downloaded. Please download a model first.")?
        } else {
            stored_model
        };

        // Get model path - for whisper, get from manager; for others, use empty path
        // (the actual path will be resolved during transcription for non-whisper engines)
        let path = if stored_engine == "whisper" {
            manager
                .get_model_path(&model_name)
                .ok_or_else(|| format!("Model '{}' not found or not downloaded", model_name))?
        } else {
            // For parakeet and other engines, model path isn't needed at server start
            // The transcription context will handle engine-specific paths
            std::path::PathBuf::new()
        };

        (model_name, path, stored_engine)
    };

    // Start the server (pass app handle for Parakeet support)
    let mut manager = server_manager.lock().await;
    manager
        .start(
            port,
            password.clone(),
            server_name,
            model_path,
            current_model,
            engine,
            Some(app.clone()),
        )
        .await?;

    // Persist the sharing enabled state so it auto-starts on next launch
    {
        let mut settings = remote_settings.lock().await;
        settings.server_config.enabled = true;
        settings.server_config.port = port;
        settings.server_config.password = password;
        save_remote_settings(&app, &settings)?;
        log::info!(
            "🌐 [SHARING] Saved sharing state: enabled=true, port={}",
            port
        );
    }

    log::info!("Sharing started on port {}", port);

    // Emit event to notify frontend of sharing status change
    let status = manager.get_status();
    let _ = app.emit(
        "sharing-status-changed",
        serde_json::json!({
            "enabled": status.enabled,
            "port": status.port,
            "model_name": status.model_name
        }),
    );
    log::info!("🔔 [SHARING] Emitted sharing-status-changed event (enabled=true)");

    Ok(())
}

/// Stop sharing models with other VoiceTypr instances
#[tauri::command]
pub async fn stop_sharing(
    app: AppHandle,
    server_manager: State<'_, AsyncMutex<RemoteServerManager>>,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<(), String> {
    let mut manager = server_manager.lock().await;
    manager.stop();

    // Persist the sharing disabled state
    {
        let mut settings = remote_settings.lock().await;
        settings.server_config.enabled = false;
        save_remote_settings(&app, &settings)?;
        log::info!("🌐 [SHARING] Saved sharing state: enabled=false");
    }

    log::info!("Sharing stopped");

    // Emit event to notify frontend of sharing status change
    let _ = app.emit(
        "sharing-status-changed",
        serde_json::json!({
            "enabled": false,
            "port": null,
            "model_name": null
        }),
    );
    log::info!("🔔 [SHARING] Emitted sharing-status-changed event (enabled=false)");

    Ok(())
}

/// Get the current sharing status
#[tauri::command]
pub async fn get_sharing_status(
    server_manager: State<'_, AsyncMutex<RemoteServerManager>>,
) -> Result<SharingStatus, String> {
    let manager = server_manager.lock().await;
    Ok(manager.get_status())
}

/// Get local IP addresses for display in Network Sharing UI
#[tauri::command]
pub fn get_local_ips() -> Result<Vec<String>, String> {
    use local_ip_address::list_afinet_netifas;

    let network_interfaces =
        list_afinet_netifas().map_err(|e| format!("Failed to get network interfaces: {}", e))?;

    let ips: Vec<String> = network_interfaces
        .into_iter()
        .filter_map(|(name, ip)| {
            // Skip loopback addresses
            if ip.is_loopback() {
                return None;
            }
            // Only include IPv4 addresses for simplicity
            if let std::net::IpAddr::V4(ipv4) = ip {
                // Skip link-local addresses (169.254.x.x)
                if ipv4.is_link_local() {
                    return None;
                }
                Some(format!("{} ({})", ip, name))
            } else {
                None // Skip IPv6 for now
            }
        })
        .collect();

    if ips.is_empty() {
        Ok(vec!["No network connection".to_string()])
    } else {
        Ok(ips)
    }
}

// ============================================================================
// Client Mode Commands
// ============================================================================

/// Add a new remote server connection
#[tauri::command]
pub async fn add_remote_server(
    app: AppHandle,
    host: String,
    port: u16,
    password: Option<String>,
    name: Option<String>,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<SavedConnection, String> {
    // Try to test connection to get server name and model, but don't require it
    let (display_name, model) = match test_connection(&host, port, password.as_deref()).await {
        Ok(status) => {
            // Use server name if no custom name provided
            let name_to_use = name.unwrap_or(status.name);
            (name_to_use, Some(status.model))
        }
        Err(_) => {
            // Connection failed, but still allow adding the server
            log::info!(
                "Connection test failed for {}:{}, adding server anyway",
                host,
                port
            );
            (name.unwrap_or_else(|| format!("{}:{}", host, port)), None)
        }
    };

    // Add to settings
    let mut settings = remote_settings.lock().await;
    let connection =
        settings.add_connection(host, port, password, Some(display_name), model.clone());

    log::info!(
        "Added remote server: {} (model: {:?})",
        connection.display_name(),
        model
    );

    // Save settings
    save_remote_settings(&app, &settings)?;

    Ok(connection)
}

/// Remove a remote server connection
#[tauri::command]
pub async fn remove_remote_server(
    app: AppHandle,
    server_id: String,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<(), String> {
    let mut settings = remote_settings.lock().await;
    settings.remove_connection(&server_id)?;

    log::info!("Removed remote server: {}", server_id);

    // Save settings
    save_remote_settings(&app, &settings)?;

    Ok(())
}

/// Update an existing remote server connection
#[tauri::command]
pub async fn update_remote_server(
    app: AppHandle,
    server_id: String,
    host: String,
    port: u16,
    password: Option<String>,
    name: Option<String>,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<SavedConnection, String> {
    let mut settings = remote_settings.lock().await;

    // Find the existing connection
    let existing = settings
        .get_connection(&server_id)
        .ok_or_else(|| format!("Server '{}' not found", server_id))?
        .clone();

    // Try to test connection to get model info, but don't require it
    let model = match test_connection(&host, port, password.as_deref()).await {
        Ok(status) => Some(status.model),
        Err(_) => existing.model.clone(), // Keep existing model if connection fails
    };

    // Remove old connection and add updated one with same ID
    settings.remove_connection(&server_id)?;

    // Re-add with updated values but preserve the original ID
    let display_name = name.unwrap_or_else(|| format!("{}:{}", host, port));
    let mut connection = settings.add_connection(host, port, password, Some(display_name), model);

    // Override the generated ID with the original ID to maintain references
    connection.id = server_id.clone();

    // Update the connection in the list with the correct ID
    if let Some(last) = settings.saved_connections.last_mut() {
        last.id = server_id.clone();
    }

    log::info!("Updated remote server: {}", connection.display_name());

    // Save settings
    save_remote_settings(&app, &settings)?;

    Ok(connection)
}

/// List all saved remote server connections
#[tauri::command]
pub async fn list_remote_servers(
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<Vec<SavedConnection>, String> {
    let settings = remote_settings.lock().await;
    Ok(settings.list_connections())
}

/// Test connection to a remote server by host/port/password (before saving)
/// Returns specific error types: "connection_failed", "auth_failed", or success
#[tauri::command]
pub async fn test_remote_connection(
    host: String,
    port: u16,
    password: Option<String>,
) -> Result<StatusResponse, String> {
    test_connection(&host, port, password.as_deref()).await
}

/// Test connection to a saved remote server
/// Also updates the cached model if it changed and refreshes the tray menu
#[tauri::command]
pub async fn test_remote_server(
    app: AppHandle,
    server_id: String,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<StatusResponse, String> {
    // Get connection info and cached model
    let (connection, cached_model) = {
        let settings = remote_settings.lock().await;
        let conn = settings
            .get_connection(&server_id)
            .ok_or_else(|| format!("Server '{}' not found", server_id))?
            .clone();
        let cached = conn.model.clone();
        (conn, cached)
    };

    // Test the connection
    let status = test_connection(
        &connection.host,
        connection.port,
        connection.password.as_deref(),
    )
    .await?;

    // Check if model changed and update if needed
    let new_model = Some(status.model.clone());
    if cached_model != new_model {
        log::info!(
            "🔄 [REMOTE] Model changed for '{}': {:?} -> {:?}",
            connection.display_name(),
            cached_model,
            new_model
        );

        // Update the cached model
        {
            let mut settings = remote_settings.lock().await;
            if let Some(conn) = settings
                .saved_connections
                .iter_mut()
                .find(|c| c.id == server_id)
            {
                conn.model = new_model;
            }
            // Save updated settings
            save_remote_settings(&app, &settings)?;
        }

        // Refresh tray menu to show new model
        if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
            log::warn!(
                "🔄 [REMOTE] Failed to update tray menu after model change: {}",
                e
            );
        } else {
            log::info!("🔄 [REMOTE] Tray menu updated with new model");
        }
    }

    Ok(status)
}

/// Set the active remote server for transcription
#[tauri::command]
pub async fn set_active_remote_server(
    app: AppHandle,
    server_id: Option<String>,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
    server_manager: State<'_, AsyncMutex<RemoteServerManager>>,
) -> Result<(), String> {
    log::info!(
        "🔧 [REMOTE] set_active_remote_server called with server_id={:?}",
        server_id
    );

    // Track if we need to restore sharing after clearing remote server
    let mut should_restore_sharing = false;
    let mut restore_port: Option<u16> = None;
    let mut restore_password: Option<String> = None;

    // If selecting a remote server, stop sharing first and remember it was active
    if server_id.is_some() {
        let manager = server_manager.lock().await;
        if manager.get_status().enabled {
            // Get current sharing settings before stopping
            let status = manager.get_status();
            restore_port = status.port;
            restore_password = status.password.clone();

            drop(manager); // Release lock before calling stop
            log::info!("🔧 [REMOTE] Stopping network sharing - using remote server (will remember for auto-restore)");
            let mut manager = server_manager.lock().await;
            manager.stop();

            // Set flag in remote settings to remember sharing was active
            let mut settings = remote_settings.lock().await;
            settings.sharing_was_active = true;
            save_remote_settings(&app, &settings)?;
            log::info!("🔧 [REMOTE] Network sharing stopped, sharing_was_active flag set");

            // Emit event to notify frontend of sharing status change
            let _ = app.emit(
                "sharing-status-changed",
                serde_json::json!({
                    "enabled": false,
                    "port": null,
                    "model_name": null
                }),
            );
            log::info!("🔔 [SHARING] Emitted sharing-status-changed event (auto-disabled for remote server)");
        }
    } else {
        // Clearing remote server - check if we should restore sharing
        let settings = remote_settings.lock().await;
        if settings.sharing_was_active {
            should_restore_sharing = true;
            // Get stored sharing settings from app settings
            if let Ok(store) = app.store("settings") {
                restore_port = store
                    .get("sharing_port")
                    .and_then(|v| v.as_u64())
                    .map(|p| p as u16);
                restore_password = store
                    .get("sharing_password")
                    .and_then(|v| v.as_str().map(|s| s.to_string()));
            }
            log::info!("🔧 [REMOTE] Will restore sharing after clearing remote server");
        }
    }

    // Update settings (scoped to release lock before tray update)
    {
        let mut settings = remote_settings.lock().await;
        log::info!(
            "🔧 [REMOTE] Before change: active_connection_id={:?}",
            settings.active_connection_id
        );

        if let Some(id) = &server_id {
            settings.set_active_connection(Some(id.clone()))?;
            log::info!("🔧 [REMOTE] Active remote server set to: {}", id);
        } else {
            settings.set_active_connection(None)?;
            // Clear the sharing_was_active flag since we're restoring now
            if should_restore_sharing {
                settings.sharing_was_active = false;
            }
            log::info!("🔧 [REMOTE] Active remote server cleared");
        }

        log::info!(
            "🔧 [REMOTE] After change: active_connection_id={:?}",
            settings.active_connection_id
        );

        // Save settings
        save_remote_settings(&app, &settings)?;
        log::info!("🔧 [REMOTE] Settings saved to store");
    }

    // Restore sharing if we were using it before switching to remote
    if should_restore_sharing {
        let port = restore_port.unwrap_or(DEFAULT_PORT);
        log::info!(
            "🔧 [REMOTE] Auto-restoring network sharing on port {}",
            port
        );

        let mut manager = server_manager.lock().await;

        // Get current model and engine from store
        let (current_model, current_engine) = {
            if let Ok(store) = app.store("settings") {
                let model = store
                    .get("current_model")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| "base.en".to_string());
                let engine = store
                    .get("current_model_engine")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| "whisper".to_string());
                (model, engine)
            } else {
                ("base.en".to_string(), "whisper".to_string())
            }
        };

        let server_name = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "VoiceTypr Server".to_string());

        // Get model path for whisper models
        let model_path: PathBuf = if current_engine == "whisper" {
            let whisper_state: &AsyncRwLock<WhisperManager> =
                app.state::<AsyncRwLock<WhisperManager>>().inner();
            let guard = whisper_state.read().await;
            guard.get_model_path(&current_model).unwrap_or_default()
        } else {
            PathBuf::new()
        };

        if let Err(e) = manager
            .start(
                port,
                restore_password,
                server_name,
                model_path,
                current_model,
                current_engine,
                Some(app.clone()),
            )
            .await
        {
            log::warn!("🔧 [REMOTE] Failed to auto-restore sharing: {}", e);
        } else {
            log::info!("🔧 [REMOTE] Network sharing auto-restored successfully");

            // Emit event to notify frontend of sharing status change
            let status = manager.get_status();
            let _ = app.emit(
                "sharing-status-changed",
                serde_json::json!({
                    "enabled": status.enabled,
                    "port": status.port,
                    "model_name": status.model_name
                }),
            );
            log::info!("🔔 [SHARING] Emitted sharing-status-changed event (auto-restored)");
        }
    }

    // Update tray menu to reflect the change
    if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
        log::warn!("🔧 [REMOTE] Failed to update tray menu: {}", e);
    } else {
        log::info!("🔧 [REMOTE] Tray menu updated");
    }

    Ok(())
}

/// Get the currently active remote server ID
#[tauri::command]
pub async fn get_active_remote_server(
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<Option<String>, String> {
    let settings = remote_settings.lock().await;
    let active_id = settings.active_connection_id.clone();
    log::info!(
        "🔍 [REMOTE] get_active_remote_server returning: {:?}",
        active_id
    );
    Ok(active_id)
}

// ============================================================================
// Transcription Commands
// ============================================================================

/// Transcribe audio using a remote server
#[tauri::command]
pub async fn transcribe_remote(
    server_id: String,
    audio_path: String,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<String, String> {
    // Get the connection info
    let connection = {
        let settings = remote_settings.lock().await;
        settings
            .get_connection(&server_id)
            .cloned()
            .ok_or_else(|| format!("Server '{}' not found", server_id))?
    };

    let display_name = connection.display_name();
    log::info!(
        "[Remote Client] Starting remote transcription to '{}' ({}:{})",
        display_name,
        connection.host,
        connection.port
    );

    // Read the audio file
    let audio_data =
        std::fs::read(&audio_path).map_err(|e| format!("Failed to read audio file: {}", e))?;

    let audio_size_kb = audio_data.len() as f64 / 1024.0;
    log::info!(
        "[Remote Client] Sending {:.1} KB audio to '{}'",
        audio_size_kb,
        display_name
    );

    // Create HTTP client connection
    let server_conn =
        RemoteServerConnection::new(connection.host, connection.port, connection.password);

    // Send transcription request
    let client = reqwest::Client::new();
    let mut request = client
        .post(&server_conn.transcribe_url())
        .header("Content-Type", "audio/wav")
        .body(audio_data);

    // Add auth header if password is set
    if let Some(pwd) = server_conn.password.as_ref() {
        request = request.header("X-VoiceTypr-Key", pwd);
    }

    let response = request.send().await.map_err(|e| {
        log::warn!(
            "[Remote Client] Connection FAILED to '{}': {}",
            display_name,
            e
        );
        format!("Failed to send request: {}", e)
    })?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        log::warn!(
            "[Remote Client] Authentication FAILED to '{}'",
            display_name
        );
        return Err("Authentication failed".to_string());
    }

    if !response.status().is_success() {
        log::warn!(
            "[Remote Client] Server error from '{}': {}",
            display_name,
            response.status()
        );
        return Err(format!("Server error: {}", response.status()));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let text = result
        .get("text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Invalid response: missing 'text' field".to_string())?;

    log::info!(
        "[Remote Client] Transcription COMPLETED from '{}': {} chars received",
        display_name,
        text.len()
    );

    Ok(text)
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Test connection to a remote server
async fn test_connection(
    host: &str,
    port: u16,
    password: Option<&str>,
) -> Result<StatusResponse, String> {
    let conn = RemoteServerConnection::new(host.to_string(), port, password.map(String::from));

    log::info!("[Remote Client] Testing connection to {}:{}", host, port);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let mut request = client.get(&conn.status_url());

    if let Some(pwd) = password {
        request = request.header("X-VoiceTypr-Key", pwd);
    }

    let response = request.send().await.map_err(|e| {
        log::warn!(
            "[Remote Client] Connection test FAILED to {}:{} - {}",
            host,
            port,
            e
        );
        format!("Failed to connect: {}", e)
    })?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        log::warn!(
            "[Remote Client] Connection test REJECTED - authentication failed for {}:{}",
            host,
            port
        );
        return Err("Authentication failed".to_string());
    }

    if !response.status().is_success() {
        log::warn!(
            "[Remote Client] Connection test FAILED - server error {} for {}:{}",
            response.status(),
            host,
            port
        );
        return Err(format!("Server error: {}", response.status()));
    }

    let status = response
        .json::<StatusResponse>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    log::info!(
        "[Remote Client] Connection test SUCCEEDED to '{}' ({}:{}) - model: '{}', version: {}",
        status.name,
        host,
        port,
        status.model,
        status.version
    );

    Ok(status)
}

/// Save remote settings to the store
pub fn save_remote_settings(app: &AppHandle, settings: &RemoteSettings) -> Result<(), String> {
    let store = app
        .store("voicetypr-store.json")
        .map_err(|e| format!("Failed to access store: {}", e))?;

    let settings_json = serde_json::to_value(settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    store.set("remote_settings", settings_json);
    store
        .save()
        .map_err(|e| format!("Failed to save store: {}", e))?;

    Ok(())
}

/// Load remote settings from the store
pub fn load_remote_settings(app: &AppHandle) -> RemoteSettings {
    log::info!("🔧 [REMOTE] load_remote_settings called");

    let store = match app.store("voicetypr-store.json") {
        Ok(s) => s,
        Err(e) => {
            log::warn!(
                "🔧 [REMOTE] Failed to open store: {:?}, returning default",
                e
            );
            return RemoteSettings::default();
        }
    };

    let raw_value = store.get("remote_settings");
    log::info!(
        "🔧 [REMOTE] Raw store value exists: {}",
        raw_value.is_some()
    );

    let settings: RemoteSettings = raw_value
        .and_then(|v| {
            log::debug!("🔧 [REMOTE] Raw JSON: {:?}", v);
            serde_json::from_value(v.clone()).ok()
        })
        .unwrap_or_default();

    log::info!(
        "🔧 [REMOTE] Loaded settings: {} connections, active_id={:?}",
        settings.saved_connections.len(),
        settings.active_connection_id
    );

    settings
}

/// Get the unique machine ID for this VoiceTypr instance
/// Used to prevent adding self as a remote server
#[tauri::command]
pub fn get_local_machine_id() -> Result<String, String> {
    crate::license::device::get_device_hash()
}

// ============================================================================
// Firewall Detection (macOS and Windows)
// ============================================================================

/// Firewall status for network sharing
#[derive(Debug, Clone, serde::Serialize)]
pub struct FirewallStatus {
    /// Whether the system firewall is enabled
    pub firewall_enabled: bool,
    /// Whether VoiceTypr is allowed through the firewall
    pub app_allowed: bool,
    /// Whether incoming connections may be blocked
    pub may_be_blocked: bool,
}

/// Check if the system firewall may be blocking incoming connections
/// Returns firewall status to help users troubleshoot connection issues
#[tauri::command]
pub fn get_firewall_status() -> FirewallStatus {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        // Check if firewall is enabled
        let firewall_enabled = Command::new("/usr/libexec/ApplicationFirewall/socketfilterfw")
            .arg("--getglobalstate")
            .output()
            .map(|output| {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.contains("enabled") || stdout.contains("State = 1")
            })
            .unwrap_or(false);

        if !firewall_enabled {
            return FirewallStatus {
                firewall_enabled: false,
                app_allowed: true, // No firewall means no blocking
                may_be_blocked: false,
            };
        }

        // Check if VoiceTypr is in the allow list
        // The output format is:
        //   60 : /Applications/Voicetypr.app
        //              (Allow incoming connections)
        // We need to find a voicetypr entry followed by "Allow" on the next line
        let app_allowed = Command::new("/usr/libexec/ApplicationFirewall/socketfilterfw")
            .arg("--listapps")
            .output()
            .map(|output| {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let lines: Vec<&str> = stdout.lines().collect();

                // Find any line containing voicetypr and check the next line for "Allow"
                for (i, line) in lines.iter().enumerate() {
                    if line.to_lowercase().contains("voicetypr") {
                        // Check if next line contains "Allow incoming connections"
                        if let Some(next_line) = lines.get(i + 1) {
                            if next_line
                                .to_lowercase()
                                .contains("allow incoming connections")
                            {
                                return true;
                            }
                        }
                        // Also check same line in case format varies
                        if line.to_lowercase().contains("allow incoming connections") {
                            return true;
                        }
                    }
                }
                false
            })
            .unwrap_or(false);

        FirewallStatus {
            firewall_enabled: true,
            app_allowed,
            may_be_blocked: !app_allowed,
        }
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows, we don't show a proactive firewall warning because:
        // 1. Windows Firewall shows its own popup when an app starts listening on a port
        // 2. We can't reliably detect if traffic is actually blocked without testing
        // 3. Checking for a rule named "VoiceTypr" is unreliable - user may have clicked
        //    "Allow" in the Windows popup, which creates a rule with a different name
        //
        // If users have connection issues, they'll troubleshoot from there.
        // Showing a warning when ports aren't actually blocked is confusing.
        FirewallStatus {
            firewall_enabled: false, // Don't claim we know firewall state
            app_allowed: true,
            may_be_blocked: false, // Don't show warning on Windows
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // On other platforms (Linux, etc.), assume no firewall issues
        // Linux firewall detection could be added later (iptables/ufw/firewalld)
        FirewallStatus {
            firewall_enabled: false,
            app_allowed: true,
            may_be_blocked: false,
        }
    }
}

/// Open the system firewall settings
#[tauri::command]
pub fn open_firewall_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        // Try macOS Ventura+ Network > Firewall URL first
        let result = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.Network-Settings.extension?Firewall")
            .spawn();

        if result.is_err() {
            // Fallback to older Security & Privacy > Firewall URL
            let result2 = Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Firewall")
                .spawn();

            if result2.is_err() {
                // Last resort: open System Settings directly
                let _ = Command::new("open")
                    .arg("-a")
                    .arg("System Settings")
                    .spawn();
            }
        }

        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        // Open Windows Firewall settings
        let _ = Command::new("control").arg("firewall.cpl").spawn();
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("Firewall settings not supported on this platform".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_port() {
        assert_eq!(DEFAULT_PORT, 47842);
    }
}
