use tauri::{Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_store::StoreExt;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use tauri::async_runtime::Mutex as AsyncMutex;
use serde_json;

mod audio;
mod whisper;
mod commands;

use commands::{audio::*, model::*, settings::*, text::*};
use audio::recorder::AudioRecorder;
use whisper::cache::TranscriberCache;

// Global state for tracking the recording shortcut
static RECORDING_SHORTCUT: std::sync::OnceLock<Arc<Mutex<tauri_plugin_global_shortcut::Shortcut>>> = std::sync::OnceLock::new();

// Recording state enum matching frontend
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecordingState {
    Idle,
    Starting,
    Recording,
    Stopping,
    Transcribing,
    Error,
}

// Global state for tracking recording state (single source of truth)
static RECORDING_STATE: std::sync::OnceLock<Arc<Mutex<RecordingState>>> = std::sync::OnceLock::new();

// Helper function to update recording state and emit event
pub fn update_recording_state(app: &tauri::AppHandle, new_state: RecordingState, error: Option<String>) {
    if let Some(state) = RECORDING_STATE.get() {
        if let Ok(mut current_state) = state.lock() {
            *current_state = new_state;
            
            // Emit state change event
            let _ = app.emit("recording-state-changed", serde_json::json!({
                "state": match new_state {
                    RecordingState::Idle => "idle",
                    RecordingState::Starting => "starting",
                    RecordingState::Recording => "recording",
                    RecordingState::Stopping => "stopping",
                    RecordingState::Transcribing => "transcribing",
                    RecordingState::Error => "error",
                },
                "error": error
            }));
        }
    }
}

// Helper function to get current recording state
pub fn get_recording_state() -> RecordingState {
    if let Some(state) = RECORDING_STATE.get() {
        if let Ok(current_state) = state.lock() {
            return *current_state;
        }
    }
    RecordingState::Idle
}

// Helper function to reset recording state
pub fn reset_recording_state() {
    if let Some(state) = RECORDING_STATE.get() {
        if let Ok(mut current_state) = state.lock() {
            *current_state = RecordingState::Idle;
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {

    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    println!("Global shortcut triggered: {:?} - State: {:?}", shortcut, event.state());
                    
                    // Only handle key press events, ignore release for toggle behavior
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    
                    // Check if this is the recording shortcut
                    if let Some(recording_shortcut) = RECORDING_SHORTCUT.get() {
                        if let Ok(guard) = recording_shortcut.lock() {
                            if shortcut == &*guard {
                                // Toggle recording based on current state
                                let current_state = get_recording_state();
                                match current_state {
                                    RecordingState::Idle | RecordingState::Error => {
                                        println!("Toggle: Starting recording via hotkey");
                                        
                                        // Use Tauri's command system to start recording
                                        let app_handle = app.app_handle().clone();
                                        tauri::async_runtime::spawn(async move {
                                            // Get the recorder state from app handle
                                            let recorder_state = app_handle.state::<RecorderState>();
                                            match start_recording(app_handle.clone(), recorder_state).await {
                                                Ok(_) => println!("Toggle: Recording started successfully"),
                                                Err(e) => {
                                                    println!("Toggle: Error starting recording: {}", e);
                                                    update_recording_state(&app_handle, RecordingState::Error, Some(e));
                                                }
                                            }
                                        });
                                    }
                                    RecordingState::Recording | RecordingState::Starting => {
                                        println!("Toggle: Stopping recording via hotkey");
                                        
                                        // Use Tauri's command system to stop recording
                                        let app_handle = app.app_handle().clone();
                                        tauri::async_runtime::spawn(async move {
                                            // Get the recorder state from app handle
                                            let recorder_state = app_handle.state::<RecorderState>();
                                            match stop_recording(app_handle.clone(), recorder_state).await {
                                                Ok(_) => println!("Toggle: Recording stopped successfully"),
                                                Err(e) => println!("Toggle: Error stopping recording: {}", e),
                                            }
                                        });
                                    }
                                    _ => {
                                        println!("Toggle: Ignoring hotkey in state {:?}", current_state);
                                    }
                                }
                            }
                        }
                    }
                })
                .build(),
        )
        .setup(|app| {
            // Initialize whisper manager
            let models_dir = app.path().app_data_dir()?.join("models");
            println!("Models directory: {:?}", models_dir);

            // Ensure the models directory exists
            std::fs::create_dir_all(&models_dir)
                .map_err(|e| format!("Failed to create models directory: {}", e))?;

            let whisper_manager = whisper::manager::WhisperManager::new(models_dir);
            app.manage(AsyncMutex::new(whisper_manager));

            // NEW: cache for transcribers (keeps models in memory)
            app.manage(AsyncMutex::new(TranscriberCache::new()));

            // Initialize recorder state
            app.manage(RecorderState(Mutex::new(AudioRecorder::new())));

            // Initialize audio file path state
            app.manage(Mutex::new(None::<PathBuf>));

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
                    } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Load hotkey from settings store
            let store = app.store("settings")
                .map_err(|e| e.to_string())?;
            
            let hotkey_str = store.get("hotkey")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "CommandOrControl+Shift+Space".to_string());
            
            println!("Loading hotkey from store: {}", hotkey_str);
            
            // Register global shortcut from settings
            let shortcut: tauri_plugin_global_shortcut::Shortcut = hotkey_str.parse()
                .map_err(|_| "Invalid shortcut format")?;

            // Store the recording shortcut in global state
            RECORDING_SHORTCUT.set(Arc::new(Mutex::new(shortcut.clone())))
                .map_err(|_| "Failed to set recording shortcut")?;
            
            // Initialize recording state (single source of truth)
            RECORDING_STATE.set(Arc::new(Mutex::new(RecordingState::Idle)))
                .map_err(|_| "Failed to set recording state")?;

            app.global_shortcut()
                .register(shortcut)?;

            // Hide window on start (menu bar only)
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
