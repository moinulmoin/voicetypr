use crate::AppState;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use tauri_plugin_store::StoreExt;

#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    pub hotkey: String,
    pub current_model: String,
    pub language: String,
    pub theme: String,
    pub transcription_cleanup_days: Option<u32>,
    pub pill_position: Option<(f64, f64)>,
    pub launch_at_startup: bool,
    pub onboarding_completed: bool,
    pub compact_recording_status: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "CommandOrControl+Shift+Space".to_string(),
            current_model: "".to_string(), // Empty means auto-select
            language: "en".to_string(),
            theme: "system".to_string(),
            transcription_cleanup_days: None, // None means keep forever
            pill_position: None,              // No saved position initially
            launch_at_startup: false,         // Default to not launching at startup
            onboarding_completed: false,      // Default to not completed
            compact_recording_status: true,   // Default to compact mode
        }
    }
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Result<Settings, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;

    let settings = Settings {
        hotkey: store
            .get("hotkey")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().hotkey),
        current_model: store
            .get("current_model")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().current_model),
        language: store
            .get("language")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().language),
        theme: store
            .get("theme")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().theme),
        transcription_cleanup_days: store
            .get("transcription_cleanup_days")
            .and_then(|v| v.as_u64().map(|n| n as u32)),
        pill_position: store.get("pill_position").and_then(|v| {
            if let Some(arr) = v.as_array() {
                if arr.len() == 2 {
                    let x = arr[0].as_f64()?;
                    let y = arr[1].as_f64()?;
                    Some((x, y))
                } else {
                    None
                }
            } else {
                None
            }
        }),
        launch_at_startup: store
            .get("launch_at_startup")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().launch_at_startup),
        onboarding_completed: store
            .get("onboarding_completed")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().onboarding_completed),
        compact_recording_status: store
            .get("compact_recording_status")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().compact_recording_status),
    };

    // Pill position is already loaded from store, no need for duplicate state

    Ok(settings)
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;

    // Check if model changed
    let old_model = store
        .get("current_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    store.set("hotkey", json!(settings.hotkey));
    store.set("current_model", json!(settings.current_model));
    store.set("language", json!(settings.language));
    store.set("theme", json!(settings.theme));
    store.set(
        "transcription_cleanup_days",
        json!(settings.transcription_cleanup_days),
    );
    store.set("launch_at_startup", json!(settings.launch_at_startup));
    store.set("onboarding_completed", json!(settings.onboarding_completed));
    store.set(
        "compact_recording_status",
        json!(settings.compact_recording_status),
    );

    // Save pill position if provided
    if let Some((x, y)) = settings.pill_position {
        store.set("pill_position", json!([x, y]));
    }

    store.save().map_err(|e| e.to_string())?;

    // Preload new model if it changed
    if !settings.current_model.is_empty() && old_model != settings.current_model {
        use crate::commands::model::preload_model;
        use tauri::async_runtime::Mutex as AsyncMutex;

        log::info!(
            "Model changed from '{}' to '{}', preloading new model",
            old_model,
            settings.current_model
        );

        let app_clone = app.clone();
        let model_name = settings.current_model.clone();
        tokio::spawn(async move {
            let whisper_state =
                app_clone.state::<AsyncMutex<crate::whisper::manager::WhisperManager>>();
            match preload_model(app_clone.clone(), model_name.clone(), whisper_state).await {
                Ok(_) => log::info!("Successfully preloaded new model: {}", model_name),
                Err(e) => log::warn!("Failed to preload new model: {}", e),
            }
        });
    }

    Ok(())
}

#[tauri::command]
pub async fn set_global_shortcut(app: AppHandle, shortcut: String) -> Result<(), String> {
    // Validate shortcut format
    if shortcut.is_empty() || shortcut.len() > 100 {
        return Err("Invalid shortcut format".to_string());
    }
    let shortcuts = app.global_shortcut();

    // Unregister all existing shortcuts
    shortcuts.unregister_all().map_err(|e| e.to_string())?;

    // Register new shortcut
    let shortcut_obj: Shortcut = shortcut
        .parse()
        .map_err(|_| "Invalid shortcut format".to_string())?;
    shortcuts
        .register(shortcut_obj.clone())
        .map_err(|e| e.to_string())?;

    // Update the recording shortcut in managed state
    let app_state = app.state::<AppState>();
    if let Ok(mut shortcut_guard) = app_state.recording_shortcut.lock() {
        *shortcut_guard = Some(shortcut_obj);
    }

    // Save to settings
    let store = app.store("settings").map_err(|e| e.to_string())?;
    store.set("hotkey", json!(shortcut));
    store.save().map_err(|e| e.to_string())?;

    Ok(())
}
