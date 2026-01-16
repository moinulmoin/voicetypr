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
mod menu;
mod parakeet;
mod recognition;
mod recording;
mod secure_store;
mod simple_cache;
mod state;
mod state_machine;
mod utils;
mod whisper;
mod window_manager;
mod remote;

#[cfg(test)]
mod tests;

// Helper functions for macOS dock icon visibility
#[cfg(target_os = "macos")]
pub fn show_dock_icon(app: &tauri::AppHandle) {
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    log::debug!("Dock icon shown (ActivationPolicy::Regular)");
}

#[cfg(target_os = "macos")]
pub fn hide_dock_icon(app: &tauri::AppHandle) {
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    log::debug!("Dock icon hidden (ActivationPolicy::Accessory)");
}

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
        check_accessibility_permission, check_microphone_permission, open_accessibility_settings,
        request_accessibility_permission, request_microphone_permission,
        test_automation_permission,
    },
    remote::{
        add_remote_server, get_active_remote_server, get_firewall_status, get_local_ips,
        get_local_machine_id, get_sharing_status, list_remote_servers, open_firewall_settings,
        remove_remote_server, set_active_remote_server, start_sharing, stop_sharing,
        test_remote_connection, test_remote_server, transcribe_remote, update_remote_server,
    },
    reset::reset_app_data,
    settings::*,
    stt::{clear_soniox_key_cache, validate_and_cache_soniox_key},
    text::*,
    utils::export_transcriptions,
    window::*,
};
use commands::remote::load_remote_settings;
use remote::lifecycle::RemoteServerManager;
use state::unified_state::UnifiedRecordingState;
use tauri::menu::{CheckMenuItem, MenuBuilder, MenuItem, PredefinedMenuItem, Submenu};
use whisper::cache::TranscriberCache;
use window_manager::WindowManager;

use menu::build_tray_menu;
pub use state::{
    emit_to_all, emit_to_window, flush_pill_event_queue, get_recording_state,
    update_recording_state, AppState, QueuedPillEvent, RecordingMode, RecordingState,
};
pub use recognition::{
    auto_select_model_if_needed, recognition_availability_snapshot, RecognitionAvailabilitySnapshot,
};

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
                file_name: Some(format!("voicetypr-{}", today)),
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
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // When a second instance is launched, bring the existing window to focus
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
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
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init());

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
                    recording::handle_global_shortcut(app.app_handle(), shortcut, event.state());
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

            // Initialize remote transcription state
            app.manage(AsyncMutex::new(RemoteServerManager::new()));
            // Load saved remote settings from store (persists connections across restarts)
            let remote_settings = load_remote_settings(&app.handle());
            let connection_count = remote_settings.saved_connections.len();
            let active_id = remote_settings.active_connection_id.clone();
            let sharing_was_enabled = remote_settings.server_config.enabled;
            let saved_port = remote_settings.server_config.port;
            let saved_password = remote_settings.server_config.password.clone();
            log::info!(
                "🌐 [STARTUP] Remote settings loaded: {} connections, active_connection_id={:?}, sharing_enabled={}",
                connection_count,
                active_id,
                sharing_was_enabled
            );
            app.manage(AsyncMutex::new(remote_settings));
            log::info!("🌐 Remote transcription state initialized ({} saved connections)", connection_count);

            // Auto-start network sharing if it was enabled before app closed
            if sharing_was_enabled {
                let app_handle_for_sharing = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    // Wait for app to fully initialize
                    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

                    log::info!("🌐 [STARTUP] Auto-starting network sharing (was enabled before shutdown)");

                    // Get the required state handles
                    let server_manager = app_handle_for_sharing.state::<AsyncMutex<crate::remote::lifecycle::RemoteServerManager>>();
                    let whisper_manager = app_handle_for_sharing.state::<AsyncRwLock<crate::whisper::manager::WhisperManager>>();

                    // Call start_sharing logic directly (can't use command due to State extraction)
                    let result = async {
                        // Get server name from hostname
                        let server_name = hostname::get()
                            .ok()
                            .and_then(|h| h.into_string().ok())
                            .unwrap_or_else(|| "VoiceTypr Server".to_string());

                        // Get current model from store
                        let store = app_handle_for_sharing
                            .store("settings")
                            .map_err(|e| format!("Failed to access store: {}", e))?;

                        let stored_model = store
                            .get("current_model")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                            .unwrap_or_default();

                        // Get current engine from settings
                        let stored_engine = store
                            .get("current_model_engine")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                            .unwrap_or_else(|| "whisper".to_string());

                        // Get model path from whisper manager
                        let (current_model, model_path) = {
                            let manager = whisper_manager.read().await;

                            let model_name = if stored_model.is_empty() {
                                manager
                                    .get_first_downloaded_model()
                                    .ok_or("No model downloaded")?
                            } else {
                                stored_model
                            };

                            // Get model path for whisper, use empty path for other engines
                            let path = if stored_engine == "whisper" {
                                manager
                                    .get_model_path(&model_name)
                                    .ok_or_else(|| format!("Model '{}' not found", model_name))?
                            } else {
                                std::path::PathBuf::new()
                            };

                            (model_name, path)
                        };

                        // Start the server (pass app handle for Parakeet support)
                        let mut manager = server_manager.lock().await;
                        manager
                            .start(saved_port, saved_password, server_name, model_path, current_model, stored_engine, Some(app_handle_for_sharing.clone()))
                            .await?;

                        Ok::<(), String>(())
                    }.await;

                    match result {
                        Ok(()) => log::info!("🌐 [STARTUP] Network sharing auto-started successfully on port {}", saved_port),
                        Err(e) => log::warn!("🌐 [STARTUP] Failed to auto-start network sharing: {}", e),
                    }
                });
            }

            // Initialize unified application state
            app.manage(AppState::new());
            log::info!("🧠 App state managed and ready");

            // Initialize window manager after app state is managed
            let app_state = app.state::<AppState>();
            let window_manager = WindowManager::new(app.app_handle().clone());
            app_state.set_window_manager(window_manager);

            // Run comprehensive startup checks after state/window manager are ready
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                perform_startup_checks(app_handle).await;
            });

            // Show pill on startup if pill_indicator_mode is "always"
            let app_handle_for_pill = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Wait for frontend to be ready (Vite in dev mode, bundled files in prod)
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

                // Check if pill_indicator_mode setting is "always"
                let pill_mode = if let Ok(store) = app_handle_for_pill.store("settings") {
                    let stored_mode = store
                        .get("pill_indicator_mode")
                        .and_then(|v| v.as_str().map(|s| s.to_string()));
                    let legacy_show = store.get("show_pill_indicator").and_then(|v| v.as_bool());
                    let resolved = crate::commands::settings::resolve_pill_indicator_mode(
                        stored_mode.clone(),
                        legacy_show,
                        crate::commands::settings::Settings::default().pill_indicator_mode,
                    );
                    log::info!(
                        "Startup: pill_indicator_mode resolved='{}' stored={:?} legacy_show={:?}",
                        resolved,
                        stored_mode,
                        legacy_show
                    );
                    resolved
                } else {
                    let default_mode = crate::commands::settings::Settings::default().pill_indicator_mode;
                    log::info!(
                        "Startup: pill_indicator_mode using default='{}' (settings store unavailable)",
                        default_mode
                    );
                    default_mode
                };

                log::info!(
                    "Startup: pill_indicator_mode='{}', will show pill={}",
                    pill_mode,
                    pill_mode == "always"
                );

                // Only show pill on startup if mode is "always"
                if pill_mode == "always" {
                    log::info!("Startup: Showing pill because mode is 'always'");
                    if let Err(e) = crate::commands::window::show_pill_widget(app_handle_for_pill).await {
                        log::warn!("Failed to show pill on startup: {}", e);
                    }
                }
            });

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

            // Create device watcher in deferred state - will be started after mic permission granted
            // This prevents early mic permission prompts from CPAL's input_devices() enumeration
            app.manage(audio::device_watcher::DeviceWatcher::new(app.app_handle().clone()));

            // For returning users (onboarding already complete + mic permission granted),
            // start the device watcher automatically
            let app_handle_for_watcher = app.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                audio::device_watcher::try_start_device_watcher_if_ready(&app_handle_for_watcher).await;
            });

            // Create display watcher to reposition pill/toast on monitor changes
            let display_watcher = utils::display_watcher::DisplayWatcher::new(app.app_handle().clone());
            display_watcher.start();
            app.manage(display_watcher);

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
                            // Show dock icon when main window is shown via tray icon
                            #[cfg(target_os = "macos")]
                            show_dock_icon(app);
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

            // Create pill (macOS) and toast (all platforms) windows at startup
            {
                use tauri::{WebviewUrl, WebviewWindowBuilder};

                // Calculate center-bottom position for pill/toast
                let (pos_x, pos_y) = {
                    let (screen_width, screen_height) = if let Ok(Some(monitor)) = app.primary_monitor() {
                        let size = monitor.size();
                        let scale = monitor.scale_factor();
                        (size.width as f64 / scale, size.height as f64 / scale)
                    } else {
                        (1440.0, 900.0) // Fallback
                    };

                    let pill_width = 80.0;  // Sized for 3-dot pill (active state with padding)
                    let pill_height = 40.0;
                    let bottom_offset = 60.0;  // Distance from bottom of screen (above taskbar)

                    let x = (screen_width - pill_width) / 2.0;
                    let y = screen_height - pill_height - bottom_offset;
                    (x, y)
                };

                // macOS: create pill window and convert to NSPanel
                #[cfg(target_os = "macos")]
                {
                    // Create the pill window - sized for 3 dots
                    // Properties aligned with window_manager.rs for consistency
                    let pill_builder = WebviewWindowBuilder::new(app, "pill", WebviewUrl::App("pill".into()))
                        .title("Recording")
                        .resizable(false)
                        .maximizable(false)
                        .minimizable(false)
                        .decorations(false)
                        .always_on_top(true)
                        .visible_on_all_workspaces(true)
                        .content_protected(true)
                        .skip_taskbar(true)
                        .transparent(true)
                        .shadow(false)  // Prevent window shadow on macOS
                        .inner_size(80.0, 40.0)  // Sized for 3-dot pill (active state with padding)
                        .position(pos_x, pos_y)
                        .visible(true)  // Always visible (controlled by show_pill_indicator setting)
                        .focused(false);  // Don't steal focus

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

                // Create toast window for feedback messages (positioned above pill) - all platforms
                let toast_width = 400.0;
                let toast_height = 80.0;
                let pill_width = 80.0;
                let gap = 8.0; // Gap between pill and toast

                // Center toast above pill
                let toast_x = pos_x + (pill_width - toast_width) / 2.0;
                let toast_y = pos_y - toast_height - gap;
                log::info!("Toast window position: ({}, {}) - above pill at ({}, {})", toast_x, toast_y, pos_x, pos_y);

                let toast_builder = WebviewWindowBuilder::new(app, "toast", WebviewUrl::App("toast".into()))
                    .title("Feedback")
                    .resizable(false)
                    .decorations(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .transparent(true)
                    .inner_size(toast_width, toast_height)
                    .position(toast_x, toast_y)
                    .visible(false); // Starts hidden

                #[cfg(not(debug_assertions))]
                let toast_builder = toast_builder.initialization_script("document.addEventListener('contextmenu', e => e.preventDefault());");

                #[cfg(debug_assertions)]
                let toast_builder = toast_builder;

                let toast_window = toast_builder.build()?;

                // macOS: Convert toast to NSPanel to match pill behavior
                #[cfg(target_os = "macos")]
                {
                    use tauri_nspanel::WebviewWindowExt;
                    toast_window.to_panel().map_err(|e| format!("Failed to convert toast to NSPanel: {:?}", e))?;
                }

                log::info!("Created toast window for feedback");
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
                // Keep dock icon hidden when main window is hidden
                #[cfg(target_os = "macos")]
                hide_dock_icon(&app.app_handle());
            } else {
                log::info!("👋 First launch or no model configured - keeping main window visible");
                // Show dock icon when main window is visible
                #[cfg(target_os = "macos")]
                show_dock_icon(&app.app_handle());
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
            get_recordings_directory,
            open_recordings_folder,
            check_recording_exists,
            get_recording_path,
            save_retranscription,
            update_transcription,
            show_in_folder,
            get_transcription_history,
            get_transcription_count,
            delete_transcription_entry,
            clear_all_transcriptions,
            export_transcriptions,
            show_pill_widget,
            hide_pill_widget,
            close_pill_widget,
            recreate_pill_widget,
            hide_toast_window,
            focus_main_window,
            check_accessibility_permission,
            request_accessibility_permission,
            open_accessibility_settings,
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
            // Remote transcription commands
            start_sharing,
            stop_sharing,
            get_sharing_status,
            get_local_ips,
            get_local_machine_id,
            get_firewall_status,
            open_firewall_settings,
            add_remote_server,
            remove_remote_server,
            update_remote_server,
            list_remote_servers,
            test_remote_connection,
            test_remote_server,
            set_active_remote_server,
            get_active_remote_server,
            transcribe_remote,
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
                            // Hide dock icon when main window is hidden
                            #[cfg(target_os = "macos")]
                            hide_dock_icon(&window.app_handle());
                        }
                    }
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .map_err(|e| -> Box<dyn std::error::Error> {
            log_failed("APPLICATION_BUILD", &format!("Critical error building Tauri application: {}", e));
            log_with_context(log::Level::Error, "Application build failed", &[
                ("stage", "application_build"),
                ("total_startup_time_ms", &app_start.elapsed().as_millis().to_string().as_str())
            ]);
            eprintln!("VoiceTypr failed to start: {}", e);
            Box::new(e)
        })?
        .run(|app_handle, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { has_visible_windows, .. } = event {
                if !has_visible_windows {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                        // Show dock icon when main window is shown
                        show_dock_icon(app_handle);
                    }
                }
            }
        });

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

    let availability = recognition_availability_snapshot(&app).await;
    log_model_operation(
        "AVAILABILITY_CHECK",
        "all",
        if availability.any_available() {
            "AVAILABLE"
        } else {
            "NONE_FOUND"
        },
        None,
    );

    if let Err(err) = app.emit("recognition-availability", availability.clone()) {
        log::warn!("Failed to emit recognition availability event: {}", err);
    }

    if availability.any_available() {
        if let Err(e) = auto_select_model_if_needed(&app, &availability).await {
            log::warn!("Failed to auto-select default model: {}", e);
        }
    }

    if !availability.any_available() {
        log::warn!("⚠️  No speech recognition engines are ready");
        let _ = app.emit("no-models-on-startup", ());
    } else {
        log::info!("✅ At least one speech recognition engine is ready");
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
