use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::parakeet;
use crate::remote::settings::{ConnectionStatus, RemoteSettings};
use crate::whisper;

/// Snapshot of recognition engine availability
#[derive(Debug, Clone, serde::Serialize)]
pub struct RecognitionAvailabilitySnapshot {
    pub whisper_available: bool,
    pub parakeet_available: bool,
    pub soniox_selected: bool,
    pub soniox_ready: bool,
    pub remote_selected: bool,
    pub remote_status: ConnectionStatus,
    pub remote_last_checked: u64,
    pub remote_available: bool,
}

impl RecognitionAvailabilitySnapshot {
    pub fn any_available(&self) -> bool {
        self.whisper_available
            || self.parakeet_available
            || (self.soniox_selected && self.soniox_ready)
            || self.remote_available
    }
}

pub(crate) fn remote_availability_from_settings(
    remote_settings: Option<&RemoteSettings>,
) -> (bool, ConnectionStatus, u64, bool) {
    let Some(remote_settings) = remote_settings else {
        return (false, ConnectionStatus::Unknown, 0, false);
    };

    let Some(connection) = remote_settings.get_active_connection() else {
        return (false, ConnectionStatus::Unknown, 0, false);
    };

    let remote_status = connection.status.clone();
    let remote_last_checked = connection.last_checked;
    let remote_available = matches!(remote_status, ConnectionStatus::Online);

    (true, remote_status, remote_last_checked, remote_available)
}

pub async fn emit_recognition_availability(
    app: &tauri::AppHandle,
) -> RecognitionAvailabilitySnapshot {
    let availability = recognition_availability_snapshot(app).await;

    if let Err(err) = app.emit("recognition-availability", availability.clone()) {
        log::warn!("Failed to emit recognition availability event: {}", err);
    }

    availability
}

/// Get a snapshot of which recognition engines are available
pub async fn recognition_availability_snapshot(
    app: &tauri::AppHandle,
) -> RecognitionAvailabilitySnapshot {
    let whisper_available =
        if let Some(manager) = app.try_state::<AsyncRwLock<whisper::manager::WhisperManager>>() {
            manager.read().await.has_downloaded_models()
        } else {
            false
        };

    let parakeet_available =
        if let Some(parakeet_manager) = app.try_state::<parakeet::ParakeetManager>() {
            parakeet_manager
                .list_models()
                .into_iter()
                .any(|model| model.downloaded)
        } else {
            false
        };

    let (soniox_selected, soniox_ready) = match app.store("settings") {
        Ok(store) => {
            let engine = store
                .get("current_model_engine")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "whisper".to_string());

            if engine == "soniox" {
                let has_key =
                    crate::secure_store::secure_has(app, "stt_api_key_soniox").unwrap_or(false);
                (true, has_key)
            } else {
                (false, false)
            }
        }
        Err(_) => (false, false),
    };

    let (remote_selected, remote_status, remote_last_checked, remote_available) =
        if let Some(remote_settings) = app.try_state::<AsyncMutex<RemoteSettings>>() {
            let settings = remote_settings.lock().await;
            remote_availability_from_settings(Some(&settings))
        } else {
            remote_availability_from_settings(None)
        };

    RecognitionAvailabilitySnapshot {
        whisper_available,
        parakeet_available,
        soniox_selected,
        soniox_ready,
        remote_selected,
        remote_status,
        remote_last_checked,
        remote_available,
    }
}

#[tauri::command]
pub async fn get_recognition_availability_snapshot(
    app: tauri::AppHandle,
 ) -> Result<RecognitionAvailabilitySnapshot, String> {
    Ok(recognition_availability_snapshot(&app).await)
}

fn pick_best_parakeet_model(models: Vec<parakeet::ParakeetModelStatus>) -> Option<String> {
    let mut downloaded: Vec<_> = models.into_iter().filter(|m| m.downloaded).collect();
    downloaded.sort_by(|a, b| {
        b.recommended
            .cmp(&a.recommended)
            .then(b.accuracy_score.cmp(&a.accuracy_score))
            .then(a.size.cmp(&b.size))
    });
    downloaded.first().map(|m| m.name.clone())
}

async fn pick_best_whisper_model(
    manager: &AsyncRwLock<whisper::manager::WhisperManager>,
) -> Option<String> {
    let manager = manager.read().await;
    let mut downloaded: Vec<_> = manager
        .get_models_status()
        .into_iter()
        .filter(|(_, info)| info.downloaded)
        .collect();
    downloaded.sort_by(|a, b| {
        b.1.recommended
            .cmp(&a.1.recommended)
            .then(b.1.accuracy_score.cmp(&a.1.accuracy_score))
            .then(a.1.size.cmp(&b.1.size))
    });
    downloaded.first().map(|(name, _)| name.clone())
}

/// Auto-select the best available model if none is currently selected
pub async fn auto_select_model_if_needed(
    app: &tauri::AppHandle,
    availability: &RecognitionAvailabilitySnapshot,
) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let onboarding_completed = store
        .get("onboarding_completed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let current_model = store
        .get("current_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    if !current_model.is_empty() {
        if !onboarding_completed && availability.any_available() {
            store.set("onboarding_completed", serde_json::Value::Bool(true));
            store.save().map_err(|e| e.to_string())?;
            if let Err(e) = app.emit("settings-changed", ()) {
                log::warn!("Failed to emit settings-changed after onboarding repair: {}", e);
            }
        }
        return Ok(());
    }

    let mut selection: Option<(String, String)> = None;

    if availability.parakeet_available {
        if let Some(parakeet_manager) = app.try_state::<parakeet::ParakeetManager>() {
            if let Some(model) = pick_best_parakeet_model(parakeet_manager.list_models()) {
                selection = Some(("parakeet".to_string(), model));
            }
        }
    }

    if selection.is_none() && availability.whisper_available {
        if let Some(whisper_state) =
            app.try_state::<AsyncRwLock<whisper::manager::WhisperManager>>()
        {
            if let Some(model) = pick_best_whisper_model(&whisper_state).await {
                selection = Some(("whisper".to_string(), model));
            }
        }
    }

    if selection.is_none() && availability.soniox_selected && availability.soniox_ready {
        selection = Some(("soniox".to_string(), "soniox".to_string()));
    }

    let Some((engine, model)) = selection else {
        if availability.remote_available {
            store.set("onboarding_completed", serde_json::Value::Bool(true));
            store.save().map_err(|e| e.to_string())?;
            if let Err(e) = app.emit("settings-changed", ()) {
                log::warn!("Failed to emit settings-changed after remote onboarding completion: {}", e);
            }

            log::info!(
                "Marked onboarding complete because an active remote server is available"
            );
        }

        return Ok(());
    };

    store.set("current_model", serde_json::Value::String(model.clone()));
    store.set(
        "current_model_engine",
        serde_json::Value::String(engine.clone()),
    );
    store.set("onboarding_completed", serde_json::Value::Bool(true));
    store.save().map_err(|e| e.to_string())?;
    if let Err(e) = app.emit("settings-changed", ()) {
        log::warn!("Failed to emit settings-changed after auto-selection: {}", e);
    }

    log::info!(
        "Auto-selected {} model '{}' based on availability snapshot",
        engine,
        model
    );

    if let Err(e) = app.emit(
        "model-auto-selected",
        serde_json::json!({ "engine": engine, "model": model }),
    ) {
        log::warn!("Failed to emit model auto-selection event: {}", e);
    }

    let app_for_tray = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::commands::settings::update_tray_menu(app_for_tray.clone()).await {
            log::warn!("Failed to refresh tray menu after auto-selection: {}", e);
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::RecognitionAvailabilitySnapshot;
    use crate::remote::settings::{ConnectionStatus, RemoteSettings};
    #[test]
    fn remote_availability_snapshot_reports_no_active_remote_as_unresolved() {
        let settings = RemoteSettings::default();

        let (remote_selected, remote_status, remote_last_checked, remote_available) =
            super::remote_availability_from_settings(Some(&settings));

        assert!(!remote_selected);
        assert_eq!(remote_status, ConnectionStatus::Unknown);
        assert_eq!(remote_last_checked, 0);
        assert!(!remote_available);
    }

    #[test]
    fn remote_availability_snapshot_reports_structured_remote_state() {
        let mut settings = RemoteSettings::default();
        let conn = settings.add_connection(
            "192.168.1.10".to_string(),
            47842,
            None,
            Some("Remote".to_string()),
            None,
        );
        let conn_id = conn.id.clone();
        settings.set_active_connection(Some(conn_id.clone())).unwrap();
        let active = settings
            .saved_connections
            .iter_mut()
            .find(|c| c.id == conn_id)
            .expect("active connection should exist");
        active.status = ConnectionStatus::Unknown;
        active.last_checked = 1_717_000_000_000;

        let (remote_selected, remote_status, remote_last_checked, remote_available) =
            super::remote_availability_from_settings(Some(&settings));

        assert!(remote_selected);
        assert_eq!(remote_status, ConnectionStatus::Unknown);
        assert_eq!(remote_last_checked, 1_717_000_000_000);
        assert!(!remote_available);
    }


    #[test]
    fn any_available_is_true_when_authenticated_remote_is_available() {
        let snapshot = RecognitionAvailabilitySnapshot {
            whisper_available: false,
            parakeet_available: false,
            soniox_selected: false,
            soniox_ready: false,
            remote_selected: true,
            remote_status: ConnectionStatus::Online,
            remote_last_checked: 1,
            remote_available: true,
        };

        assert!(snapshot.any_available());
    }

    #[test]
    fn any_available_is_false_when_remote_is_only_selected() {
        let snapshot = RecognitionAvailabilitySnapshot {
            whisper_available: false,
            parakeet_available: false,
            soniox_selected: false,
            soniox_ready: false,
            remote_selected: true,
            remote_status: ConnectionStatus::Unknown,
            remote_last_checked: 1,
            remote_available: false,
        };

        assert!(!snapshot.any_available());
    }

    #[test]
    fn any_available_is_false_when_nothing_is_ready() {
        let snapshot = RecognitionAvailabilitySnapshot {
            whisper_available: false,
            parakeet_available: false,
            soniox_selected: false,
            soniox_ready: false,
            remote_selected: false,
            remote_status: ConnectionStatus::Unknown,
            remote_last_checked: 0,
            remote_available: false,
        };

        assert!(!snapshot.any_available());
    }
}
