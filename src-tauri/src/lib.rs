use chrono::Local;
use serde_json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_log::{Builder as LogBuilder, RotationStrategy, Target, TargetKind};
use tauri_plugin_store::StoreExt;

// Import our logging utilities
use crate::utils::logger::*;

mod ai;
mod audio;
mod commands;
mod ffmpeg;
mod license;
mod parakeet;
mod secure_store;
mod simple_cache;
mod state;
mod state_machine;
mod utils;
mod whisper;
mod window_manager;

#[cfg(test)]
mod tests;

use audio::recorder::AudioRecorder;
use commands::{
    ai::{
        cache_ai_api_key, clear_ai_api_key_cache, disable_ai_enhancement, enhance_transcription,
        get_ai_settings, get_ai_settings_for_provider, get_enhancement_options, get_openai_config,
        set_openai_config, test_openai_endpoint, update_ai_settings, update_enhancement_options,
        validate_and_cache_api_key,
    },
    audio::*,
    clipboard::{copy_image_to_clipboard, save_image_to_file},
    debug::{debug_transcription_flow, test_transcription_event},
    device::get_device_id,
    keyring::{keyring_delete, keyring_get, keyring_has, keyring_set},
    license::*,
    logs::{clear_old_logs, get_log_directory, open_logs_folder},
    model::{
        cancel_download, delete_model, download_model, get_model_status, list_downloaded_models,
        preload_model, verify_model,
    },
    permissions::{
        check_accessibility_permission, check_microphone_permission,
        request_accessibility_permission, request_microphone_permission,
        test_automation_permission,
    },
    reset::reset_app_data,
    settings::*,
    stt::{clear_soniox_key_cache, validate_and_cache_soniox_key},
    text::*,
    utils::export_transcriptions,
    window::*,
};
use state::unified_state::UnifiedRecordingState;
use tauri::menu::{CheckMenuItem, MenuBuilder, MenuItem, PredefinedMenuItem, Submenu};
use whisper::cache::TranscriberCache;
use window_manager::WindowManager;

// Function to build the tray menu
async fn build_tray_menu<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<tauri::menu::Menu<R>, Box<dyn std::error::Error>> {
    // Get current settings for menu state
    let (current_model, selected_microphone) = {
        match app.store("settings") {
            Ok(store) => {
                let model = store
                    .get("current_model")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_default();
                let microphone = store
                    .get("selected_microphone")
                    .and_then(|v| v.as_str().map(|s| s.to_string()));
                (model, microphone)
            }
            Err(_) => ("".to_string(), None),
        }
    };

    // Get available models across Whisper, Parakeet, and cloud (Soniox)
    // Whisper models map retained for display name lookup
    let (available_models, whisper_models_info) = {
        let whisper_state = app.state::<AsyncRwLock<whisper::manager::WhisperManager>>();
        let manager = whisper_state.read().await;
        let whisper_all = manager.get_models_status();
        let mut models: Vec<(String, String)> = whisper_all
            .iter()
            .filter(|(_, info)| info.downloaded)
            .map(|(name, info)| (name.clone(), info.display_name.clone()))
            .collect();

        // Include Parakeet downloaded models
        let parakeet_manager = app.state::<crate::parakeet::ParakeetManager>();
        for m in parakeet_manager.list_models().into_iter() {
            if m.downloaded {
                models.push((m.name.clone(), m.display_name.clone()));
            }
        }

        // Include Soniox (cloud) if connected
        let has_soniox = crate::secure_store::secure_has(app, "stt_api_key_soniox").unwrap_or(false);
        if has_soniox {
            models.push(("soniox".to_string(), "Soniox (Cloud)".to_string()));
        }

        (models, whisper_all)
    };

    // Create model submenu if there are any available models
    let model_submenu = if !available_models.is_empty() {
        let mut model_items: Vec<&dyn tauri::menu::IsMenuItem<_>> = Vec::new();
        let mut model_check_items = Vec::new();

        for (model_name, display_name) in available_models {
            let is_selected = model_name == current_model;
            let model_item = CheckMenuItem::with_id(
                app,
                &format!("model_{}", model_name),
                display_name,
                true,
                is_selected,
                None::<&str>,
            )?;
            model_check_items.push(model_item);
        }

        // Convert to trait objects
        for item in &model_check_items {
            model_items.push(item);
        }

        let current_model_display = if current_model.is_empty() {
            "Model: None".to_string()
        } else {
            // Try Whisper first
            let display_name = if let Some(info) = whisper_models_info.get(&current_model) {
                info.display_name.clone()
            } else {
                // Try Parakeet registry
                let parakeet_manager = app.state::<crate::parakeet::ParakeetManager>();
                if let Some(pm) = parakeet_manager
                    .list_models()
                    .into_iter()
                    .find(|m| m.name == current_model)
                {
                    pm.display_name
                } else if current_model == "soniox" {
                    "Soniox (Cloud)".to_string()
                } else {
                    current_model.clone()
                }
            };
            format!("Model: {}", display_name)
        };

        Some(Submenu::with_id_and_items(
            app,
            "models",
            &current_model_display,
            true,
            &model_items,
        )?)
    } else {
        None
    };

    // Get available audio devices
    let available_devices = audio::recorder::AudioRecorder::get_devices();

    // Create microphone submenu
    let microphone_submenu = if !available_devices.is_empty() {
        let mut mic_items: Vec<&dyn tauri::menu::IsMenuItem<_>> = Vec::new();
        let mut mic_check_items = Vec::new();

        // Add "Default" option first
        let default_item = CheckMenuItem::with_id(
            app,
            "microphone_default",
            "System Default",
            true,
            selected_microphone.is_none(), // Selected if no specific microphone is set
            None::<&str>,
        )?;
        mic_check_items.push(default_item);

        // Add available devices
        for device_name in &available_devices {
            let is_selected = selected_microphone.as_ref() == Some(device_name);
            let mic_item = CheckMenuItem::with_id(
                app,
                &format!("microphone_{}", device_name),
                device_name,
                true,
                is_selected,
                None::<&str>,
            )?;
            mic_check_items.push(mic_item);
        }

        // Convert to trait objects
        for item in &mic_check_items {
            mic_items.push(item);
        }

        let current_mic_display = if let Some(ref mic_name) = selected_microphone {
            format!("Microphone: {}", mic_name)
        } else {
            "Microphone: Default".to_string()
        };

        Some(Submenu::with_id_and_items(
            app,
            "microphones",
            &current_mic_display,
            true,
            &mic_items,
        )?)
    } else {
        None
    };

    // Recent transcriptions (last 5)
    use tauri_plugin_store::StoreExt;
    let mut recent_owned: Vec<tauri::menu::MenuItem<R>> = Vec::new();
    {
        if let Ok(store) = app.store("transcriptions") {
            let mut entries: Vec<(String, serde_json::Value)> = Vec::new();
            for key in store.keys() {
                if let Some(value) = store.get(&key) {
                    entries.push((key.to_string(), value));
                }
            }
            entries.sort_by(|a, b| b.0.cmp(&a.0));
            entries.truncate(5);

            for (ts, entry) in entries {
                let mut label = entry
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| {
                        let first_line = s.lines().next().unwrap_or("").trim();
                        // Build preview safely by Unicode scalar values to avoid slicing in the middle of a codepoint
                        let char_count = first_line.chars().count();
                        let mut preview: String = first_line.chars().take(40).collect();
                        if char_count > 40 {
                            preview.push('…');
                        }
                        if preview.is_empty() {
                            "(empty)".to_string()
                        } else {
                            preview
                        }
                    })
                    .unwrap_or_else(|| "(unknown)".to_string());

                if label.is_empty() {
                    label = "(empty)".to_string();
                }

                let item = tauri::menu::MenuItem::with_id(
                    app,
                    &format!("recent_copy_{}", ts),
                    label,
                    true,
                    None::<&str>,
                )?;
                recent_owned.push(item);
            }
        }
    }
    let mut recent_refs: Vec<&dyn tauri::menu::IsMenuItem<_>> = Vec::new();
    for item in &recent_owned {
        recent_refs.push(item);
    }

    // Recording mode submenu (Toggle / Push-to-Talk)
    let (toggle_item, ptt_item) = {
        let recording_mode = match app.store("settings") {
            Ok(store) => store
                .get("recording_mode")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "toggle".to_string()),
            Err(_) => "toggle".to_string(),
        };

        let toggle = tauri::menu::CheckMenuItem::with_id(
            app,
            "recording_mode_toggle",
            "Toggle",
            true,
            recording_mode == "toggle",
            None::<&str>,
        )?;
        let ptt = tauri::menu::CheckMenuItem::with_id(
            app,
            "recording_mode_push_to_talk",
            "Push-to-Talk",
            true,
            recording_mode == "push_to_talk",
            None::<&str>,
        )?;
        (toggle, ptt)
    };

    // Create menu items
    let separator1 = PredefinedMenuItem::separator(app)?;
    let settings_i = MenuItem::with_id(app, "settings", "Dashboard", true, None::<&str>)?;
    let check_updates_i = MenuItem::with_id(
        app,
        "check_updates",
        "Check for Updates",
        true,
        None::<&str>,
    )?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit VoiceTypr", true, None::<&str>)?;

    let mut menu_builder = MenuBuilder::new(app);

    if let Some(model_submenu) = model_submenu {
        menu_builder = menu_builder.item(&model_submenu);
    }

    if let Some(microphone_submenu) = microphone_submenu {
        menu_builder = menu_builder.item(&microphone_submenu);
    }

    // Add Recent Transcriptions submenu if we have items
    if !recent_refs.is_empty() {
        let recent_submenu =
            Submenu::with_id_and_items(app, "recent", "Recent Transcriptions", true, &recent_refs)?;
        menu_builder = menu_builder.item(&recent_submenu);
    }

    // Recording mode submenu
    let mode_items: Vec<&dyn tauri::menu::IsMenuItem<_>> = vec![&toggle_item, &ptt_item];
    let mode_submenu =
        Submenu::with_id_and_items(app, "recording_mode", "Recording Mode", true, &mode_items)?;
    menu_builder = menu_builder.item(&mode_submenu);

    let menu = menu_builder
        .item(&separator1)
        .item(&settings_i)
        .item(&check_updates_i)
        .item(&separator2)
        .item(&quit_i)
        .build()?;

    Ok(menu)
}

// Recording state enum matching frontend
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub enum RecordingState {
    Idle,
    Starting,
    Recording,
    Stopping,
    Transcribing,
    Error,
}

impl Default for RecordingState {
    fn default() -> Self {
        RecordingState::Idle
    }
}

// Recording mode enum to distinguish between toggle and push-to-talk
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingMode {
    Toggle,     // Click to start/stop recording
    PushToTalk, // Hold to record, release to stop
}

// Application state - managed by Tauri (runtime state only)
pub struct AppState {
    // Recording-related runtime state
    pub recording_state: UnifiedRecordingState,
    pub recording_shortcut: Arc<Mutex<Option<tauri_plugin_global_shortcut::Shortcut>>>,
    pub current_recording_path: Arc<Mutex<Option<PathBuf>>>,
    pub transcription_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,

    // Push-to-talk support
    pub recording_mode: Arc<Mutex<RecordingMode>>,
    pub ptt_key_held: Arc<AtomicBool>,
    pub ptt_shortcut: Arc<Mutex<Option<tauri_plugin_global_shortcut::Shortcut>>>,

    // Cancellation flag for graceful shutdown
    pub should_cancel_recording: Arc<AtomicBool>,

    // ESC key handling for recording cancellation
    pub esc_pressed_once: Arc<AtomicBool>,
    pub esc_timeout_handle: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,

    // Window management runtime state
    pub window_manager: Arc<Mutex<Option<WindowManager>>>,

    // Performance optimization: Cache frequently accessed settings
    pub recording_config_cache:
        Arc<tokio::sync::RwLock<Option<crate::commands::audio::RecordingConfig>>>,

    // License cache with 6-hour expiration
    pub license_cache: Arc<tokio::sync::RwLock<Option<crate::commands::license::CachedLicense>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            recording_state: UnifiedRecordingState::new(),
            recording_shortcut: Arc::new(Mutex::new(None)),
            current_recording_path: Arc::new(Mutex::new(None)),
            transcription_task: Arc::new(Mutex::new(None)),
            recording_mode: Arc::new(Mutex::new(RecordingMode::Toggle)), // Default to toggle mode
            ptt_key_held: Arc::new(AtomicBool::new(false)),
            ptt_shortcut: Arc::new(Mutex::new(None)),
            should_cancel_recording: Arc::new(AtomicBool::new(false)),
            esc_pressed_once: Arc::new(AtomicBool::new(false)),
            esc_timeout_handle: Arc::new(Mutex::new(None)),
            window_manager: Arc::new(Mutex::new(None)),
            recording_config_cache: Arc::new(tokio::sync::RwLock::new(None)),
            license_cache: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    pub fn set_window_manager(&self, manager: WindowManager) {
        if let Ok(mut wm_guard) = self.window_manager.lock() {
            *wm_guard = Some(manager);
        } else {
            log::error!("Failed to acquire window manager lock");
        }
    }

    pub fn get_window_manager(&self) -> Option<WindowManager> {
        match self.window_manager.lock() {
            Ok(guard) => guard.clone(),
            Err(e) => {
                log::error!("Failed to acquire window manager lock: {}", e);
                None
            }
        }
    }

    /// Transition recording state with validation
    pub fn transition_recording_state(&self, new_state: RecordingState) -> Result<(), String> {
        self.recording_state.transition_to(new_state)
    }
    /// Get current recording state
    pub fn get_current_state(&self) -> RecordingState {
        self.recording_state.current()
    }

    /// Request cancellation of ongoing recording/transcription
    pub fn request_cancellation(&self) {
        self.should_cancel_recording.store(true, Ordering::SeqCst);
        log::info!("Recording cancellation requested");
    }

    /// Clear cancellation flag (call when starting new recording)
    pub fn clear_cancellation(&self) {
        self.should_cancel_recording.store(false, Ordering::SeqCst);
    }

    /// Check if cancellation was requested
    pub fn is_cancellation_requested(&self) -> bool {
        self.should_cancel_recording.load(Ordering::SeqCst)
    }

    /// Emit event to specific window using WindowManager
    pub fn emit_to_window(
        &self,
        window: &str,
        event: &str,
        payload: impl serde::Serialize,
    ) -> Result<(), String> {
        if let Some(wm) = self.get_window_manager() {
            // Convert payload to JSON value
            let json_payload = serde_json::to_value(payload)
                .map_err(|e| format!("Failed to serialize payload: {}", e))?;

            match window {
                "main" => wm.emit_to_main(event, json_payload),
                "pill" => wm.emit_to_pill(event, json_payload),
                _ => Err(format!("Unknown window: {}", window)),
            }
        } else {
            Err("WindowManager not initialized".to_string())
        }
    }
}

// Helper function to update recording state and emit event
pub fn update_recording_state(
    app: &tauri::AppHandle,
    new_state: RecordingState,
    error: Option<String>,
) {
    let app_state = app.state::<AppState>();

    // Use atomic transition with fallback to prevent race conditions
    let final_state =
        match app_state
            .recording_state
            .transition_with_fallback(new_state, |current| {
                log::debug!(
                    "update_recording_state: {:?} -> {:?}, error: {:?}",
                    current,
                    new_state,
                    error
                );

                // Determine if we should force a transition based on current state
                let should_force = match (current, new_state) {
                    // Allow force transition from Error state to Idle (recovery)
                    (RecordingState::Error, RecordingState::Idle) => true,
                    // Allow force transition to Error state (error reporting)
                    (_, RecordingState::Error) => true,
                    // Allow force transition to Idle from any state (reset)
                    (_, RecordingState::Idle) if error.is_some() => true,
                    // Disallow other forced transitions
                    _ => false,
                };

                if should_force {
                    log::warn!(
                        "Will force state transition from {:?} to {:?} for recovery",
                        current,
                        new_state
                    );
                    Some(new_state)
                } else {
                    log::error!(
                        "Invalid state transition from {:?} to {:?} - transition blocked",
                        current,
                        new_state
                    );
                    None
                }
            }) {
            Ok(state) => {
                log::debug!("Successfully transitioned to state: {:?}", state);
                state
            }
            Err(e) => {
                log::error!("Failed to transition state: {}", e);
                app_state.get_current_state()
            }
        };

    // Emit state change event with typed payload using the actual final state
    let payload = serde_json::json!({
        "state": match final_state {
            RecordingState::Idle => "idle",
            RecordingState::Starting => "starting",
            RecordingState::Recording => "recording",
            RecordingState::Stopping => "stopping",
            RecordingState::Transcribing => "transcribing",
            RecordingState::Error => "error",
        },
        "error": error
    });

    // Emit to all windows
    let _ = app.emit("recording-state-changed", payload.clone());

    // Also emit specifically to pill window to ensure it receives the event
    if let Some(pill_window) = app.get_webview_window("pill") {
        let _ = pill_window.emit("recording-state-changed", payload);
    }
}

// Helper function to get current recording state
pub fn get_recording_state(app: &tauri::AppHandle) -> RecordingState {
    let app_state = app.state::<AppState>();
    app_state.get_current_state()
}

// Helper function to emit events to specific windows
pub fn emit_to_window(
    app: &tauri::AppHandle,
    window: &str,
    event: &str,
    payload: impl serde::Serialize,
) -> Result<(), String> {
    let app_state = app.state::<AppState>();
    app_state.emit_to_window(window, event, payload)
}

// Helper function to emit events to all windows
pub fn emit_to_all(
    app: &tauri::AppHandle,
    event: &str,
    payload: impl serde::Serialize + Clone,
) -> Result<(), String> {
    app.emit(event, payload)
        .map_err(|e| format!("Failed to emit to all windows: {}", e))
}

// Show a short error message on the pill window for a brief duration
pub async fn show_pill_error_short(
    app: &tauri::AppHandle,
    event: &str,
    payload: &str,
    millis: u64,
) {
    let app_state = app.state::<AppState>();
    if let Some(window_manager) = app_state.get_window_manager() {
        // Best-effort: show pill, emit event, keep visible briefly
        let _ = window_manager.show_pill_window().await;
        let _ = emit_to_window(app, "pill", event, payload);
        tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
    } else {
        log::error!("WindowManager not initialized; unable to show pill error");
        let _ = emit_to_window(app, "pill", event, payload);
    }
}

// Setup logging with daily rotation
fn setup_logging() -> tauri_plugin_log::Builder {
    let today = Local::now().format("%Y-%m-%d").to_string();

    LogBuilder::default()
        .targets([
            Target::new(TargetKind::Stdout).filter(|metadata| {
                // Filter out noisy logs
                let target = metadata.target();
                !target.contains("whisper_rs")
                    && !target.contains("audio::level_meter")
                    && !target.contains("cpal")
                    && !target.contains("rubato")
                    && !target.contains("hound")
            }),
            Target::new(TargetKind::LogDir {
                file_name: Some(format!("voicetypr-{}.log", today)),
            })
            .filter(|metadata| {
                // Filter out noisy logs from file as well
                let target = metadata.target();
                !target.contains("whisper_rs")
                    && !target.contains("audio::level_meter")
                    && !target.contains("cpal")
                    && !target.contains("rubato")
                    && !target.contains("hound")
            }),
        ])
        .rotation_strategy(RotationStrategy::KeepAll)
        .max_file_size(10_000_000) // 10MB per file
        .level(if cfg!(debug_assertions) {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let app_start = Instant::now();
    let app_version = env!("CARGO_PKG_VERSION");

    // Log application startup
    log_lifecycle_event("APPLICATION_START", Some(app_version), None);

    // Load .env file if it exists (for development)
    log_start("ENV_FILE_LOAD");
    match dotenv::dotenv() {
        Ok(path) => {
            log_file_operation("LOAD", &format!("{:?}", path), true, None, None);
            println!("Loaded .env file from: {:?}", path);
        }
        Err(e) => {
            log::info!("📄 No .env file found or error loading it: {}", e);
            println!("No .env file found or error loading it: {}", e);
        }
    }

    // Initialize encryption key for secure storage
    log_start("ENCRYPTION_INIT");
    log_with_context(
        log::Level::Debug,
        "Initializing encryption",
        &[("component", "secure_store")],
    );

    if let Err(e) = secure_store::initialize_encryption_key() {
        log_failed(
            "ENCRYPTION_INIT",
            &format!("Failed to initialize encryption: {}", e),
        );
        eprintln!("Failed to initialize encryption: {}", e);
    } else {
        log::info!("✅ Encryption initialized successfully");
    }

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .plugin(setup_logging().build())
        // Replaced tauri-plugin-cache with simple_store-backed cache
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin({
            #[cfg(target_os = "macos")]
            let autostart = tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                None::<Vec<&str>>,
            );

            #[cfg(not(target_os = "macos"))]
            let autostart = tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent, // This param is ignored on non-macOS
                None::<Vec<&str>>,
            );

            autostart
        })
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init());

    // Add NSPanel plugin on macOS
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .plugin(tauri_nspanel::init())
            .plugin(tauri_plugin_macos_permissions::init());
    }

    builder
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    log::debug!(
                        "Global shortcut triggered: {:?} - State: {:?}",
                        shortcut,
                        event.state()
                    );

                    let app_handle = app.app_handle();
                    let app_state = app_handle.state::<AppState>();

                    // Get current recording mode
                    let recording_mode = {
                        if let Ok(mode_guard) = app_state.recording_mode.lock() {
                            *mode_guard
                        } else {
                            RecordingMode::Toggle // Default to toggle if we can't get the mode
                        }
                    };

                    // Check if this is the recording shortcut
                    let is_recording_shortcut = {
                        if let Ok(shortcut_guard) = app_state.recording_shortcut.lock() {
                            if let Some(ref recording_shortcut) = *shortcut_guard {
                                shortcut == recording_shortcut
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    };

                    // Check if this is the PTT shortcut (if configured differently)
                    let is_ptt_shortcut = {
                        if let Ok(ptt_guard) = app_state.ptt_shortcut.lock() {
                            if let Some(ref ptt_shortcut) = *ptt_guard {
                                shortcut == ptt_shortcut
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    };

                    // Determine if we should handle this shortcut based on mode
                    let should_handle = match recording_mode {
                        RecordingMode::Toggle => is_recording_shortcut && event.state() == ShortcutState::Pressed,
                        RecordingMode::PushToTalk => is_recording_shortcut || is_ptt_shortcut,
                    };

                    if should_handle {
                        let current_state = get_recording_state(&app_handle);

                        match recording_mode {
                            RecordingMode::Toggle => {
                                // Toggle mode - only handle press events
                                if event.state() == ShortcutState::Pressed {
                                    match current_state {
                                        RecordingState::Idle | RecordingState::Error => {
                                            log::info!("Toggle: Starting recording via hotkey");

                                            let app_handle = app.app_handle().clone();
                                            tauri::async_runtime::spawn(async move {
                                                let recorder_state = app_handle.state::<RecorderState>();
                                                match start_recording(app_handle.clone(), recorder_state).await {
                                                    Ok(_) => log::info!("Toggle: Recording started successfully"),
                                                    Err(e) => {
                                                        log::error!("Toggle: Error starting recording: {}", e);
                                                        update_recording_state(
                                                            &app_handle,
                                                            RecordingState::Error,
                                                            Some(e),
                                                        );
                                                    }
                                                }
                                            });
                                        }
                                        RecordingState::Recording | RecordingState::Starting => {
                                            log::info!("Toggle: Stopping recording via hotkey");

                                            let app_handle = app.app_handle().clone();
                                            tauri::async_runtime::spawn(async move {
                                                let recorder_state = app_handle.state::<RecorderState>();
                                                match stop_recording(app_handle.clone(), recorder_state).await {
                                                    Ok(_) => log::info!("Toggle: Recording stopped successfully"),
                                                    Err(e) => log::error!("Toggle: Error stopping recording: {}", e)
                                                }
                                            });
                                        }
                                        _ => log::debug!("Toggle: Ignoring hotkey in state {:?}", current_state),
                                    }
                                }
                            }
                            RecordingMode::PushToTalk => {
                                // Push-to-talk mode - handle both press and release
                                match event.state() {
                                    ShortcutState::Pressed => {
                                        log::info!("PTT: Key pressed");
                                        app_state.ptt_key_held.store(true, Ordering::Relaxed);

                                        if matches!(current_state, RecordingState::Idle | RecordingState::Error) {
                                            log::info!("PTT: Starting recording");

                                            let app_handle = app.app_handle().clone();
                                            tauri::async_runtime::spawn(async move {
                                                let recorder_state = app_handle.state::<RecorderState>();
                                                match start_recording(app_handle.clone(), recorder_state).await {
                                                    Ok(_) => log::info!("PTT: Recording started successfully"),
                                                    Err(e) => {
                                                        log::error!("PTT: Error starting recording: {}", e);
                                                        update_recording_state(
                                                            &app_handle,
                                                            RecordingState::Error,
                                                            Some(e),
                                                        );
                                                    }
                                                }
                                            });
                                        }
                                    }
                                    ShortcutState::Released => {
                                        log::info!("PTT: Key released");
                                        app_state.ptt_key_held.store(false, Ordering::Relaxed);

                                        if matches!(current_state, RecordingState::Recording | RecordingState::Starting) {
                                            log::info!("PTT: Stopping recording");

                                            let app_handle = app.app_handle().clone();
                                            tauri::async_runtime::spawn(async move {
                                                let recorder_state = app_handle.state::<RecorderState>();
                                                match stop_recording(app_handle.clone(), recorder_state).await {
                                                    Ok(_) => log::info!("PTT: Recording stopped successfully"),
                                                    Err(e) => log::error!("PTT: Error stopping recording: {}", e)
                                                }
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    } else if !is_recording_shortcut && !is_ptt_shortcut {
                        // Debug log all shortcuts
                        log::debug!("Non-recording shortcut triggered: {:?}", shortcut);

                        // Check if this is the ESC key
                        let escape_shortcut: tauri_plugin_global_shortcut::Shortcut = match "Escape".parse() {
                            Ok(s) => s,
                            Err(e) => {
                                log::error!("Failed to parse Escape shortcut: {:?}", e);
                                return;
                            }
                        };

                        log::debug!("Comparing shortcuts - received: {:?}, escape: {:?}", shortcut, escape_shortcut);

                        if shortcut == &escape_shortcut {
                            log::info!("ESC key detected in global handler");

                            // Only react to ESC key press events (ignore key release)
                            if event.state() != ShortcutState::Pressed {
                                log::debug!("Ignoring ESC event since it is not a key press: {:?}", event.state());
                                return;
                            }

                            // Handle ESC key for recording cancellation
                            let current_state = get_recording_state(&app_handle);
                            log::debug!("Current recording state: {:?}", current_state);

                            // Only handle ESC during recording or transcribing
                            if matches!(current_state, RecordingState::Recording | RecordingState::Transcribing | RecordingState::Starting | RecordingState::Stopping) {
                            let app_state = app_handle.state::<AppState>();
                            let was_pressed_once = app_state.esc_pressed_once.load(Ordering::SeqCst);

                            if !was_pressed_once {
                                // First ESC press
                                log::info!("First ESC press detected during recording");
                                app_state.esc_pressed_once.store(true, Ordering::SeqCst);

                                // Emit event to pill for feedback
                                let _ = emit_to_window(&app_handle, "pill", "esc-first-press", "Press ESC again to stop recording");

                                // Set timeout to reset ESC state
                                let app_for_timeout = app_handle.clone();
                                let timeout_handle = tauri::async_runtime::spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

                                    let app_state = app_for_timeout.state::<AppState>();
                                    app_state.esc_pressed_once.store(false, Ordering::SeqCst);
                                    log::debug!("ESC timeout expired, resetting state");
                                });

                                // Store timeout handle (abort previous one if exists)
                                if let Ok(mut timeout_guard) = app_state.esc_timeout_handle.lock() {
                                    if let Some(old_handle) = timeout_guard.take() {
                                        old_handle.abort();
                                        log::debug!("Aborted previous ESC timeout handle");
                                    }
                                    *timeout_guard = Some(timeout_handle);
                                }
                            } else {
                                // Second ESC press - cancel recording
                                log::info!("Second ESC press detected, cancelling recording");

                                // Cancel timeout
                                if let Ok(mut timeout_guard) = app_state.esc_timeout_handle.lock() {
                                    if let Some(handle) = timeout_guard.take() {
                                        handle.abort();
                                    }
                                }

                                // Reset ESC state
                                app_state.esc_pressed_once.store(false, Ordering::SeqCst);

                                // Cancel recording
                                let app_for_cancel = app_handle.clone();
                                tauri::async_runtime::spawn(async move {
                                    if let Err(e) = cancel_recording(app_for_cancel).await {
                                        log::error!("Failed to cancel recording: {}", e);
                                    }
                                });
                            }
                        }
                    }
                }
                })
                .build(),
        )
        .setup(move |app| {
            let setup_start = Instant::now();
            log::info!("🚀 App setup START - version: {}", app_version);
            
            // Keyring is now used instead of Stronghold for API keys
            // Much faster and uses OS-native secure storage
            log::info!("🔐 Using OS-native keyring for secure API key storage");

            // Set up panic handler to catch crashes
            log_start("PANIC_HANDLER_SETUP");
            log_with_context(log::Level::Debug, "Setting up panic handler", &[
                ("component", "panic_handler")
            ]);
            
            std::panic::set_hook(Box::new(|panic_info| {
                let location = panic_info.location()
                    .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                    .unwrap_or_else(|| "unknown location".to_string());
                
                let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic payload".to_string()
                };
                
                log::error!("💥 CRITICAL PANIC at {}: {}", location, message);
                log_failed("PANIC", "Application panic occurred");
                log_with_context(log::Level::Error, "Panic details", &[
                    ("panic_location", &location),
                    ("panic_message", &message),
                    ("severity", "critical")
                ]);
                eprintln!("Application panic at {}: {}", location, message);
                
                // Try to save panic info to a crash file for debugging
                if let Ok(home_dir) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
                    let crash_file = std::path::Path::new(&home_dir).join(".voicetypr_crash.log");
                    let _ = std::fs::write(&crash_file, format!(
                        "Panic at {}: {}\nFull info: {:?}\nTime: {:?}",
                        location, message, panic_info, chrono::Local::now()
                    ));
                }
            }));
            
            log::info!("✅ Panic handler configured");

            // Clean up old logs on startup (keep last 30 days)
            log_start("LOG_CLEANUP");
            log_with_context(log::Level::Debug, "Cleaning up old logs", &[
                ("retention_days", "30")
            ]);
            
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let cleanup_start = Instant::now();
                match commands::logs::clear_old_logs(app_handle, 30).await {
                    Ok(deleted) => {
                        log_complete("LOG_CLEANUP", cleanup_start.elapsed().as_millis() as u64);
                        log_with_context(log::Level::Debug, "Log cleanup complete", &[
                            ("files_deleted", &deleted.to_string().as_str())
                        ]);
                        if deleted > 0 {
                            log::info!("🧹 Cleaned up {} old log files", deleted);
                        }
                    }
                    Err(e) => {
                        log_failed("LOG_CLEANUP", &e);
                        log_with_context(log::Level::Debug, "Log cleanup failed", &[
                            ("retention_days", "30")
                        ]);
                        log::warn!("Failed to clean up old logs: {}", e);
                    }
                }
            });

            // Set activation policy on macOS to prevent focus stealing
            #[cfg(target_os = "macos")]
            {
                log_start("MACOS_SETUP");
                log_with_context(log::Level::Debug, "Setting up macOS policy", &[
                    ("policy", "Accessory")
                ]);
                
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                log::info!("🍎 Set macOS activation policy to Accessory");

            }

            // Clear license cache on app start to ensure fresh checks
            {
                use crate::simple_cache;
                let _ = simple_cache::remove(&app.app_handle(), "license_status");
                let _ = simple_cache::remove(&app.app_handle(), "last_license_validation");
            }

            // Run comprehensive startup checks
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                perform_startup_checks(app_handle).await;
            });

            // Initialize whisper manager
            let models_dir = app.path().app_data_dir()?.join("models");
            log::info!("🗂️  Models directory: {:?}", models_dir);

            log_start("WHISPER_MANAGER_INIT");
            log_with_context(log::Level::Debug, "Initializing Whisper manager", &[
                ("models_dir", &format!("{:?}", models_dir).as_str())
            ]);

            // Ensure the models directory exists
            match std::fs::create_dir_all(&models_dir) {
                Ok(_) => {
                    log_file_operation("CREATE_DIR", &format!("{:?}", models_dir), true, None, None);
                }
                Err(e) => {
                    let error_msg = format!("Failed to create models directory: {}", e);
                    log_file_operation("CREATE_DIR", &format!("{:?}", models_dir), false, None, Some(&e.to_string()));
                    return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, error_msg)));
                }
            }

            let whisper_manager = whisper::manager::WhisperManager::new(models_dir.clone());
            app.manage(AsyncRwLock::new(whisper_manager));
            
            log::info!("✅ Whisper manager initialized and managed");

            // Initialize Parakeet manager and cache directory
            let parakeet_dir = models_dir.join("parakeet");
            if let Err(e) = std::fs::create_dir_all(&parakeet_dir) {
                let error_msg = format!("Failed to create parakeet models directory: {}", e);
                log_file_operation("CREATE_DIR", &format!("{:?}", parakeet_dir), false, None, Some(&e.to_string()));
                return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, error_msg)));
            }

            log_file_operation("CREATE_DIR", &format!("{:?}", parakeet_dir), true, None, None);
            let parakeet_manager = parakeet::ParakeetManager::new(parakeet_dir);
            app.manage(parakeet_manager);
            log::info!("🦜 Parakeet manager initialized");

            // Manage active downloads for cancellation
            app.manage(Arc::new(Mutex::new(HashMap::<String, Arc<AtomicBool>>::new())));

            // Initialize transcriber cache for keeping models in memory
            // Cache size is 1: only the current model (1-3GB RAM)
            // When user switches models, old one is unloaded immediately
            app.manage(AsyncMutex::new(TranscriberCache::new()));

            // Initialize unified application state
            app.manage(AppState::new());

            // Initialize window manager after app state is managed
            let app_state = app.state::<AppState>();
            let window_manager = WindowManager::new(app.app_handle().clone());
            app_state.set_window_manager(window_manager);

            // Clean up old logs on startup (keep only today's log)
            let app_handle_for_logs = app.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                match clear_old_logs(app_handle_for_logs, 1).await {
                    Ok(deleted_count) => {
                        if deleted_count > 0 {
                            log::info!("Cleaned up {} old log files (keeping only today)", deleted_count);
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to clean up old logs: {}. App will continue normally.", e);
                    }
                }
            });

            // Pill position is loaded from settings when needed, no duplicate state

            // Initialize recorder state (kept separate for backwards compatibility)
            app.manage(RecorderState(Mutex::new(AudioRecorder::new())));

            // Create tray icon
            use tauri::tray::{TrayIconBuilder, TrayIconEvent};

            // Build the tray menu using our helper function
            // Note: We need to block here since setup is sync
            let menu = tauri::async_runtime::block_on(build_tray_menu(&app.app_handle()))?;


            // Use default window icon for tray
            let tray_icon = match app.default_window_icon() {
                Some(icon) => icon.clone(),
                None => {
                    log::error!("Default window icon not found, cannot create tray");
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "Default window icon not available"
                    )));
                }
            };

            let _tray = TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .tooltip("VoiceTypr")
                .menu(&menu)
                .on_menu_event(move |app, event| {
                    log::info!("Tray menu event: {:?}", event.id);
                    let event_id = event.id.as_ref().to_string();

                    if event_id == "settings" {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            // Emit event to navigate to settings
                            let _ = window.emit("navigate-to-settings", ());
                        }
                    } else if event_id == "quit" {
                        app.exit(0);
                    } else if event_id == "check_updates" {
                        let _ = app.emit("tray-check-updates", ());
                    } else if event_id.starts_with("model_") {
                        // Handle model selection
                        let model_name = match event_id.strip_prefix("model_") {
                            Some(name) => name.to_string(),
                            None => {
                                log::warn!("Invalid model event_id format: {}", event_id);
                                return; // Skip processing invalid model events
                            }
                        };
                        let app_handle = app.app_handle().clone();

                        tauri::async_runtime::spawn(async move {
                            match crate::commands::settings::set_model_from_tray(app_handle.clone(), model_name.clone()).await {
                                Ok(_) => {
                                    log::info!("Model changed from tray to: {}", model_name);
                                }
                                Err(e) => {
                                    log::error!("Failed to set model from tray: {}", e);
                                    // Emit error event so UI can show notification
                                    let _ = app_handle.emit("tray-action-error", &format!("Failed to change model: {}", e));
                                }
                            }
                        });
                    } else if event_id == "microphone_default" {
                        // Handle default microphone selection
                        let app_handle = app.app_handle().clone();
                        
                        tauri::async_runtime::spawn(async move {
                            match crate::commands::settings::set_audio_device(app_handle.clone(), None).await {
                                Ok(_) => {
                                    log::info!("Microphone changed from tray to: System Default");
                                }
                                Err(e) => {
                                    log::error!("Failed to set default microphone from tray: {}", e);
                                    let _ = app_handle.emit("tray-action-error", &format!("Failed to change microphone: {}", e));
                                }
                            }
                        });
                    } else if event_id.starts_with("microphone_") {
                        // Handle specific microphone selection
                        let device_name = match event_id.strip_prefix("microphone_") {
                            Some(name) if name != "default" => Some(name.to_string()),
                            _ => {
                                // Already handled by microphone_default case above
                                return;
                            }
                        };
                        let app_handle = app.app_handle().clone();

                        tauri::async_runtime::spawn(async move {
                            match crate::commands::settings::set_audio_device(app_handle.clone(), device_name.clone()).await {
                                Ok(_) => {
                                    log::info!("Microphone changed from tray to: {:?}", device_name);
                                }
                                Err(e) => {
                                    log::error!("Failed to set microphone from tray: {}", e);
                                    let _ = app_handle.emit("tray-action-error", &format!("Failed to change microphone: {}", e));
                                }
                            }
                        });
                    }
                    // Recent transcriptions copy handler
                    else if let Some(ts) = event_id.strip_prefix("recent_copy_") {
                        let ts_owned = ts.to_string();
                        let app_handle = app.app_handle().clone();
                        tauri::async_runtime::spawn(async move {
                            // Read text by timestamp and copy
                            match app_handle.store("transcriptions") {
                                Ok(store) => {
                                    if let Some(val) = store.get(&ts_owned) {
                                        if let Some(text) = val.get("text").and_then(|v| v.as_str()) {
                                            if let Err(e) = crate::commands::text::copy_text_to_clipboard(text.to_string()).await {
                                                log::error!("Failed to copy recent transcription: {}", e);
                                                let _ = app_handle.emit("tray-action-error", &format!("Failed to copy: {}", e));
                                            } else {
                                                log::info!("Copied recent transcription to clipboard");
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to open transcriptions store: {}", e);
                                }
                            }
                        });
                    }
                    // Recording mode switchers
                    else if event_id == "recording_mode_toggle" || event_id == "recording_mode_push_to_talk" {
                        let app_handle = app.app_handle().clone();
                        let mode = if event_id.ends_with("push_to_talk") { "push_to_talk" } else { "toggle" };
                        tauri::async_runtime::spawn(async move {
                            match crate::commands::settings::get_settings(app_handle.clone()).await {
                                Ok(mut s) => {
                                    s.recording_mode = mode.to_string();
                                    match crate::commands::settings::save_settings(app_handle.clone(), s).await {
                                        Err(e) => {
                                            log::error!("Failed to save recording mode from tray: {}", e);
                                            let _ = app_handle.emit("tray-action-error", &format!("Failed to change recording mode: {}", e));
                                        }
                                        Ok(()) => {
                                            if let Err(e) = crate::commands::settings::update_tray_menu(app_handle.clone()).await {
                                                log::warn!("Failed to refresh tray after mode change: {}", e);
                                            }
                                            // Notify frontend so SettingsContext refreshes
                                            let _ = app_handle.emit("settings-changed", ());
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to get settings for mode change: {}", e);
                                }
                            }
                        });
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Load hotkey from settings store with graceful degradation
            log_start("HOTKEY_SETUP");
            log_with_context(log::Level::Debug, "Setting up hotkey", &[
                ("default", "CommandOrControl+Shift+Space")
            ]);
            
            let hotkey_str = match app.store("settings") {
                Ok(store) => {
                    store
                        .get("hotkey")
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| {
                            log::info!("🎹 No hotkey configured, using default");
                            "CommandOrControl+Shift+Space".to_string()
                        })
                }
                Err(e) => {
                    log_failed("SETTINGS_LOAD", &format!("Failed to load settings store: {}", e));
                    log_with_context(log::Level::Debug, "Settings load failed", &[
                        ("component", "settings"),
                        ("fallback", "CommandOrControl+Shift+Space")
                    ]);
                    "CommandOrControl+Shift+Space".to_string()
                }
            };

            log::info!("🎯 Loading hotkey: {}", hotkey_str);

            // Load recording mode and PTT settings
            let (recording_mode_str, use_different_ptt_key, ptt_hotkey_str) = match app.store("settings") {
                Ok(store) => {
                    let mode = store
                        .get("recording_mode")
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| "toggle".to_string());

                    let use_diff = store
                        .get("use_different_ptt_key")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let ptt_key = store
                        .get("ptt_hotkey")
                        .and_then(|v| v.as_str().map(|s| s.to_string()));

                    (mode, use_diff, ptt_key)
                }
                Err(_) => {
                    log::info!("Using default recording mode settings");
                    ("toggle".to_string(), false, None)
                }
            };

            // Set recording mode in AppState
            let app_state = app.state::<AppState>();
            let recording_mode = match recording_mode_str.as_str() {
                "push_to_talk" => RecordingMode::PushToTalk,
                _ => RecordingMode::Toggle,
            };

            if let Ok(mut mode_guard) = app_state.recording_mode.lock() {
                *mode_guard = recording_mode;
                log::info!("Recording mode set to: {:?}", recording_mode);
            }

            // Normalize the hotkey for Tauri
            let normalized_hotkey = crate::commands::key_normalizer::normalize_shortcut_keys(&hotkey_str);

            // Register global shortcut from settings with fallback
            let shortcut: tauri_plugin_global_shortcut::Shortcut = match normalized_hotkey.parse() {
                Ok(s) => s,
                Err(_) => {
                    log::warn!("Invalid hotkey format '{}', using default", normalized_hotkey);
                    match "CommandOrControl+Shift+Space".parse() {
                        Ok(default_shortcut) => default_shortcut,
                        Err(e) => {
                            log::error!("Even default shortcut failed to parse: {}", e);
                            // Emit event to notify frontend that hotkey registration failed
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.emit("hotkey-registration-failed", ());
                            }
                            // Return a minimal working shortcut or continue without hotkey
                            return Ok(());
                        }
                    }
                }
            };

            // Store the recording shortcut in managed state
            let app_state = app.state::<AppState>();
            if let Ok(mut shortcut_guard) = app_state.recording_shortcut.lock() {
                *shortcut_guard = Some(shortcut.clone());
            }

            // Try to register global shortcut with panic protection
            let registration_start = Instant::now();
            let registration_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                app.global_shortcut().register(shortcut.clone())
            }));
            
            match registration_result {
                Ok(Ok(_)) => {
                    log_complete("HOTKEY_REGISTRATION", registration_start.elapsed().as_millis() as u64);
                    log_with_context(log::Level::Debug, "Hotkey registered", &[
                        ("hotkey", &hotkey_str),
                        ("normalized", &normalized_hotkey)
                    ]);
                    log::info!("✅ Successfully registered global hotkey: {}", hotkey_str);
                }
                Ok(Err(e)) => {
                    log_failed("HOTKEY_REGISTRATION", &e.to_string());
                    log_with_context(log::Level::Debug, "Hotkey registration failed", &[
                        ("hotkey", &hotkey_str),
                        ("normalized", &normalized_hotkey),
                        ("suggestion", "Try different hotkey or close conflicting apps")
                    ]);
                    
                    log::error!("❌ Failed to register global hotkey '{}': {}", hotkey_str, e);
                    log::warn!("⚠️  The app will continue without global hotkey support. Another application may be using this shortcut.");
                    
                    // Emit event to notify frontend that hotkey registration failed
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit("hotkey-registration-failed", serde_json::json!({
                            "hotkey": hotkey_str,
                            "error": e.to_string(),
                            "suggestion": "Please choose a different hotkey in settings or close conflicting applications"
                        }));
                    }
                }
                Err(panic_err) => {
                    let panic_msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_err.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "Unknown panic during hotkey registration".to_string()
                    };
                    
                    log::error!("💥 PANIC during hotkey registration: {}", panic_msg);
                    log::warn!("⚠️  Continuing without global hotkey due to panic");
                    
                    // Emit event to notify frontend
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit("hotkey-registration-failed", serde_json::json!({
                            "hotkey": hotkey_str,
                            "error": format!("Critical error: {}", panic_msg),
                            "suggestion": "The hotkey system encountered an error. Please restart the app or try a different hotkey."
                        }));
                    }
                }
            }

            // Register PTT shortcut if configured differently
            if recording_mode == RecordingMode::PushToTalk && use_different_ptt_key {
                if let Some(ptt_key) = ptt_hotkey_str {
                    log::info!("🎤 Registering separate PTT hotkey: {}", ptt_key);

                    let normalized_ptt = crate::commands::key_normalizer::normalize_shortcut_keys(&ptt_key);

                    if let Ok(ptt_shortcut) = normalized_ptt.parse::<tauri_plugin_global_shortcut::Shortcut>() {
                        // Store PTT shortcut in AppState
                        let app_state = app.state::<AppState>();
                        if let Ok(mut ptt_guard) = app_state.ptt_shortcut.lock() {
                            *ptt_guard = Some(ptt_shortcut.clone());
                        }

                        // Try to register PTT shortcut
                        match app.global_shortcut().register(ptt_shortcut.clone()) {
                            Ok(_) => {
                                log::info!("✅ Successfully registered PTT hotkey: {}", ptt_key);
                            }
                            Err(e) => {
                                log::error!("❌ Failed to register PTT hotkey '{}': {}", ptt_key, e);
                                log::warn!("⚠️  PTT will use primary hotkey instead");

                                // Clear the PTT shortcut so we fall back to primary
                                if let Ok(mut ptt_guard) = app_state.ptt_shortcut.lock() {
                                    *ptt_guard = None;
                                }
                            }
                        }
                    } else {
                        log::warn!("Invalid PTT hotkey format: {}", ptt_key);
                    }
                }
            }

            // Preload current model if set (graceful degradation)
            // Use Tauri's async runtime which is available after setup
            if let Ok(store) = app.store("settings") {
                if let Some(current_model) = store.get("current_model")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .filter(|s| !s.is_empty())
                {
                    let app_handle = app.app_handle().clone();
                    // Use tauri::async_runtime instead of tokio directly
                    tauri::async_runtime::spawn(async move {
                        log::info!("Attempting to preload model on startup: {}", current_model);

                        // Get model path from WhisperManager
                        let whisper_state = app_handle.state::<AsyncRwLock<whisper::manager::WhisperManager>>();
                        let model_path = {
                            let manager = whisper_state.read().await;
                            manager.get_model_path(&current_model)
                        };

                        if let Some(model_path) = model_path {
                            // Load model into cache
                            let cache_state = app_handle.state::<AsyncMutex<TranscriberCache>>();
                            let mut cache = cache_state.lock().await;

                            match cache.get_or_create(&model_path) {
                                Ok(_) => {
                                    log::info!("Successfully preloaded model '{}' into cache", current_model);
                                }
                                Err(e) => {
                                    log::warn!("Failed to preload model '{}': {}. App will continue without preloading.",
                                             current_model, e);
                                }
                            }
                        } else {
                            log::warn!("Model '{}' not found in models directory, skipping preload", current_model);
                        }
                    });
                } else {
                    log::info!("No model configured for preloading");
                }
            }

            // Create pill window at startup and convert to NSPanel
            #[cfg(target_os = "macos")]
            {
                use tauri::{WebviewUrl, WebviewWindowBuilder};

                // Create the pill window with extra height for tooltip
                let pill_builder = WebviewWindowBuilder::new(app, "pill", WebviewUrl::App("pill".into()))
                    .title("Recording")
                    .resizable(false)
                    .decorations(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .transparent(true)
                    .inner_size(350.0, 150.0)  // Match window_manager.rs size
                    .visible(false); // Start hidden

                // Disable context menu only in production builds
                #[cfg(not(debug_assertions))]
                let pill_builder = pill_builder.initialization_script("document.addEventListener('contextmenu', e => e.preventDefault());");

                #[cfg(debug_assertions)]
                let pill_builder = pill_builder;

                let pill_window = pill_builder.build()?;

                // Convert to NSPanel to prevent focus stealing
                use tauri_nspanel::WebviewWindowExt;
                pill_window.to_panel().map_err(|e| format!("Failed to convert to NSPanel: {:?}", e))?;

                // Store the pill window reference in WindowManager
                let app_state = app.state::<AppState>();
                if let Some(window_manager) = app_state.get_window_manager() {
                    window_manager.set_pill_window(pill_window);
                    log::info!("Created pill window as NSPanel and stored in WindowManager");
                } else {
                    log::warn!("Could not store pill window reference - WindowManager not available");
                }
            }

            // Sync autostart state with saved settings
            if let Ok(store) = app.store("settings") {
                let saved_autostart = store.get("launch_at_startup")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                // Check actual autostart state and sync if needed
                use tauri_plugin_autostart::ManagerExt;
                let autolaunch = app.autolaunch();

                match autolaunch.is_enabled() {
                    Ok(actual_enabled) => {
                        if actual_enabled != saved_autostart {
                            log::info!("Syncing autostart state: saved={}, actual={}", saved_autostart, actual_enabled);

                            // Settings are source of truth
                            if saved_autostart {
                                if let Err(e) = autolaunch.enable() {
                                    log::warn!("Failed to enable autostart: {}", e);
                                }
                            } else {
                                if let Err(e) = autolaunch.disable() {
                                    log::warn!("Failed to disable autostart: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to check autostart state: {}", e);
                    }
                }
            }

            // Hide main window on start (menu bar only)
            // Check if this is first launch by looking for current_model setting
            let should_hide_main = if let Ok(store) = app.store("settings") {
                // If user has a model configured, they've completed onboarding
                store.get("current_model")
                    .and_then(|v| v.as_str().map(|s| !s.is_empty()))
                    .unwrap_or(false)
            } else {
                false
            };

            if should_hide_main {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                    log::info!("Main window hidden - menubar mode active");
                }
            } else {
                log::info!("👋 First launch or no model configured - keeping main window visible");
            }

            // Log setup completion
            log_performance("APP_SETUP_COMPLETE", setup_start.elapsed().as_millis() as u64, None);
            log::info!("🎉 App setup COMPLETED - Total time: {}ms", setup_start.elapsed().as_millis());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            cancel_recording,
            get_current_recording_state,
            debug_transcription_flow,
            test_transcription_event,
            save_transcription,
            get_audio_devices,
            get_current_audio_device,
            download_model,
            get_model_status,
            preload_model,
            verify_model,
            transcribe_audio,
            transcribe_audio_file,
            get_settings,
            save_settings,
            set_audio_device,
            set_global_shortcut,
            get_supported_languages,
            set_model_from_tray,
            update_tray_menu,
            insert_text,
            delete_model,
            list_downloaded_models,
            cancel_download,
            cleanup_old_transcriptions,
            get_transcription_history,
            delete_transcription_entry,
            clear_all_transcriptions,
            export_transcriptions,
            show_pill_widget,
            hide_pill_widget,
            close_pill_widget,
            focus_main_window,
            check_accessibility_permission,
            request_accessibility_permission,
            check_microphone_permission,
            request_microphone_permission,
            test_automation_permission,
            check_license_status,
            restore_license,
            activate_license,
            deactivate_license,
            open_purchase_page,
            invalidate_license_cache,
            reset_app_data,
            copy_image_to_clipboard,
            save_image_to_file,
            copy_text_to_clipboard,
            get_ai_settings,
            get_ai_settings_for_provider,
            cache_ai_api_key,
            validate_and_cache_api_key,
            set_openai_config,
            get_openai_config,
            test_openai_endpoint,
            clear_ai_api_key_cache,
            update_ai_settings,
            enhance_transcription,
            disable_ai_enhancement,
            get_enhancement_options,
            update_enhancement_options,
            keyring_set,
            keyring_get,
            keyring_delete,
            keyring_has,
            validate_and_cache_soniox_key,
            clear_soniox_key_cache,
            get_log_directory,
            open_logs_folder,
            get_device_id,
        ])
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    // Only hide the window instead of closing it (except for pill)
                    if window.label() == "main" {
                        api.prevent_close();
                        if let Err(e) = window.hide() {
                            log::error!("Failed to hide main window: {}", e);
                        } else {
                            log::info!("Main window hidden instead of closed");
                        }
                    }
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .map_err(|e| -> Box<dyn std::error::Error> {
            log_failed("APPLICATION_RUN", &format!("Critical error running Tauri application: {}", e));
            log_with_context(log::Level::Error, "Application run failed", &[
                ("stage", "application_run"),
                ("total_startup_time_ms", &app_start.elapsed().as_millis().to_string().as_str())
            ]);
            eprintln!("VoiceTypr failed to start: {}", e);
            Box::new(e)
        })?;

    // Log successful application startup
    log_lifecycle_event("APPLICATION_READY", Some(app_version), None);

    Ok(())
}

/// Perform essential startup checks
async fn perform_startup_checks(app: tauri::AppHandle) {
    let checks_start = Instant::now();
    log_start("STARTUP_CHECKS");
    log_with_context(
        log::Level::Debug,
        "Running startup checks",
        &[("stage", "comprehensive_validation")],
    );

    // Check if any models are downloaded
    if let Some(whisper_manager) = app.try_state::<AsyncRwLock<whisper::manager::WhisperManager>>()
    {
        let has_models = whisper_manager.read().await.has_downloaded_models();

        log_model_operation(
            "AVAILABILITY_CHECK",
            "all",
            if has_models {
                "AVAILABLE"
            } else {
                "NONE_FOUND"
            },
            None,
        );

        if !has_models {
            log::warn!("⚠️  No speech recognition models downloaded");
            // Emit event to frontend to show download prompt
            let _ = emit_to_window(&app, "main", "no-models-on-startup", ());
        } else {
            log::info!("✅ Speech recognition models are available");
        }
    }

    // Validate AI settings if enabled
    if let Ok(store) = app.store("settings") {
        let ai_enabled = store
            .get("ai_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if ai_enabled {
            let provider = store
                .get("ai_provider")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "groq".to_string());

            let model = store
                .get("ai_model")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();

            // Check if API key is cached
            use crate::commands::ai::get_ai_settings;
            match get_ai_settings(app.clone()).await {
                Ok(settings) => {
                    if !settings.has_api_key {
                        log::warn!("AI enabled but no API key found for provider: {}", provider);
                        // Disable AI to prevent errors during recording
                        store.set("ai_enabled", serde_json::Value::Bool(false));
                        let _ = store.save();

                        // Notify frontend
                        let _ = emit_to_window(
                            &app,
                            "main",
                            "ai-disabled-no-key",
                            "AI enhancement disabled - no API key found",
                        );
                    } else if model.is_empty() {
                        log::warn!("AI enabled but no model selected");
                        store.set("ai_enabled", serde_json::Value::Bool(false));
                        let _ = store.save();

                        let _ = emit_to_window(
                            &app,
                            "main",
                            "ai-disabled-no-model",
                            "AI enhancement disabled - no model selected",
                        );
                    } else {
                        log::info!("AI enhancement ready: {} with {}", provider, model);
                    }
                }
                Err(e) => {
                    log::error!("Failed to check AI settings: {}", e);
                }
            }
        }
    }

    let mut autoload_parakeet_model: Option<String> = None;

    // Pre-check recording settings
    if let Ok(store) = app.store("settings") {
        // Validate language setting
        let language = store
            .get("language")
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        if let Some(lang) = language {
            use crate::whisper::languages::validate_language;
            let validated = validate_language(Some(&lang));
            if validated != lang.as_str() {
                log::warn!(
                    "Invalid language '{}' in settings, resetting to '{}'",
                    lang,
                    validated
                );
                store.set("language", serde_json::Value::String(validated.to_string()));
                let _ = store.save();
            }
        }

        // Check current model is still available based on engine type
        let mut _model_available = false;
        if let Some(current_model) = store
            .get("current_model")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
        {
            if !current_model.is_empty() {
                // Get the engine type to determine which manager to check
                let engine = store
                    .get("current_model_engine")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| "whisper".to_string());

                if engine == "parakeet" {
                    // Check ParakeetManager for Parakeet models
                    if let Some(parakeet_manager) = app.try_state::<parakeet::ParakeetManager>() {
                        let models = parakeet_manager.list_models();
                        if let Some(status) = models.iter().find(|m| m.name == current_model) {
                            _model_available = status.downloaded;
                            if status.downloaded {
                                autoload_parakeet_model = Some(current_model.clone());
                            }
                        }
                        if !_model_available {
                            log::warn!(
                                "Current Parakeet model '{}' no longer available",
                                current_model
                            );
                            // Clear the selection
                            store.set("current_model", serde_json::Value::String(String::new()));
                            store.set(
                                "current_model_engine",
                                serde_json::Value::String("whisper".to_string()),
                            );
                            let _ = store.save();
                        }
                    }
                } else {
                    // Check WhisperManager for Whisper models (default)
                    if let Some(whisper_manager) =
                        app.try_state::<AsyncRwLock<whisper::manager::WhisperManager>>()
                    {
                        let downloaded = whisper_manager.read().await.get_downloaded_model_names();
                        _model_available = downloaded.contains(&current_model);
                        if !_model_available {
                            log::warn!(
                                "Current Whisper model '{}' no longer available",
                                current_model
                            );
                            // Clear the selection
                            store.set("current_model", serde_json::Value::String(String::new()));
                            let _ = store.save();
                        }
                    }
                }
            }
        }
    }

    if let Some(model_name) = autoload_parakeet_model {
        if let Some(parakeet_manager) = app.try_state::<parakeet::ParakeetManager>() {
            match parakeet_manager.load_model(&app, &model_name).await {
                Ok(_) => {
                    log::info!("✅ Parakeet model '{}' autoloaded from cache", model_name);
                }
                Err(err) => {
                    log::warn!(
                        "Failed to autoload Parakeet model '{}': {}",
                        model_name,
                        err
                    );
                    let message = format!(
                        "Unable to load Parakeet model '{}'. Please re-download it.",
                        model_name
                    );
                    let _ = app.emit("parakeet-unavailable", message.clone());
                }
            }
        }
    }

    // Log startup checks completion
    log_complete("STARTUP_CHECKS", checks_start.elapsed().as_millis() as u64);
    log_with_context(
        log::Level::Debug,
        "Startup checks complete",
        &[("status", "all_checks_completed")],
    );
    log::info!(
        "✅ Startup checks COMPLETED in {}ms",
        checks_start.elapsed().as_millis()
    );
}
