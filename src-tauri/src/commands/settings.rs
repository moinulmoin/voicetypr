use crate::{AppState, RecordingState};
use crate::commands::key_normalizer::{normalize_shortcut_keys, validate_key_combination};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Manager, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use tauri_plugin_store::StoreExt;
use crate::whisper::languages::{SUPPORTED_LANGUAGES, validate_language};

#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    pub hotkey: String,
    pub current_model: String,
    pub language: String,
    pub translate_to_english: bool,
    pub theme: String,
    pub transcription_cleanup_days: Option<u32>,
    pub pill_position: Option<(f64, f64)>,
    pub launch_at_startup: bool,
    pub onboarding_completed: bool,
    pub compact_recording_status: bool,
    pub check_updates_automatically: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "CommandOrControl+Shift+Space".to_string(),
            current_model: "".to_string(), // Empty means auto-select
            language: "en".to_string(),
            translate_to_english: false,      // Default to transcribe mode
            theme: "system".to_string(),
            transcription_cleanup_days: None, // None means keep forever
            pill_position: None,              // No saved position initially
            launch_at_startup: false,         // Default to not launching at startup
            onboarding_completed: false,      // Default to not completed
            compact_recording_status: true,   // Default to compact mode
            check_updates_automatically: true, // Default to automatic updates enabled
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
        translate_to_english: store
            .get("translate_to_english")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().translate_to_english),
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
        check_updates_automatically: store
            .get("check_updates_automatically")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().check_updates_automatically),
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
    
    // Validate language before saving
    let validated_language = validate_language(Some(&settings.language));
    store.set("language", json!(validated_language));
    store.set("translate_to_english", json!(settings.translate_to_english));
    
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
    store.set(
        "check_updates_automatically",
        json!(settings.check_updates_automatically),
    );

    // Save pill position if provided
    if let Some((x, y)) = settings.pill_position {
        store.set("pill_position", json!([x, y]));
    }

    store.save().map_err(|e| e.to_string())?;

    // Preload new model if it changed
    if !settings.current_model.is_empty() && old_model != settings.current_model {
        use crate::commands::model::preload_model;
        use tauri::async_runtime::RwLock as AsyncRwLock;

        log::info!(
            "Model changed from '{}' to '{}', preloading new model",
            old_model,
            settings.current_model
        );

        let app_clone = app.clone();
        let model_name = settings.current_model.clone();
        tokio::spawn(async move {
            let whisper_state =
                app_clone.state::<AsyncRwLock<crate::whisper::manager::WhisperManager>>();
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
    
    // Validate key combination
    validate_key_combination(&shortcut)?;
    
    // Normalize the shortcut keys
    let normalized_shortcut = normalize_shortcut_keys(&shortcut);
    
    // Validate that shortcut can be parsed
    let _new_shortcut: Shortcut = normalized_shortcut
        .parse()
        .map_err(|_| format!("Invalid shortcut format: {}", normalized_shortcut))?;

    // Store the pending shortcut in app state (normalized version)
    let app_state = app.state::<AppState>();
    
    match app_state.pending_shortcut.lock() {
        Ok(mut pending_guard) => {
            *pending_guard = Some(normalized_shortcut.clone());
            log::info!("Pending shortcut set to: {} (normalized from: {})", normalized_shortcut, shortcut);
        }
        Err(e) => {
            log::error!("Failed to acquire pending shortcut lock: {}", e);
            return Err("Failed to update shortcut state".to_string());
        }
    }

    // Save to settings (original version for display)
    let store = app.store("settings").map_err(|e| e.to_string())?;
    store.set("hotkey", json!(shortcut));
    store.save().map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn apply_pending_shortcut(app: AppHandle) -> Result<bool, String> {
    let app_state = app.state::<AppState>();
    
    // Check if we're in idle state (safe to update shortcuts)
    let current_state = app_state.get_current_state();
    if current_state != RecordingState::Idle {
        log::info!("Cannot apply shortcut while recording state is: {:?}", current_state);
        return Ok(false);
    }
    
    // Check if there's a pending shortcut
    let pending_shortcut = {
        let pending_guard = app_state.pending_shortcut.lock()
            .map_err(|e| format!("Failed to lock pending shortcut: {}", e))?;
        pending_guard.clone()
    };
    
    if let Some(shortcut_str) = pending_shortcut {
        log::info!("Applying pending shortcut: {}", shortcut_str);
        
        let shortcuts = app.global_shortcut();
        
        // Unregister all existing shortcuts
        shortcuts.unregister_all().map_err(|e| e.to_string())?;
        
        // Parse and register new shortcut
        let shortcut_obj: Shortcut = shortcut_str
            .parse()
            .map_err(|_| "Invalid shortcut format".to_string())?;
        shortcuts
            .register(shortcut_obj.clone())
            .map_err(|e| e.to_string())?;
        
        // Update the recording shortcut in managed state
        match app_state.recording_shortcut.lock() {
            Ok(mut shortcut_guard) => {
                *shortcut_guard = Some(shortcut_obj);
            }
            Err(e) => {
                log::error!("Failed to acquire recording shortcut lock: {}", e);
                return Err("Failed to update recording shortcut".to_string());
            }
        }
        
        // Clear the pending shortcut
        match app_state.pending_shortcut.lock() {
            Ok(mut pending_guard) => {
                *pending_guard = None;
            }
            Err(e) => {
                log::error!("Failed to acquire pending shortcut lock: {}", e);
                // Don't fail here as the shortcut was already applied
                log::warn!("Continuing despite lock failure");
            }
        }
        
        log::info!("Successfully applied shortcut: {}", shortcut_str);
        return Ok(true);
    }
    
    Ok(false)
}

#[derive(Serialize)]
pub struct LanguageInfo {
    pub code: String,
    pub name: String,
}

#[tauri::command]
pub async fn get_supported_languages() -> Result<Vec<LanguageInfo>, String> {
    let mut languages: Vec<LanguageInfo> = SUPPORTED_LANGUAGES
        .iter()
        .map(|(code, lang)| LanguageInfo {
            code: code.to_string(),
            name: lang.name.to_string(),
        })
        .collect();
    
    // Sort by name for better UX (auto-detect removed)
    languages.sort_by(|a, b| a.name.cmp(&b.name));
    
    Ok(languages)
}

#[tauri::command]
pub async fn set_model_from_tray(app: AppHandle, model_name: String) -> Result<(), String> {
    // Get current settings
    let mut settings = get_settings(app.clone()).await?;
    
    // Update the model
    settings.current_model = model_name.clone();
    
    // Save settings (this will also preload the model)
    save_settings(app.clone(), settings).await?;
    
    // Update the tray menu to reflect the new selection
    update_tray_menu(app.clone()).await?;
    
    // Emit event to update UI only after successful tray menu update
    if let Err(e) = app.emit("model-changed", &model_name) {
        log::warn!("Failed to emit model-changed event: {}", e);
        // Return error to caller so they know the UI might be out of sync
        return Err(format!("Failed to emit model-changed event: {}", e));
    }
    
    Ok(())
}

#[tauri::command]
pub async fn set_language_from_tray(app: AppHandle, language_code: String) -> Result<(), String> {
    // Get current settings
    let mut settings = get_settings(app.clone()).await?;
    
    // Update the language
    settings.language = language_code.clone();
    
    // Save settings
    save_settings(app.clone(), settings).await?;
    
    // Update the tray menu to reflect the new selection
    update_tray_menu(app.clone()).await?;
    
    // Emit event to update UI only after successful tray menu update
    if let Err(e) = app.emit("language-changed", &language_code) {
        log::warn!("Failed to emit language-changed event: {}", e);
        // Return error to caller so they know the UI might be out of sync
        return Err(format!("Failed to emit language-changed event: {}", e));
    }
    
    Ok(())
}

#[tauri::command]
pub async fn update_tray_menu(app: AppHandle) -> Result<(), String> {
    // Build the new menu
    let new_menu = crate::build_tray_menu(&app).await
        .map_err(|e| format!("Failed to build tray menu: {}", e))?;
    
    // Update the tray menu
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(new_menu))
            .map_err(|e| format!("Failed to set tray menu: {}", e))?;
        log::info!("Tray menu updated successfully");
    } else {
        log::warn!("Tray icon not found");
    }
    
    Ok(())
}
