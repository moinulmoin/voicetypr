//! Tauri commands for remote transcription
//!
//! These commands allow the frontend to control remote transcription
//! server mode and client mode.

use std::path::PathBuf;
use std::sync::RwLock;

use tauri::{async_runtime::Mutex as AsyncMutex, AppHandle, State};
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
    whisper_manager: State<'_, RwLock<WhisperManager>>,
) -> Result<(), String> {
    let port = port.unwrap_or(DEFAULT_PORT);

    // Get server name from hostname if not provided
    let server_name = server_name.unwrap_or_else(|| {
        hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "VoiceTypr Server".to_string())
    });

    // Get current model from store
    let store = app
        .store("voicetypr-store.json")
        .map_err(|e| format!("Failed to access store: {}", e))?;

    let current_model = store
        .get("current_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .ok_or("No model selected")?;

    // Get model path from whisper manager
    let model_path = {
        let manager = whisper_manager
            .read()
            .map_err(|e| format!("Failed to read whisper manager: {}", e))?;

        manager
            .get_model_path(&current_model)
            .ok_or_else(|| format!("Model '{}' not found or not downloaded", current_model))?
    };

    // Start the server
    let mut manager = server_manager.lock().await;
    manager
        .start(port, password, server_name, model_path, current_model)
        .await?;

    log::info!("Sharing started on port {}", port);
    Ok(())
}

/// Stop sharing models with other VoiceTypr instances
#[tauri::command]
pub async fn stop_sharing(
    server_manager: State<'_, AsyncMutex<RemoteServerManager>>,
) -> Result<(), String> {
    let mut manager = server_manager.lock().await;
    manager.stop();

    log::info!("Sharing stopped");
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
    // First test the connection
    let status = test_connection(&host, port, password.as_deref()).await?;

    // Use server name if no custom name provided
    let display_name = name.unwrap_or(status.name);

    // Add to settings
    let mut settings = remote_settings.lock().await;
    let connection = settings.add_connection(host, port, password, Some(display_name));

    log::info!("Added remote server: {}", connection.display_name());

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

/// List all saved remote server connections
#[tauri::command]
pub async fn list_remote_servers(
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<Vec<SavedConnection>, String> {
    let settings = remote_settings.lock().await;
    Ok(settings.list_connections())
}

/// Test connection to a remote server
#[tauri::command]
pub async fn test_remote_server(
    server_id: String,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<StatusResponse, String> {
    let settings = remote_settings.lock().await;
    let connection = settings
        .get_connection(&server_id)
        .ok_or_else(|| format!("Server '{}' not found", server_id))?;

    test_connection(&connection.host, connection.port, connection.password.as_deref()).await
}

/// Set the active remote server for transcription
#[tauri::command]
pub async fn set_active_remote_server(
    app: AppHandle,
    server_id: Option<String>,
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<(), String> {
    let mut settings = remote_settings.lock().await;

    if let Some(id) = &server_id {
        settings.set_active_connection(Some(id.clone()))?;
        log::info!("Active remote server set to: {}", id);
    } else {
        settings.set_active_connection(None)?;
        log::info!("Active remote server cleared");
    }

    // Save settings
    save_remote_settings(&app, &settings)?;

    Ok(())
}

/// Get the currently active remote server
#[tauri::command]
pub async fn get_active_remote_server(
    remote_settings: State<'_, AsyncMutex<RemoteSettings>>,
) -> Result<Option<SavedConnection>, String> {
    let settings = remote_settings.lock().await;
    Ok(settings.get_active_connection().cloned())
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

    // Read the audio file
    let audio_data = std::fs::read(&audio_path)
        .map_err(|e| format!("Failed to read audio file: {}", e))?;

    // Create HTTP client connection
    let server_conn = RemoteServerConnection::new(
        connection.host,
        connection.port,
        connection.password,
    );

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

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Authentication failed".to_string());
    }

    if !response.status().is_success() {
        return Err(format!("Server error: {}", response.status()));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    result
        .get("text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Invalid response: missing 'text' field".to_string())
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

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let mut request = client.get(&conn.status_url());

    if let Some(pwd) = password {
        request = request.header("X-VoiceTypr-Key", pwd);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Authentication failed".to_string());
    }

    if !response.status().is_success() {
        return Err(format!("Server error: {}", response.status()));
    }

    response
        .json::<StatusResponse>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

/// Save remote settings to the store
fn save_remote_settings(app: &AppHandle, settings: &RemoteSettings) -> Result<(), String> {
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
    let store = match app.store("voicetypr-store.json") {
        Ok(s) => s,
        Err(_) => return RemoteSettings::default(),
    };

    store
        .get("remote_settings")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_port() {
        assert_eq!(DEFAULT_PORT, 47842);
    }
}
