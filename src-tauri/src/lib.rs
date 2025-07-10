use serde_json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::async_runtime::Mutex as AsyncMutex;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_store::StoreExt;

mod audio;
mod commands;
mod whisper;
mod window_manager;
mod state_machine;

#[cfg(test)]
mod tests;

use audio::recorder::AudioRecorder;
use commands::{audio::*, model::*, settings::*, text::*, window::*};
use whisper::cache::TranscriberCache;
use window_manager::WindowManager;
use state_machine::RecordingStateMachine;

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

// Application state - managed by Tauri
pub struct AppState {
    pub recording_state: Arc<Mutex<RecordingState>>,
    pub recording_state_machine: Arc<Mutex<RecordingStateMachine>>,
    pub recording_shortcut: Arc<Mutex<Option<tauri_plugin_global_shortcut::Shortcut>>>,
    pub current_recording_path: Arc<Mutex<Option<PathBuf>>>,
    pub transcription_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    pub window_manager: Arc<Mutex<Option<WindowManager>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            recording_state: Arc::new(Mutex::new(RecordingState::Idle)),
            recording_state_machine: Arc::new(Mutex::new(RecordingStateMachine::new())),
            recording_shortcut: Arc::new(Mutex::new(None)),
            current_recording_path: Arc::new(Mutex::new(None)),
            transcription_task: Arc::new(Mutex::new(None)),
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
        let mut state_machine = self.recording_state_machine.lock()
            .map_err(|e| format!("Failed to lock state machine: {}", e))?;
        
        state_machine.transition_to(new_state)
            .map_err(|e| e.to_string())?;
        
        // Update the legacy state as well
        let mut state = self.recording_state.lock()
            .map_err(|e| format!("Failed to lock recording state: {}", e))?;
        *state = new_state;
        
        Ok(())
    }
    
    /// Get current recording state from state machine
    pub fn get_current_state(&self) -> RecordingState {
        self.recording_state_machine.lock()
            .map(|sm| sm.current())
            .unwrap_or(RecordingState::Idle)
    }
}

// Helper function to update recording state and emit event
pub fn update_recording_state(
    app: &tauri::AppHandle,
    new_state: RecordingState,
    error: Option<String>,
) {
    let app_state = app.state::<AppState>();
    
    // Try to transition using state machine
    match app_state.transition_recording_state(new_state) {
        Ok(_) => {
            log::info!("Successfully transitioned to state: {:?}", new_state);
        }
        Err(e) => {
            log::error!("State transition failed: {}. Forcing state update.", e);
            // Force state update even if transition is invalid (for backwards compatibility)
            if let Ok(mut state_machine) = app_state.recording_state_machine.lock() {
                state_machine.reset();
                let _ = state_machine.transition_to(new_state);
            }
            if let Ok(mut current_state) = app_state.recording_state.lock() {
                *current_state = new_state;
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
}

// Helper function to get current recording state
pub fn get_recording_state(app: &tauri::AppHandle) -> RecordingState {
    let app_state = app.state::<AppState>();
    app_state
        .recording_state
        .lock()
        .map(|guard| *guard)
        .unwrap_or(RecordingState::Idle)
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
        builder = builder.plugin(tauri_nspanel::init());
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
            // Set activation policy on macOS to prevent focus stealing
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                log::info!("Set macOS activation policy to Accessory");
            }

            // Initialize whisper manager
            let models_dir = app.path().app_data_dir()?.join("models");
            log::info!("Models directory: {:?}", models_dir);

            // Ensure the models directory exists
            std::fs::create_dir_all(&models_dir)
                .map_err(|e| format!("Failed to create models directory: {}", e))?;

            let whisper_manager = whisper::manager::WhisperManager::new(models_dir);
            app.manage(AsyncMutex::new(whisper_manager));

            // NEW: cache for transcribers (keeps models in memory)
            app.manage(AsyncMutex::new(TranscriberCache::new()));

            // Initialize unified application state
            app.manage(AppState::new());
            
            // Initialize window manager after app state is managed
            let app_state = app.state::<AppState>();
            let window_manager = WindowManager::new(app.app_handle().clone());
            app_state.set_window_manager(window_manager);

            // Initialize recorder state (kept separate for backwards compatibility)
            app.manage(RecorderState(Mutex::new(AudioRecorder::new())));

            // Create tray icon
            use tauri::menu::{MenuBuilder, MenuItem};
            use tauri::tray::{TrayIconBuilder, TrayIconEvent};

            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;

            let menu = MenuBuilder::new(app)
                .item(&show_i)
                .separator()
                .item(&quit_i)
                .build()?;

            let _tray = TrayIconBuilder::with_id("main")
                .tooltip("VoiceType")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
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

            // Load hotkey from settings store
            let store = app.store("settings").map_err(|e| e.to_string())?;

            let hotkey_str = store
                .get("hotkey")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "CommandOrControl+Shift+Space".to_string());

            log::info!("Loading hotkey from store: {}", hotkey_str);

            // Register global shortcut from settings
            let shortcut: tauri_plugin_global_shortcut::Shortcut =
                hotkey_str.parse().map_err(|_| "Invalid shortcut format")?;

            // Store the recording shortcut in managed state
            let app_state = app.state::<AppState>();
            if let Ok(mut shortcut_guard) = app_state.recording_shortcut.lock() {
                *shortcut_guard = Some(shortcut.clone());
            }

            app.global_shortcut().register(shortcut)?;

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
            // TODO: Only hide after successful onboarding
            // if let Some(window) = app.get_webview_window("main") {
            //     let _ = window.hide();
            // }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            get_audio_devices,
            download_model,
            get_model_status,
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
            update_pill_position,
            focus_main_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
