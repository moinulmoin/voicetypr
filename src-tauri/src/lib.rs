use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use tauri::async_runtime::Mutex as AsyncMutex;

mod audio;
mod whisper;
mod commands;

use commands::{audio::*, model::*, settings::*, text::*};
use audio::recorder::AudioRecorder;
use whisper::cache::TranscriberCache;

// Global state for tracking the recording shortcut
static RECORDING_SHORTCUT: std::sync::OnceLock<Arc<Mutex<tauri_plugin_global_shortcut::Shortcut>>> = std::sync::OnceLock::new();

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

                    // Check if this is the recording shortcut
                    if let Some(recording_shortcut) = RECORDING_SHORTCUT.get() {
                        if let Ok(guard) = recording_shortcut.lock() {
                            if shortcut == &*guard {
                                match event.state() {
                                    ShortcutState::Pressed => {
                                        println!("Recording started via hotkey");
                                        let _ = app.emit("start-recording", ());
                                    }
                                    ShortcutState::Released => {
                                        println!("Recording stopped via hotkey");
                                        let _ = app.emit("stop-recording", ());
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

            // Register default global shortcut
            let shortcut: tauri_plugin_global_shortcut::Shortcut = "CommandOrControl+Shift+Space".parse()
                .map_err(|_| "Invalid shortcut format")?;

            // Store the recording shortcut in global state
            RECORDING_SHORTCUT.set(Arc::new(Mutex::new(shortcut.clone())))
                .map_err(|_| "Failed to set recording shortcut")?;

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
