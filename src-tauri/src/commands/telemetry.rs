//! Tauri commands backing the Diagnostics (opt-in error reporting) consent
//! toggle and the frontend error-capture bridge.
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
    // On opt-out, stop egress IMMEDIATELY — before any (possibly failing) store
    // write — so no in-flight error can slip through after the user said no.
    if !enabled {
        telemetry::set_enabled(false);
    }

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

    // Only turn the gate ON after a successful persist; enabling is fully
    // effective next launch (the client/panic hook are wired at startup).
    if enabled {
        telemetry::set_enabled(true);
    }
    let restart_required = enabled && !was_enabled;

    Ok(TelemetryConsentResult {
        enabled,
        restart_required,
    })
}

/// Bridges a frontend-reported error (e.g. from the React error boundary) into
/// the Rust Sentry client, where it is scrubbed by `before_send`. Gated on
/// consent and a no-op when telemetry is disabled.
#[tauri::command]
pub async fn report_frontend_error(name: Option<String>, message: String) -> Result<(), String> {
    telemetry::capture_frontend_error(name.as_deref(), &message);
    Ok(())
}
