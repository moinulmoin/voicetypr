use crate::commands::key_normalizer::{normalize_shortcut_keys, validate_key_combination};
use crate::whisper::languages::{validate_language, SUPPORTED_LANGUAGES};
use crate::AppState;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use tauri_plugin_store::StoreExt;

#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    pub hotkey: String,
    pub current_model: String,
    pub current_model_engine: String,
    pub language: String,
    pub translate_to_english: bool,
    pub theme: String,
    pub transcription_cleanup_days: Option<u32>,
    pub pill_position: Option<(f64, f64)>,
    pub launch_at_startup: bool,
    pub onboarding_completed: bool,
    pub compact_recording_status: bool,
    pub check_updates_automatically: bool,
    pub selected_microphone: Option<String>,
    // Push-to-talk support
    pub recording_mode: String, // "toggle" or "push_to_talk"
    pub use_different_ptt_key: bool,
    pub ptt_hotkey: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "CommandOrControl+Shift+Space".to_string(),
            current_model: "".to_string(), // Empty means auto-select
            current_model_engine: "whisper".to_string(),
            language: "en".to_string(),
            translate_to_english: false, // Default to transcribe mode
            theme: "system".to_string(),
            transcription_cleanup_days: None, // None means keep forever
            pill_position: None,              // No saved position initially
            launch_at_startup: false,         // Default to not launching at startup
            onboarding_completed: false,      // Default to not completed
            compact_recording_status: true,   // Default to compact mode
            check_updates_automatically: true, // Default to automatic updates enabled
            selected_microphone: None,        // Default to system default microphone
            recording_mode: "toggle".to_string(), // Default to toggle mode for backward compatibility
            use_different_ptt_key: false,         // Default to using same key
            ptt_hotkey: Some("Alt+Space".to_string()), // Default PTT key
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
        current_model_engine: store
            .get("current_model_engine")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().current_model_engine.clone()),
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
        selected_microphone: store
            .get("selected_microphone")
            .and_then(|v| v.as_str().map(|s| s.to_string())),
        recording_mode: store
            .get("recording_mode")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().recording_mode),
        use_different_ptt_key: store
            .get("use_different_ptt_key")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().use_different_ptt_key),
        ptt_hotkey: store
            .get("ptt_hotkey")
            .and_then(|v| v.as_str().map(|s| s.to_string())),
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
    store.set("current_model_engine", json!(settings.current_model_engine));

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
    store.set("selected_microphone", json!(settings.selected_microphone));

    // Save push-to-talk settings
    store.set("recording_mode", json!(settings.recording_mode.clone()));
    store.set(
        "use_different_ptt_key",
        json!(settings.use_different_ptt_key),
    );
    if let Some(ref ptt_hotkey) = settings.ptt_hotkey {
        store.set("ptt_hotkey", json!(ptt_hotkey));
    }

    // Save pill position if provided
    if let Some((x, y)) = settings.pill_position {
        store.set("pill_position", json!([x, y]));
    }

    store.save().map_err(|e| e.to_string())?;

    // Update recording mode in AppState
    let app_state = app.state::<crate::AppState>();
    let recording_mode = match settings.recording_mode.as_str() {
        "push_to_talk" => crate::RecordingMode::PushToTalk,
        _ => crate::RecordingMode::Toggle,
    };

    if let Ok(mut mode_guard) = app_state.recording_mode.lock() {
        *mode_guard = recording_mode;
        log::info!("Recording mode updated to: {:?}", recording_mode);
    }

    // Handle PTT shortcut registration if needed
    if recording_mode == crate::RecordingMode::PushToTalk && settings.use_different_ptt_key {
        if let Some(ptt_hotkey) = settings.ptt_hotkey.clone() {
            let normalized_ptt =
                crate::commands::key_normalizer::normalize_shortcut_keys(&ptt_hotkey);

            if let Ok(ptt_shortcut) =
                normalized_ptt.parse::<tauri_plugin_global_shortcut::Shortcut>()
            {
                let shortcuts = app.global_shortcut();

                // Unregister old PTT shortcut if exists
                if let Ok(ptt_guard) = app_state.ptt_shortcut.lock() {
                    if let Some(old_ptt) = ptt_guard.clone() {
                        let _ = shortcuts.unregister(old_ptt);
                    }
                }

                // Register new PTT shortcut
                match shortcuts.register(ptt_shortcut.clone()) {
                    Ok(_) => {
                        if let Ok(mut ptt_guard) = app_state.ptt_shortcut.lock() {
                            *ptt_guard = Some(ptt_shortcut);
                        }
                        log::info!("PTT shortcut updated to: {}", ptt_hotkey);
                    }
                    Err(e) => {
                        log::error!("Failed to register PTT shortcut: {}", e);
                    }
                }
            }
        }
    } else {
        // Clear PTT shortcut if not using different key
        if let Ok(mut ptt_guard) = app_state.ptt_shortcut.lock() {
            if let Some(old_ptt) = ptt_guard.clone() {
                let _ = app.global_shortcut().unregister(old_ptt);
            }
            *ptt_guard = None;
        }
    }

    // Invalidate recording config cache when settings change
    crate::commands::audio::invalidate_recording_config_cache(&app).await;

    // Preload new model and update tray menu if model changed
    let is_parakeet_engine = settings.current_model_engine == "parakeet";

    if !settings.current_model.is_empty() && old_model != settings.current_model {
        use crate::commands::model::preload_model;
        use tauri::async_runtime::RwLock as AsyncRwLock;

        log::info!(
            "Model changed from '{}' to '{}', preloading new model and updating tray menu",
            old_model,
            settings.current_model
        );

        if !is_parakeet_engine {
            // Preload the new Whisper model
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
        } else {
            log::info!("Skipping Whisper preload for Parakeet engine selection");
        }

        // Update the tray menu to reflect the new selection
        if let Err(e) = update_tray_menu(app.clone()).await {
            log::warn!("Failed to update tray menu after model change: {}", e);
            // Don't fail the whole operation if tray update fails
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn set_global_shortcut(app: AppHandle, shortcut: String) -> Result<(), String> {
    log::info!("Updating global shortcut to: {}", shortcut);

    // Validate shortcut format
    if shortcut.is_empty() || shortcut.len() > 100 {
        log::error!("Invalid shortcut format: empty or too long");
        return Err("Invalid shortcut format".to_string());
    }

    // Validate key combination
    if let Err(e) = validate_key_combination(&shortcut) {
        log::error!("Invalid key combination '{}': {}", shortcut, e);
        return Err(format!("Invalid key combination: {}", e));
    }

    // Normalize the shortcut keys
    let normalized_shortcut = normalize_shortcut_keys(&shortcut);
    log::debug!(
        "Normalized shortcut: {} -> {}",
        shortcut,
        normalized_shortcut
    );

    // Validate that shortcut can be parsed
    let new_shortcut: Shortcut = normalized_shortcut.parse().map_err(|e| {
        log::error!(
            "Failed to parse normalized shortcut '{}': {}",
            normalized_shortcut,
            e
        );
        "Invalid shortcut format".to_string()
    })?;

    // Get global shortcut manager and app state
    let shortcuts = app.global_shortcut();
    let app_state = app.state::<AppState>();

    // Unregister only the current recording shortcut (not ESC or others)
    log::debug!("Unregistering current recording shortcut if exists");
    let old_shortcut = app_state
        .recording_shortcut
        .lock()
        .ok()
        .and_then(|guard| guard.clone());

    if let Some(old) = old_shortcut {
        log::debug!("Unregistering old shortcut: {:?}", old);
        if let Err(e) = shortcuts.unregister(old) {
            log::warn!(
                "Failed to unregister old shortcut: {}. Continuing anyway.",
                e
            );
            // Don't fail - the old shortcut might already be unregistered
        }
    }

    // Register new shortcut immediately
    log::debug!("Registering new shortcut: {}", normalized_shortcut);

    // Attempt registration - according to docs, ANY error means hotkey won't work
    let registration_result = shortcuts.register(new_shortcut.clone());

    match registration_result {
        Ok(_) => {
            log::info!("Successfully registered hotkey: {}", normalized_shortcut);
            // Hotkey registered successfully, no conflicts
        }
        Err(e) => {
            let error_msg = e.to_string();
            let error_lower = error_msg.to_lowercase();

            // According to tauri-plugin-global-shortcut docs:
            // If register() returns an error, the shortcut is NOT functional
            // Registration is atomic - it either succeeds completely or fails
            log::error!("Failed to register hotkey '{}': {}", normalized_shortcut, e);

            // Provide helpful error message based on error type
            let detailed_error = if error_lower.contains("already registered")
                || error_lower.contains("conflict")
                || error_lower.contains("in use")
            {
                format!("Hotkey is already in use by another application. Please choose a different combination.")
            } else if error_lower.contains("parse") || error_lower.contains("invalid") {
                format!("Invalid hotkey combination. Please use a valid key combination.")
            } else {
                format!("Failed to register hotkey: {}", e)
            };

            return Err(detailed_error);
        }
    }

    // Update the recording shortcut in managed state regardless of registration warnings
    match app_state.recording_shortcut.lock() {
        Ok(mut shortcut_guard) => {
            *shortcut_guard = Some(new_shortcut);
            log::debug!("Updated recording shortcut state");
        }
        Err(e) => {
            log::error!("Failed to acquire recording shortcut lock: {}", e);
            // Continue anyway since the hotkey might be registered
            log::warn!("Continuing despite lock failure");
        }
    }

    // Save to settings (original version for display)
    let store = app.store("settings").map_err(|e| {
        log::error!("Failed to get settings store: {}", e);
        "Failed to access settings store".to_string()
    })?;

    store.set("hotkey", json!(shortcut));
    if let Err(e) = store.save() {
        log::error!("Failed to save settings: {}", e);
        // The shortcut is already registered, so this isn't a critical failure
        log::warn!("Shortcut registered but settings save failed");
    }

    log::info!("Successfully updated global shortcut to: {}", shortcut);

    Ok(())
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
    settings.current_model_engine = "whisper".to_string();

    // Save settings (this will also preload the model)
    save_settings(app.clone(), settings).await?;

    // Update the tray menu to reflect the new selection
    update_tray_menu(app.clone()).await?;

    // Emit event to update UI only after successful tray menu update
    if let Err(e) = app.emit(
        "model-changed",
        json!({
            "model": model_name,
            "engine": "whisper"
        }),
    ) {
        log::warn!("Failed to emit model-changed event: {}", e);
        // Return error to caller so they know the UI might be out of sync
        return Err(format!("Failed to emit model-changed event: {}", e));
    }

    Ok(())
}

#[tauri::command]
pub async fn update_tray_menu(app: AppHandle) -> Result<(), String> {
    // Build the new menu
    let new_menu = crate::build_tray_menu(&app)
        .await
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

/// Set the selected microphone device
#[tauri::command]
pub async fn set_audio_device(app: AppHandle, device_name: Option<String>) -> Result<(), String> {
    log::info!("Setting audio device to: {:?}", device_name);

    // Get current settings
    let mut settings = get_settings(app.clone()).await?;

    // Check if recording is in progress and stop it
    let recorder_state = app.state::<crate::commands::audio::RecorderState>();
    {
        let mut recorder = recorder_state
            .inner()
            .0
            .lock()
            .map_err(|e| format!("Failed to acquire recorder lock: {}", e))?;

        if recorder.is_recording() {
            log::info!("Recording in progress, stopping it before changing microphone");

            // Update state to notify UI
            crate::update_recording_state(&app, crate::RecordingState::Stopping, None);

            match recorder.stop_recording() {
                Ok(msg) => {
                    log::info!("Recording stopped: {}", msg);
                    // Update state to idle after successful stop
                    crate::update_recording_state(&app, crate::RecordingState::Idle, None);
                }
                Err(e) => {
                    log::warn!("Failed to stop recording: {}", e);
                    // Update state to error if stop failed
                    crate::update_recording_state(&app, crate::RecordingState::Error, Some(e));
                }
            }
        }
    } // Lock released here

    // Update the selected microphone
    settings.selected_microphone = device_name.clone();

    // Save the updated settings
    save_settings(app.clone(), settings).await?;

    // Update tray menu to reflect the change
    update_tray_menu(app.clone()).await?;

    // Emit event to notify frontend - just emit a signal, frontend will reload settings
    if let Err(e) = app.emit("audio-device-changed", ()) {
        log::warn!("Failed to emit audio-device-changed event: {}", e);
    }

    log::info!("Audio device successfully set to: {:?}", device_name);
    Ok(())
}
