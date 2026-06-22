//! Tauri commands backing the Diagnostics (opt-in error reporting) consent toggle.
//!
//! Consent is stored as dedicated keys in the `settings` store (read at startup
//! by a raw, fail-closed reader in `crate::telemetry`) and is owned exclusively
//! by these commands — never by the generic `save_settings` flow.

use crate::telemetry;
use serde::Serialize;
use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const SETTINGS_STORE_FILE: &str = "settings";

#[derive(Serialize)]
pub struct TelemetryStatus {
    /// Whether the user has opted in.
    pub enabled: bool,
    /// Whether this build can report at all (a DSN was compiled in).
    pub available: bool,
}

#[derive(Serialize)]
pub struct TelemetryConsentResult {
    pub enabled: bool,
    /// Enabling mid-session needs a restart to actually wire the Sentry client
    /// (it is only initialized at startup). Disabling takes effect immediately.
    pub restart_required: bool,
}

/// Returns the current consent + whether reporting is possible in this build.
#[tauri::command]
pub async fn get_telemetry_status(app: AppHandle) -> Result<TelemetryStatus, String> {
    let store = app.store(SETTINGS_STORE_FILE).map_err(|e| e.to_string())?;
    let enabled = store
        .get(telemetry::KEY_TELEMETRY_ENABLED)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(TelemetryStatus {
        enabled,
        available: telemetry::is_available(),
    })
}

/// Persists the consent choice. On opt-in, mints an anonymous install id if one
/// does not exist; on opt-out, deletes it (a fresh id is minted if re-enabled).
#[tauri::command]
pub async fn set_telemetry_consent(
    app: AppHandle,
    enabled: bool,
) -> Result<TelemetryConsentResult, String> {
    let store = app.store(SETTINGS_STORE_FILE).map_err(|e| e.to_string())?;
    let was_enabled = store
        .get(telemetry::KEY_TELEMETRY_ENABLED)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    store.set(telemetry::KEY_TELEMETRY_ENABLED, json!(enabled));
    if enabled {
        let has_id = store
            .get(telemetry::KEY_TELEMETRY_INSTALL_ID)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .is_some();
        if !has_id {
            store.set(
                telemetry::KEY_TELEMETRY_INSTALL_ID,
                json!(uuid::Uuid::new_v4().to_string()),
            );
        }
    } else {
        store.delete(telemetry::KEY_TELEMETRY_INSTALL_ID);
    }
    store.save().map_err(|e| e.to_string())?;

    // Flip the in-process gate. Disabling stops egress immediately; enabling is
    // only fully effective after a restart (client/plugin are wired at startup).
    telemetry::set_enabled(enabled);
    let restart_required = enabled && !was_enabled;

    Ok(TelemetryConsentResult {
        enabled,
        restart_required,
    })
}
