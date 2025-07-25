use serde_json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_store::StoreExt;
use tauri_plugin_sentry::{minidump, sentry};

mod audio;
mod commands;
mod license;
mod state;
mod state_machine;
mod whisper;
mod window_manager;

#[cfg(test)]
mod tests;

use audio::recorder::AudioRecorder;
use commands::{
    audio::*,
    debug::{debug_transcription_flow, test_transcription_event},
    license::*,
    model::{
        cancel_download, delete_model, download_model, get_model_status, list_downloaded_models,
        preload_model,
    },
    permissions::{check_accessibility_permission, check_microphone_permission, request_accessibility_permission, request_microphone_permission, test_automation_permission},
    reset::reset_app_data,
    settings::*,
    text::*,
    window::*,
};
use state::unified_state::UnifiedRecordingState;
use std::collections::HashMap;
use whisper::cache::TranscriberCache;
use window_manager::WindowManager;
use tauri::menu::{MenuBuilder, MenuItem, PredefinedMenuItem, Submenu, CheckMenuItem};

// Helper function to format model names for display
fn format_model_name(name: &str) -> String {
    match name {
        "base.en" => "Base (English)".to_string(),
        "small.en" => "Small (English)".to_string(),
        "large-v3" => "Large v3".to_string(),
        "large-v3-q5_0" => "Large v3 Q5".to_string(),
        "large-v3-turbo" => "Large v3 Turbo".to_string(),
        "large-v3-turbo-q5_0" => "Large v3 Turbo Q5".to_string(),
        _ => name.to_string(),
    }
}

// Function to build the tray menu
async fn build_tray_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<tauri::menu::Menu<R>, Box<dyn std::error::Error>> {
    // Get current settings for menu state
    let current_settings = {
        match app.store("settings") {
            Ok(store) => {
                let current_model = store.get("current_model")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_default();
                let current_language = store.get("language")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| "en".to_string());
                (current_model, current_language)
            }
            Err(_) => ("".to_string(), "en".to_string())
        }
    };
    
    // Get downloaded models
    let downloaded_models = {
        let whisper_state = app.state::<AsyncRwLock<whisper::manager::WhisperManager>>();
        let manager = whisper_state.read().await;
        manager.get_models_status()
            .into_iter()
            .filter(|(_, info)| info.downloaded)
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
    };
    
    // Create model submenu if there are downloaded models
    let model_submenu = if !downloaded_models.is_empty() {
        let mut model_items: Vec<&dyn tauri::menu::IsMenuItem<_>> = Vec::new();
        let mut model_check_items = Vec::new();
        
        for model_name in downloaded_models {
            let display_name = format_model_name(&model_name);
            let is_selected = model_name == current_settings.0;
            let model_item = CheckMenuItem::with_id(
                app,
                &format!("model_{}", model_name),
                display_name,
                true,
                is_selected,
                None::<&str>
            )?;
            model_check_items.push(model_item);
        }
        
        // Convert to trait objects
        for item in &model_check_items {
            model_items.push(item);
        }
        
        let current_model_display = if current_settings.0.is_empty() {
            "Model: None".to_string()
        } else {
            format!("Model: {}", format_model_name(&current_settings.0))
        };
        
        Some(Submenu::with_id_and_items(
            app,
            "models",
            &current_model_display,
            true,
            &model_items
        )?)
    } else {
        None
    };
    
    // Get supported languages
    let languages = crate::whisper::languages::SUPPORTED_LANGUAGES
        .iter()
        .map(|(code, lang)| (code.to_string(), lang.name.to_string()))
        .collect::<Vec<_>>();
    
    // Create language submenu
    let mut lang_items: Vec<&dyn tauri::menu::IsMenuItem<_>> = Vec::new();
    let mut lang_check_items = Vec::new();
    
    // Sort languages by name but keep Auto Detect first
    let mut sorted_languages = languages;
    sorted_languages.sort_by(|a, b| {
        if a.0 == "auto" {
            std::cmp::Ordering::Less
        } else if b.0 == "auto" {
            std::cmp::Ordering::Greater
        } else {
            a.1.cmp(&b.1)
        }
    });
    
    for (code, name) in sorted_languages {
        let is_selected = code == current_settings.1;
        let lang_item = CheckMenuItem::with_id(
            app,
            &format!("lang_{}", code),
            &name,
            true,
            is_selected,
            None::<&str>
        )?;
        lang_check_items.push(lang_item);
    }
    
    // Convert to trait objects
    for item in &lang_check_items {
        lang_items.push(item);
    }
    
    let current_lang_name = crate::whisper::languages::SUPPORTED_LANGUAGES
        .get(current_settings.1.as_str())
        .map(|l| l.name)
        .unwrap_or("Unknown");
    let lang_submenu = Submenu::with_id_and_items(
        app,
        "languages",
        &format!("Language: {}", current_lang_name),
        true,
        &lang_items
    )?;

    // Create menu items
    let separator1 = PredefinedMenuItem::separator(app)?;
    let settings_i = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit VoiceTypr", true, None::<&str>)?;

    let mut menu_builder = MenuBuilder::new(app);
    
    if let Some(model_submenu) = model_submenu {
        menu_builder = menu_builder.item(&model_submenu);
    }
    
    let menu = menu_builder
        .item(&lang_submenu)
        .item(&separator1)
        .item(&settings_i)
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

// Application state - managed by Tauri (runtime state only)
pub struct AppState {
    // Recording-related runtime state
    pub recording_state: UnifiedRecordingState,
    pub recording_shortcut: Arc<Mutex<Option<tauri_plugin_global_shortcut::Shortcut>>>,
    pub current_recording_path: Arc<Mutex<Option<PathBuf>>>,
    pub transcription_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,

    // Cancellation flag for graceful shutdown
    pub should_cancel_recording: Arc<AtomicBool>,

    // ESC key handling for recording cancellation
    pub esc_pressed_once: Arc<AtomicBool>,
    pub esc_timeout_handle: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,

    // Window management runtime state
    pub window_manager: Arc<Mutex<Option<WindowManager>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            recording_state: UnifiedRecordingState::new(),
            recording_shortcut: Arc::new(Mutex::new(None)),
            current_recording_path: Arc::new(Mutex::new(None)),
            transcription_task: Arc::new(Mutex::new(None)),
            should_cancel_recording: Arc::new(AtomicBool::new(false)),
            esc_pressed_once: Arc::new(AtomicBool::new(false)),
            esc_timeout_handle: Arc::new(Mutex::new(None)),
            window_manager: Arc::new(Mutex::new(None)),
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
    let final_state = match app_state.recording_state.transition_with_fallback(
        new_state,
        |current| {
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
        }
    ) {
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
    let _ = app.emit(
        "recording-state-changed",
        serde_json::json!({
            "state": match final_state {
                RecordingState::Idle => "idle",
                RecordingState::Starting => "starting",
                RecordingState::Recording => "recording",
                RecordingState::Stopping => "stopping",
                RecordingState::Transcribing => "transcribing",
                RecordingState::Error => "error",
            },
            "error": error
        }),
    );
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logger with appropriate level based on build type
    #[cfg(debug_assertions)]
    {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    }
    #[cfg(not(debug_assertions))]
    {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }

    // Load .env file if it exists (for development)
    match dotenv::dotenv() {
        Ok(path) => log::debug!("Loaded .env file from: {:?}", path),
        Err(e) => log::debug!("No .env file found or error loading it: {}", e),
    }

    log::info!("Starting VoiceTypr application");
    
    // Initialize Sentry if DSN is provided
    let _sentry_guard = if let Ok(dsn) = std::env::var("SENTRY_DSN") {
        if !dsn.is_empty() && dsn != "__YOUR_SENTRY_DSN__" {
            log::info!("Initializing Sentry error tracking");
            let client = sentry::init((
                dsn,
                sentry::ClientOptions {
                    release: sentry::release_name!(),
                    // Disable session tracking for privacy
                    auto_session_tracking: false,
                    // Set environment based on build mode
                    environment: Some(if cfg!(debug_assertions) {
                        "development"
                    } else {
                        "production"
                    }.into()),
                    // Sample rate for performance monitoring (0.0 to 1.0)
                    traces_sample_rate: 0.1,
                    // Attach stack traces to messages
                    attach_stacktrace: true,
                    // Privacy: Don't send user information
                    send_default_pii: false,
                    ..Default::default()
                },
            ));

            // Initialize minidump for native crash reporting (not on iOS)
            #[cfg(not(target_os = "ios"))]
            let _minidump_guard = minidump::init(&client);
            
            Some(client)
        } else {
            log::warn!("Sentry DSN not configured. Error tracking disabled.");
            None
        }
    } else {
        log::info!("SENTRY_DSN environment variable not set. Error tracking disabled.");
        None
    };

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_cache::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None::<Vec<&str>>,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init());

    // Add NSPanel plugin on macOS
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .plugin(tauri_nspanel::init())
            .plugin(tauri_plugin_macos_permissions::init());
    }
    
    // Add Sentry plugin if initialized
    if let Some(ref client) = _sentry_guard {
        builder = builder.plugin(tauri_plugin_sentry::init(client));
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

                    // Only handle key press events, ignore release for toggle behavior
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }

                    // Check if this is the recording shortcut
                    let app_handle = app.app_handle();
                    let is_recording_shortcut = {
                        let app_state = app_handle.state::<AppState>();
                        let result = if let Ok(shortcut_guard) = app_state.recording_shortcut.lock()
                        {
                            if let Some(ref recording_shortcut) = *shortcut_guard {
                                shortcut == recording_shortcut
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        result
                    }; // Lock dropped here

                    if is_recording_shortcut {
                        // Toggle recording based on current state
                        let current_state = get_recording_state(&app_handle);
                        match current_state {
                            RecordingState::Idle | RecordingState::Error => {
                                log::info!("Toggle: Starting recording via hotkey");

                                // Use Tauri's command system to start recording
                                let app_handle = app.app_handle().clone();
                                tauri::async_runtime::spawn(async move {
                                    // Get the recorder state from app handle
                                    let recorder_state = app_handle.state::<RecorderState>();
                                    match start_recording(app_handle.clone(), recorder_state).await
                                    {
                                        Ok(_) => {
                                            log::info!("Toggle: Recording started successfully")
                                        }
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

                                // Use Tauri's command system to stop recording
                                let app_handle = app.app_handle().clone();
                                tauri::async_runtime::spawn(async move {
                                    // Get the recorder state from app handle
                                    let recorder_state = app_handle.state::<RecorderState>();
                                    match stop_recording(app_handle.clone(), recorder_state).await {
                                        Ok(_) => {
                                            log::info!("Toggle: Recording stopped successfully")
                                        }
                                        Err(e) => {
                                            log::error!("Toggle: Error stopping recording: {}", e)
                                        }
                                    }
                                });
                            }
                            _ => {
                                log::debug!("Toggle: Ignoring hotkey in state {:?}", current_state);
                            }
                        }
                    } else {
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

                                // Store timeout handle
                                if let Ok(mut timeout_guard) = app_state.esc_timeout_handle.lock() {
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
        .setup(|app| {
            // Set up panic handler to catch crashes
            std::panic::set_hook(Box::new(|panic_info| {
                log::error!("PANIC: {:?}", panic_info);
                eprintln!("Application panic: {:?}", panic_info);
            }));

            // Set activation policy on macOS to prevent focus stealing
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                log::info!("Set macOS activation policy to Accessory");

                // Check accessibility permissions at startup
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    // Small delay to ensure app is fully initialized
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

                    // Check and request accessibility permission for keyboard simulation
                    match app_handle.emit("check-accessibility-permission", ()) {
                        Ok(_) => log::info!("Emitted accessibility permission check event"),
                        Err(e) => log::error!("Failed to emit accessibility check: {}", e),
                    }
                });
            }

            // Clear license cache on app start to ensure fresh checks
            {
                use tauri_plugin_cache::CacheExt;
                let cache = app.cache();
                if let Err(e) = cache.remove("license_status") {
                    log::debug!("No license cache to clear on startup: {}", e);
                } else {
                    log::info!("🧹 Cleared license cache on app startup for fresh check");
                }
                // Also clear the last validation tracker
                let _ = cache.remove("last_license_validation");
            }

            // Initialize whisper manager
            let models_dir = app.path().app_data_dir()?.join("models");
            log::info!("Models directory: {:?}", models_dir);

            // Ensure the models directory exists
            std::fs::create_dir_all(&models_dir)
                .map_err(|e| format!("Failed to create models directory: {}", e))?;

            let whisper_manager = whisper::manager::WhisperManager::new(models_dir);
            app.manage(AsyncRwLock::new(whisper_manager));

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

            // Pill position is loaded from settings when needed, no duplicate state

            // Initialize recorder state (kept separate for backwards compatibility)
            app.manage(RecorderState(Mutex::new(AudioRecorder::new())));

            // Create tray icon
            use tauri::tray::{TrayIconBuilder, TrayIconEvent};
            
            // Build the tray menu using our helper function
            // Note: We need to block here since setup is sync
            let menu = tauri::async_runtime::block_on(build_tray_menu(&app.app_handle()))?;


            // Use default window icon for tray
            let tray_icon = app.default_window_icon()
                .expect("Failed to get default window icon")
                .clone();

            let _tray = TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .tooltip("VoiceTypr")
                .menu(&menu)
                .on_menu_event(move |app, event| {
                    log::info!("Tray menu event: {:?}", event.id);
                    let event_id = event.id.as_ref();
                    
                    if event_id == "settings" {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            // Emit event to navigate to settings
                            let _ = window.emit("navigate-to-settings", ());
                        }
                    } else if event_id == "quit" {
                        app.exit(0);
                    } else if event_id.starts_with("model_") {
                        // Handle model selection
                        let model_name = event_id.strip_prefix("model_").unwrap().to_string();
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
                    } else if event_id.starts_with("lang_") {
                        // Handle language selection
                        let lang_code = event_id.strip_prefix("lang_").unwrap().to_string();
                        let app_handle = app.app_handle().clone();
                        
                        tauri::async_runtime::spawn(async move {
                            match crate::commands::settings::set_language_from_tray(app_handle.clone(), lang_code.clone()).await {
                                Ok(_) => {
                                    log::info!("Language changed from tray to: {}", lang_code);
                                }
                                Err(e) => {
                                    log::error!("Failed to set language from tray: {}", e);
                                    // Emit error event so UI can show notification
                                    let _ = app_handle.emit("tray-action-error", &format!("Failed to change language: {}", e));
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
            let hotkey_str = match app.store("settings") {
                Ok(store) => {
                    store
                        .get("hotkey")
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| {
                            log::info!("No hotkey configured, using default");
                            "CommandOrControl+Shift+Space".to_string()
                        })
                }
                Err(e) => {
                    log::warn!("Failed to load settings store: {}. Using default hotkey.", e);
                    "CommandOrControl+Shift+Space".to_string()
                }
            };

            log::info!("Loading hotkey: {}", hotkey_str);

            // Register global shortcut from settings with fallback
            let shortcut: tauri_plugin_global_shortcut::Shortcut = match hotkey_str.parse() {
                Ok(s) => s,
                Err(_) => {
                    log::warn!("Invalid hotkey format '{}', using default", hotkey_str);
                    "CommandOrControl+Shift+Space".parse()
                        .expect("Default shortcut should be valid")
                }
            };

            // Store the recording shortcut in managed state
            let app_state = app.state::<AppState>();
            if let Ok(mut shortcut_guard) = app_state.recording_shortcut.lock() {
                *shortcut_guard = Some(shortcut.clone());
            }

            app.global_shortcut().register(shortcut)?;

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
                let pill_window = WebviewWindowBuilder::new(app, "pill", WebviewUrl::App("pill".into()))
                    .title("Recording")
                    .resizable(false)
                    .decorations(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .transparent(true)
                    .inner_size(350.0, 150.0)  // Match window_manager.rs size
                    .visible(false) // Start hidden
                    .build()?;

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
                log::info!("First launch or no model configured - keeping main window visible");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            cancel_recording,
            debug_transcription_flow,
            test_transcription_event,
            save_transcription,
            get_audio_devices,
            download_model,
            get_model_status,
            preload_model,
            transcribe_audio,
            get_settings,
            save_settings,
            set_global_shortcut,
            get_supported_languages,
            set_model_from_tray,
            set_language_from_tray,
            update_tray_menu,
            insert_text,
            delete_model,
            list_downloaded_models,
            cancel_download,
            cleanup_old_transcriptions,
            get_transcription_history,
            delete_transcription_entry,
            clear_all_transcriptions,
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
            reset_app_data,
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
        .expect("error while running tauri application");
}
