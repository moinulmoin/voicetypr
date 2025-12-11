use tauri::async_runtime::RwLock as AsyncRwLock;
use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::parakeet;
use crate::whisper;

/// Snapshot of recognition engine availability
#[derive(Debug, Clone, serde::Serialize)]
pub struct RecognitionAvailabilitySnapshot {
    pub whisper_available: bool,
    pub parakeet_available: bool,
    pub soniox_selected: bool,
    pub soniox_ready: bool,
}

impl RecognitionAvailabilitySnapshot {
    pub fn any_available(&self) -> bool {
        self.whisper_available
            || self.parakeet_available
            || (self.soniox_selected && self.soniox_ready)
    }
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

    RecognitionAvailabilitySnapshot {
        whisper_available,
        parakeet_available,
        soniox_selected,
        soniox_ready,
    }
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
    let current_model = store
        .get("current_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    if !current_model.is_empty() {
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
        return Ok(());
    };

    store.set("current_model", serde_json::Value::String(model.clone()));
    store.set(
        "current_model_engine",
        serde_json::Value::String(engine.clone()),
    );
    store.set("onboarding_completed", serde_json::Value::Bool(true));
    store.save().map_err(|e| e.to_string())?;

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
