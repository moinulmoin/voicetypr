use crate::audio::device_watcher::try_start_device_watcher_if_ready;
use crate::commands::key_normalizer::{normalize_shortcut_keys, validate_key_combination};
use crate::commands::remote::save_remote_settings;
use crate::parakeet::ParakeetManager;
use crate::remote::lifecycle::RemoteServerManager;
use crate::remote::settings::RemoteSettings;
use crate::whisper::languages::{validate_language, SUPPORTED_LANGUAGES};
use crate::whisper::manager::WhisperManager;
use crate::AppState;
use tauri::async_runtime::Mutex as AsyncMutex;
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
    pub check_updates_automatically: bool,
    pub selected_microphone: Option<String>,
    // Push-to-talk support
    pub recording_mode: String, // "toggle" or "push_to_talk"
    pub use_different_ptt_key: bool,
    pub ptt_hotkey: Option<String>,
    pub keep_transcription_in_clipboard: bool,
    // Audio feedback
    pub play_sound_on_recording: bool,
    pub play_sound_on_recording_end: bool,
    // Pill indicator visibility mode: "never", "always", or "when_recording"
    pub pill_indicator_mode: String,
    // Pill indicator screen position: "top", "center", or "bottom"
    pub pill_indicator_position: String,
    // Network sharing settings
    pub sharing_port: Option<u16>,
    pub sharing_password: Option<String>,
    // Recording persistence settings
    pub save_recordings: bool,
    pub recording_retention_count: Option<u32>, // None = unlimited
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
            check_updates_automatically: true, // Default to automatic updates enabled
            selected_microphone: None,        // Default to system default microphone
            recording_mode: "toggle".to_string(), // Default to toggle mode for backward compatibility
            use_different_ptt_key: false,         // Default to using same key
            ptt_hotkey: Some("Alt+Space".to_string()), // Default PTT key
            keep_transcription_in_clipboard: false, // Default to restoring clipboard after paste
            play_sound_on_recording: true,        // Default to playing sound on recording start
            play_sound_on_recording_end: true,    // Default to playing sound on recording end
            pill_indicator_mode: "when_recording".to_string(), // Default to showing only when recording
            pill_indicator_position: "bottom".to_string(), // Default to bottom of screen
            sharing_port: Some(47842), // Default network sharing port
            sharing_password: None,    // No password by default
            save_recordings: true,     // Default to saving recordings
            recording_retention_count: Some(25), // Default to keeping 25 recordings
        }
    }
}

/// Log a message from the frontend to the backend logs
#[tauri::command]
pub async fn frontend_log(message: String) {
    log::info!("[FRONTEND] {}", message);
}

pub(crate) fn resolve_pill_indicator_mode(
    stored_mode: Option<String>,
    legacy_show_pill: Option<bool>,
    default_mode: String,
) -> String {
    if let Some(mode) = stored_mode {
        return mode;
    }

    if let Some(show) = legacy_show_pill {
        if show {
            return "always".to_string();
        }
        return "when_recording".to_string();
    }

    default_mode
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
        keep_transcription_in_clipboard: store
            .get("keep_transcription_in_clipboard")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().keep_transcription_in_clipboard),
        play_sound_on_recording: store
            .get("play_sound_on_recording")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().play_sound_on_recording),
        play_sound_on_recording_end: store
            .get("play_sound_on_recording_end")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().play_sound_on_recording_end),
        // Migration: check for new pill_indicator_mode first, then fall back to old show_pill_indicator
        pill_indicator_mode: resolve_pill_indicator_mode(
            store
                .get("pill_indicator_mode")
                .and_then(|v| v.as_str().map(|s| s.to_string())),
            store.get("show_pill_indicator").and_then(|v| v.as_bool()),
            Settings::default().pill_indicator_mode,
        ),
        pill_indicator_position: store
            .get("pill_indicator_position")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().pill_indicator_position),
        sharing_port: store
            .get("sharing_port")
            .and_then(|v| v.as_u64().map(|n| n as u16))
            .or(Settings::default().sharing_port),
        sharing_password: store
            .get("sharing_password")
            .and_then(|v| v.as_str().map(|s| s.to_string())),
        save_recordings: store
            .get("save_recordings")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| Settings::default().save_recordings),
        recording_retention_count: store
            .get("recording_retention_count")
            .and_then(|v| v.as_u64().map(|n| n as u32))
            .or(Settings::default().recording_retention_count),
    };

    Ok(settings)
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;

    // Check if model, recording mode, onboarding, and pill indicator mode changed
    let old_model = store
        .get("current_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    let old_mode = store
        .get("recording_mode")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| Settings::default().recording_mode);
    let old_onboarding_completed = store
        .get("onboarding_completed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let old_pill_indicator_mode = store
        .get("pill_indicator_mode")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| Settings::default().pill_indicator_mode);
    let old_pill_indicator_position = store
        .get("pill_indicator_position")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| Settings::default().pill_indicator_position);

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
    store.set(
        "keep_transcription_in_clipboard",
        json!(settings.keep_transcription_in_clipboard),
    );
    store.set(
        "play_sound_on_recording",
        json!(settings.play_sound_on_recording),
    );
    store.set(
        "play_sound_on_recording_end",
        json!(settings.play_sound_on_recording_end),
    );
    store.set(
        "pill_indicator_mode",
        json!(settings.pill_indicator_mode),
    );
    store.set(
        "pill_indicator_position",
        json!(settings.pill_indicator_position),
    );
    // Network sharing settings
    if let Some(port) = settings.sharing_port {
        store.set("sharing_port", json!(port));
    }
    if let Some(ref pwd) = settings.sharing_password {
        store.set("sharing_password", json!(pwd));
    } else {
        store.delete("sharing_password");
    }

    // Recording persistence settings
    store.set("save_recordings", json!(settings.save_recordings));
    if let Some(count) = settings.recording_retention_count {
        store.set("recording_retention_count", json!(count));
    } else {
        store.delete("recording_retention_count");
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
    let is_cloud_engine = settings.current_model_engine == "soniox";

    if !settings.current_model.is_empty() && old_model != settings.current_model {
        use crate::commands::model::preload_model;
        use tauri::async_runtime::RwLock as AsyncRwLock;

        log::info!(
            "Model changed from '{}' to '{}', preloading new model and updating tray menu",
            old_model,
            settings.current_model
        );

        if !(is_parakeet_engine || is_cloud_engine) {
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
            log::info!(
                "Skipping preload for {} engine selection",
                settings.current_model_engine
            );
        }

        // Update the tray menu to reflect the new selection
        if let Err(e) = update_tray_menu(app.clone()).await {
            log::warn!("Failed to update tray menu after model change: {}", e);
            // Don't fail the whole operation if tray update fails
        }

        // Update the sharing server's model if it's running
        {
            use tauri::async_runtime::RwLock as AsyncRwLock;
            let server_manager = app.state::<AsyncMutex<RemoteServerManager>>();
            let mut server = server_manager.lock().await;
            if server.is_running() {
                let model_path: std::path::PathBuf = if !is_parakeet_engine && !is_cloud_engine {
                    let whisper_state = app.state::<AsyncRwLock<WhisperManager>>();
                    let manager = whisper_state.read().await;
                    manager.get_model_path(&settings.current_model).unwrap_or_default()
                } else {
                    std::path::PathBuf::new()
                };
                server.update_model(model_path, settings.current_model.clone(), settings.current_model_engine.clone());
                log::info!("Remote server model updated to: {} (engine: {})", settings.current_model, settings.current_model_engine);
            }
        }

        // Emit model-changed event so frontend can refresh remote server status
        if let Err(e) = app.emit(
            "model-changed",
            json!({
                "model": settings.current_model,
                "engine": settings.current_model_engine
            }),
        ) {
            log::warn!("Failed to emit model-changed event: {}", e);
        }
    }

    // If recording mode changed, refresh tray to update checked state
    if old_mode != settings.recording_mode {
        if let Err(e) = update_tray_menu(app.clone()).await {
            log::warn!("Failed to update tray menu after mode change: {}", e);
        }
    }

    // If onboarding just completed, try to start device watcher
    if !old_onboarding_completed && settings.onboarding_completed {
        log::info!("Onboarding just completed, checking if device watcher should start");
        try_start_device_watcher_if_ready(&app).await;
    }

    // Handle pill window visibility when pill_indicator_mode setting changes
    if old_pill_indicator_mode != settings.pill_indicator_mode {
        let app_state = app.state::<crate::AppState>();
        let current_state = app_state.get_current_state();
        let is_idle = matches!(current_state, crate::RecordingState::Idle);
        log::info!(
            "pill_visibility: mode change '{}' -> '{}' (state={:?})",
            old_pill_indicator_mode,
            settings.pill_indicator_mode,
            current_state
        );

        // Determine if pill should be visible based on new mode and current state
        let should_show = match settings.pill_indicator_mode.as_str() {
            "never" => false,
            "always" => true,
            "when_recording" => !is_idle, // Show only when recording
            _ => !is_idle, // Default to when_recording behavior
        };
        log::info!(
            "pill_visibility: mode change computed should_show={}",
            should_show
        );

        if should_show {
            if let Err(e) = crate::commands::window::show_pill_widget(app.clone()).await {
                log::warn!("Failed to show pill window: {}", e);
            }
        } else {
            if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
                log::warn!("Failed to hide pill window: {}", e);
            }
        }
    }

    // Handle pill window position when pill_indicator_position setting changes
    // We need to recreate the pill window at the new position since repositioning doesn't work reliably
    if old_pill_indicator_position != settings.pill_indicator_position {
        log::info!("Pill indicator position changed from '{}' to '{}'", old_pill_indicator_position, settings.pill_indicator_position);

        // Check if pill should be visible based on current mode
        let should_show = match settings.pill_indicator_mode.as_str() {
            "never" => false,
            "always" => true,
            "when_recording" => {
                let app_state = app.state::<crate::AppState>();
                let current_state = app_state.get_current_state();
                !matches!(current_state, crate::RecordingState::Idle)
            }
            _ => false,
        };

        if should_show {
            // Close and recreate the pill window at the new position
            if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
                log::warn!("Failed to hide pill window for position change: {}", e);
            }
            if let Err(e) = crate::commands::window::show_pill_widget(app.clone()).await {
                log::warn!("Failed to show pill window at new position: {}", e);
            }
            log::info!("Recreated pill window at new position: {}", settings.pill_indicator_position);
        }
    }

    // Emit settings-changed event so all windows (including pill) can refresh their settings
    if let Err(e) = app.emit("settings-changed", ()) {
        log::warn!("Failed to emit settings-changed event: {}", e);
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
    // Check if this is a remote server selection
    if let Some(connection_id) = model_name.strip_prefix("remote_") {
        log::info!("Setting active remote server from tray: {}", connection_id);

        // Stop network sharing if enabled (can't share while using remote)
        // and remember that it was active for auto-restore later
        if let Some(server_manager) = app.try_state::<AsyncMutex<RemoteServerManager>>() {
            let manager = server_manager.lock().await;
            if manager.get_status().enabled {
                drop(manager); // Release lock before calling stop
                log::info!("🔧 [TRAY] Stopping network sharing - selecting remote server (will remember for auto-restore)");
                let mut manager = server_manager.lock().await;
                manager.stop();

                // Set flag to remember sharing was active
                let remote_state = app.state::<AsyncMutex<RemoteSettings>>();
                let mut settings = remote_state.lock().await;
                settings.sharing_was_active = true;
                save_remote_settings(&app, &settings)?;
                log::info!("🔧 [TRAY] Network sharing stopped, sharing_was_active flag set");
            }
        }

        // Set the remote server as active
        let remote_state = app.state::<AsyncMutex<RemoteSettings>>();
        {
            let mut settings = remote_state.lock().await;
            settings.set_active_connection(Some(connection_id.to_string()))?;
            save_remote_settings(&app, &settings)?;
        }

        // Update the tray menu to reflect the new selection
        update_tray_menu(app.clone()).await?;

        // Emit event to update UI
        if let Err(e) = app.emit(
            "model-changed",
            json!({
                "model": model_name,
                "engine": "remote"
            }),
        ) {
            log::warn!("Failed to emit model-changed event: {}", e);
        }

        return Ok(());
    }

    // Clear any active remote server when selecting a local model
    // and restore sharing if it was previously active
    let should_restore_sharing;
    {
        let remote_state = app.state::<AsyncMutex<RemoteSettings>>();
        let mut settings = remote_state.lock().await;
        should_restore_sharing = settings.sharing_was_active;

        if settings.active_connection_id.is_some() {
            log::info!("Clearing active remote server - switching to local model");
            settings.set_active_connection(None)?;
        }

        // Clear the flag since we'll restore sharing now (if needed)
        if should_restore_sharing {
            settings.sharing_was_active = false;
            log::info!("🔧 [TRAY] Will auto-restore network sharing");
        }

        save_remote_settings(&app, &settings)?;
    }

    // Restore sharing if it was previously active
    if should_restore_sharing {
        if let Some(server_manager) = app.try_state::<AsyncMutex<RemoteServerManager>>() {
            // Get sharing settings from app store
            let store = app.store("settings").map_err(|e| format!("Failed to access store: {}", e))?;
            let port = store.get("sharing_port").and_then(|v| v.as_u64()).map(|p| p as u16).unwrap_or(47842);
            let password = store.get("sharing_password").and_then(|v| v.as_str().map(|s| s.to_string()));

            let server_name = hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "VoiceTypr Server".to_string());

            log::info!("🔧 [TRAY] Auto-restoring network sharing on port {} with model {}", port, model_name);

            // Determine the engine and model path for the model we're switching to
            let whisper_state = app.state::<tauri::async_runtime::RwLock<WhisperManager>>();
            let (engine, model_path) = if model_name == "soniox" {
                ("soniox".to_string(), std::path::PathBuf::new())
            } else {
                let guard = whisper_state.read().await;
                let is_whisper = guard.get_models_status().contains_key(&model_name);
                if is_whisper {
                    let path = guard.get_model_path(&model_name).unwrap_or_default();
                    ("whisper".to_string(), path)
                } else {
                    ("parakeet".to_string(), std::path::PathBuf::new())
                }
            };

            let mut manager = server_manager.lock().await;
            if let Err(e) = manager.start(port, password, server_name, model_path, model_name.clone(), engine).await {
                log::warn!("🔧 [TRAY] Failed to auto-restore sharing: {}", e);
            } else {
                log::info!("🔧 [TRAY] Network sharing auto-restored successfully");
            }
        }
    }

    // Get current settings
    let mut settings = get_settings(app.clone()).await?;

    let engine = if model_name == "soniox" {
        "soniox".to_string()
    } else {
        let whisper_state = app.state::<tauri::async_runtime::RwLock<WhisperManager>>();
        let whisper_has = {
            let guard = whisper_state.read().await;
            guard.get_models_status().contains_key(&model_name)
        };

        if whisper_has {
            "whisper".to_string()
        } else {
            let parakeet_manager = app.state::<ParakeetManager>();
            let is_parakeet = parakeet_manager
                .list_models()
                .into_iter()
                .any(|m| m.name == model_name);
            if is_parakeet {
                "parakeet".to_string()
            } else {
                log::warn!(
                    "set_model_from_tray: model '{}' not found in registries; defaulting to whisper",
                    model_name
                );
                "whisper".to_string()
            }
        }
    };

    // Update the model
    settings.current_model = model_name.clone();
    settings.current_model_engine = engine.clone();
    settings.language = "en".to_string();

    // Save settings (this will also preload the model)
    save_settings(app.clone(), settings).await?;

    // Update the sharing server's model if it's running
    {
        let server_manager = app.state::<AsyncMutex<RemoteServerManager>>();
        let mut server = server_manager.lock().await;
        if server.is_running() {
            // Get the model path based on engine type
            let model_path: std::path::PathBuf = if engine == "whisper" {
                let whisper_state = app.state::<tauri::async_runtime::RwLock<WhisperManager>>();
                let manager = whisper_state.read().await;
                manager.get_model_path(&model_name).unwrap_or_default()
            } else {
                // For non-whisper engines (parakeet, soniox), model path is handled differently
                // Parakeet uses a sidecar, soniox uses cloud API
                std::path::PathBuf::new()
            };

            // Update the shared state - this takes effect immediately for new requests
            server.update_model(model_path, model_name.clone(), engine.clone());
        }
    }

    // Update the tray menu to reflect the new selection
    update_tray_menu(app.clone()).await?;

    // Emit event to update UI only after successful tray menu update
    if let Err(e) = app.emit(
        "model-changed",
        json!({
            "model": model_name,
            "engine": engine
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

#[cfg(test)]
mod tests {
    use super::resolve_pill_indicator_mode;

    #[test]
    fn resolve_pill_indicator_mode_prefers_new_value() {
        let resolved = resolve_pill_indicator_mode(
            Some("always".to_string()),
            Some(false),
            "when_recording".to_string(),
        );

        assert_eq!(resolved, "always");
    }

    #[test]
    fn resolve_pill_indicator_mode_migrates_legacy_true() {
        let resolved = resolve_pill_indicator_mode(None, Some(true), "when_recording".to_string());

        assert_eq!(resolved, "always");
    }

    #[test]
    fn resolve_pill_indicator_mode_migrates_legacy_false() {
        let resolved =
            resolve_pill_indicator_mode(None, Some(false), "when_recording".to_string());

        assert_eq!(resolved, "when_recording");
    }

    #[test]
    fn resolve_pill_indicator_mode_uses_default() {
        let resolved = resolve_pill_indicator_mode(None, None, "when_recording".to_string());

        assert_eq!(resolved, "when_recording");
    }
}
