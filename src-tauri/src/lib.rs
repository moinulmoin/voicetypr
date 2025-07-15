use serde_json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::async_runtime::Mutex as AsyncMutex;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_store::StoreExt;

mod audio;
mod commands;
mod whisper;
mod window_manager;
mod state_machine;
mod state;

#[cfg(test)]
mod tests;

use audio::recorder::AudioRecorder;
use commands::{
    audio::*,
    model::{download_model, get_model_status, delete_model, list_downloaded_models, preload_model},
    settings::*,
    text::*,
    window::*,
    debug::{debug_transcription_flow, test_transcription_event},
    permissions::*
};
use whisper::cache::TranscriberCache;
use window_manager::WindowManager;
use state::unified_state::UnifiedRecordingState;

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

    // Window management runtime state
    pub window_manager: Arc<Mutex<Option<WindowManager>>>,
}

// Tray menu items managed state
pub struct TrayMenuItems {
    pub start_recording: Arc<Mutex<tauri::menu::MenuItem<tauri::Wry>>>,
    pub stop_recording: Arc<Mutex<tauri::menu::MenuItem<tauri::Wry>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            recording_state: UnifiedRecordingState::new(),
            recording_shortcut: Arc::new(Mutex::new(None)),
            current_recording_path: Arc::new(Mutex::new(None)),
            transcription_task: Arc::new(Mutex::new(None)),
            should_cancel_recording: Arc::new(AtomicBool::new(false)),
            window_manager: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_window_manager(&self, manager: WindowManager) {
        let mut wm_guard = self.window_manager.lock().unwrap();
        *wm_guard = Some(manager);
    }

    pub fn get_window_manager(&self) -> Option<WindowManager> {
        self.window_manager.lock().unwrap().clone()
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
    pub fn emit_to_window(&self, window: &str, event: &str, payload: impl serde::Serialize) -> Result<(), String> {
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

    log::debug!("update_recording_state called: {:?} -> {:?}, error: {:?}",
        app_state.get_current_state(), new_state, error);

    // Try to transition using unified state
    match app_state.transition_recording_state(new_state) {
        Ok(_) => {
            log::debug!("Successfully transitioned to state: {:?}", new_state);
        }
        Err(e) => {
            log::error!("State transition failed: {}", e);

            // Only force state updates in specific recovery scenarios
            let current = app_state.get_current_state();
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
                log::warn!("Forcing state transition from {:?} to {:?} for recovery", current, new_state);
                if let Err(force_err) = app_state.recording_state.force_set(new_state) {
                    log::error!("Failed to force set state: {}", force_err);
                }
            } else {
                log::error!("Invalid state transition from {:?} to {:?} - transition blocked", current, new_state);
            }
        }
    }

    // Emit state change event with typed payload
    let _ = app.emit(
        "recording-state-changed",
        serde_json::json!({
            "state": match new_state {
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

    // Update tray menu state
    update_tray_menu_state(app, new_state);
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

// Helper function to update tray menu state based on recording state
pub fn update_tray_menu_state(app: &tauri::AppHandle, recording_state: RecordingState) {
    if let Some(menu_items) = app.try_state::<TrayMenuItems>() {
        match recording_state {
            RecordingState::Idle | RecordingState::Error => {
                // Enable start, disable stop
                if let Ok(start) = menu_items.start_recording.lock() {
                    let _ = start.set_enabled(true);
                }
                if let Ok(stop) = menu_items.stop_recording.lock() {
                    let _ = stop.set_enabled(false);
                }
            }
            RecordingState::Starting | RecordingState::Recording => {
                // Disable start, enable stop
                if let Ok(start) = menu_items.start_recording.lock() {
                    let _ = start.set_enabled(false);
                }
                if let Ok(stop) = menu_items.stop_recording.lock() {
                    let _ = stop.set_enabled(true);
                }
            }
            RecordingState::Stopping | RecordingState::Transcribing => {
                // Disable both during processing
                if let Ok(start) = menu_items.start_recording.lock() {
                    let _ = start.set_enabled(false);
                }
                if let Ok(stop) = menu_items.stop_recording.lock() {
                    let _ = stop.set_enabled(false);
                }
            }
        }
    }
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

    log::info!("Starting VoiceTypr application");

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init());

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

            // Initialize whisper manager
            let models_dir = app.path().app_data_dir()?.join("models");
            log::info!("Models directory: {:?}", models_dir);

            // Ensure the models directory exists
            std::fs::create_dir_all(&models_dir)
                .map_err(|e| format!("Failed to create models directory: {}", e))?;

            let whisper_manager = whisper::manager::WhisperManager::new(models_dir);
            app.manage(AsyncMutex::new(whisper_manager));

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
            use tauri::menu::{MenuBuilder, MenuItem, PredefinedMenuItem};
            use tauri::tray::{TrayIconBuilder, TrayIconEvent};

            // Create menu items
            let start_recording_i = MenuItem::with_id(app, "start_recording", "Start Recording", true, None::<&str>)?;
            let stop_recording_i = MenuItem::with_id(app, "stop_recording", "Stop Recording", false, None::<&str>)?;
            let separator1 = PredefinedMenuItem::separator(app)?;
            let settings_i = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
            let separator2 = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit VoiceTypr", true, None::<&str>)?;

            let menu = MenuBuilder::new(app)
                .item(&start_recording_i)
                .item(&stop_recording_i)
                .item(&separator1)
                .item(&settings_i)
                .item(&separator2)
                .item(&quit_i)
                .build()?;

            // Store menu item references in app state for dynamic updates
            app.manage(TrayMenuItems {
                start_recording: Arc::new(Mutex::new(start_recording_i.clone())),
                stop_recording: Arc::new(Mutex::new(stop_recording_i.clone())),
            });

            // Use default window icon for tray
            let tray_icon = app.default_window_icon()
                .expect("Failed to get default window icon")
                .clone();

            let _tray = TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .tooltip("VoiceType")
                .menu(&menu)
                .on_menu_event(move |app, event| {
                    log::info!("Tray menu event: {:?}", event.id);
                    match event.id.as_ref() {
                        "start_recording" => {
                            let app_handle = app.app_handle().clone();
                            tauri::async_runtime::spawn(async move {
                                let recorder_state = app_handle.state::<RecorderState>();
                                if let Err(e) = start_recording(app_handle.clone(), recorder_state).await {
                                    log::error!("Failed to start recording from menu: {}", e);
                                }
                            });
                        }
                        "stop_recording" => {
                            let app_handle = app.app_handle().clone();
                            tauri::async_runtime::spawn(async move {
                                let recorder_state = app_handle.state::<RecorderState>();
                                if let Err(e) = stop_recording(app_handle.clone(), recorder_state).await {
                                    log::error!("Failed to stop recording from menu: {}", e);
                                }
                            });
                        }
                        "settings" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                                // Emit event to navigate to settings
                                let _ = window.emit("navigate-to-settings", ());
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
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

                        let whisper_state = app_handle.state::<AsyncMutex<whisper::manager::WhisperManager>>();
                        match preload_model(app_handle.clone(), current_model.clone(), whisper_state).await {
                            Ok(_) => {
                                log::info!("Successfully preloaded model: {}", current_model);
                                // Model is already set in store, no need to update
                            }
                            Err(e) => {
                                log::warn!("Failed to preload model '{}': {}. App will continue without preloading.",
                                         current_model, e);

                                // Don't fail - just continue without preloading
                                // The model will be loaded on first use
                            }
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

                // Create the pill window
                let pill_window = WebviewWindowBuilder::new(app, "pill", WebviewUrl::App("pill".into()))
                    .title("Recording")
                    .resizable(false)
                    .decorations(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .transparent(true)
                    .inner_size(64.0, 40.0)
                    .visible(false) // Start hidden
                    .build()?;

                // Convert to NSPanel to prevent focus stealing
                use tauri_nspanel::WebviewWindowExt;
                pill_window.to_panel().map_err(|e| format!("Failed to convert to NSPanel: {:?}", e))?;

                log::info!("Created pill window as NSPanel");
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
            insert_text,
            delete_model,
            list_downloaded_models,
            cleanup_old_transcriptions,
            get_transcription_history,
            show_pill_widget,
            hide_pill_widget,
            close_pill_widget,
            focus_main_window,
            check_accessibility_permission,
            request_accessibility_permission,
        ])
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    // Only hide the window instead of closing it (except for pill)
                    if window.label() == "main" {
                        api.prevent_close();
                        window.hide().unwrap();
                        log::info!("Main window hidden instead of closed");
                    }
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
