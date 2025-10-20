use tauri::{AppHandle, Manager, State};

use crate::audio::recorder::AudioRecorder;
use crate::commands::license::check_license_status_internal;
use crate::commands::settings::get_settings;
use crate::license::LicenseState;
use crate::parakeet::messages::ParakeetResponse;
use crate::parakeet::ParakeetManager;
use crate::utils::logger::*;
#[cfg(debug_assertions)]
use crate::utils::system_monitor;
use crate::whisper::cache::TranscriberCache;
use crate::whisper::languages::validate_language;
use crate::whisper::manager::WhisperManager;
use crate::{emit_to_window, update_recording_state, AppState, RecordingMode, RecordingState};
use cpal::traits::{DeviceTrait, HostTrait};
use serde_json;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tauri_plugin_store::StoreExt;

/// Cached recording configuration to avoid repeated store access during transcription flow
/// Cache is invalidated when settings change via update hooks
#[derive(Clone, Debug)]
pub struct RecordingConfig {
    pub show_pill_widget: bool,
    pub ai_enabled: bool,
    pub ai_provider: String,
    pub ai_model: String,
    pub current_model: String,
    pub current_engine: String,
    pub language: String,
    pub translate_to_english: bool,
    pub show_recording_status: bool,
    // Internal cache metadata
    loaded_at: Instant,
}

impl RecordingConfig {
    /// Maximum age of cache before considering it stale (5 minutes)
    const MAX_CACHE_AGE: std::time::Duration = std::time::Duration::from_secs(5 * 60);

    /// Load all recording-relevant settings from store in one operation
    pub async fn load_from_store(app: &AppHandle) -> Result<Self, String> {
        let store = app.store("settings").map_err(|e| e.to_string())?;

        Ok(Self {
            show_pill_widget: store
                .get("show_pill_widget")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            ai_enabled: store
                .get("ai_enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            ai_provider: store
                .get("ai_provider")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "groq".to_string()),
            ai_model: store
                .get("ai_model")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "".to_string()),
            current_model: store
                .get("current_model")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "".to_string()),
            current_engine: store
                .get("current_model_engine")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "whisper".to_string()),
            language: store
                .get("language")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "en".to_string()),
            translate_to_english: store
                .get("translate_to_english")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            show_recording_status: store
                .get("show_recording_status")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            loaded_at: Instant::now(),
        })
    }

    /// Check if this cache entry is still fresh
    pub fn is_fresh(&self) -> bool {
        self.loaded_at.elapsed() < Self::MAX_CACHE_AGE
    }
}

// Implement UnwindSafe traits for panic testing compatibility
impl UnwindSafe for RecordingConfig {}
impl RefUnwindSafe for RecordingConfig {}

#[derive(Clone)]
enum ActiveEngineSelection {
    Whisper {
        model_name: String,
        model_path: PathBuf,
    },
    Parakeet {
        model_name: String,
    },
    Soniox {
        model_name: String,
    },
}

impl ActiveEngineSelection {
    fn engine_name(&self) -> &'static str {
        match self {
            ActiveEngineSelection::Whisper { .. } => "whisper",
            ActiveEngineSelection::Parakeet { .. } => "parakeet",
            ActiveEngineSelection::Soniox { .. } => "soniox",
        }
    }

    fn model_name(&self) -> &str {
        match self {
            ActiveEngineSelection::Whisper { model_name, .. } => model_name,
            ActiveEngineSelection::Parakeet { model_name } => model_name,
            ActiveEngineSelection::Soniox { model_name } => model_name,
        }
    }
}

async fn abort_due_to_missing_model(
    app: &AppHandle,
    audio_path: &Path,
    log_message: &str,
    user_message: &str,
) -> Result<String, String> {
    log::error!("{}", log_message);
    update_recording_state(app, RecordingState::Error, Some(user_message.to_string()));

    if let Err(e) = std::fs::remove_file(audio_path) {
        log::warn!("Failed to remove audio file: {}", e);
    }

    let _ = emit_to_window(
        app,
        "pill",
        "no-models-error",
        serde_json::json!({
            "title": "No Models Installed",
            "message": user_message,
            "action": "open-settings"
        }),
    );

    if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
        log::error!("Failed to hide pill window: {}", e);
    }

    update_recording_state(app, RecordingState::Idle, None);

    Err(log_message.to_string())
}

async fn resolve_engine_for_model(
    app: &AppHandle,
    model_name: &str,
    engine_hint: Option<&str>,
) -> Result<ActiveEngineSelection, String> {
    let whisper_state = app.state::<AsyncRwLock<WhisperManager>>();
    let parakeet_manager = app.state::<ParakeetManager>();

    match engine_hint.map(|e| e.to_lowercase()) {
        Some(ref engine) if engine == "soniox" => {
            if crate::secure_store::secure_has(app, "stt_api_key_soniox").unwrap_or(false) {
                Ok(ActiveEngineSelection::Soniox {
                    model_name: model_name.to_string(),
                })
            } else {
                Err("Soniox token not configured. Please configure it in Models.".to_string())
            }
        }
        Some(ref engine) if engine == "parakeet" => {
            let status = parakeet_manager
                .list_models()
                .into_iter()
                .find(|m| m.name == model_name);

            match status {
                Some(info) if info.downloaded => Ok(ActiveEngineSelection::Parakeet {
                    model_name: model_name.to_string(),
                }),
                Some(_) => Err(format!(
                    "Parakeet model '{}' is not downloaded. Please download it first.",
                    model_name
                )),
                None => Err(format!(
                    "Parakeet model '{}' not found in registry.",
                    model_name
                )),
            }
        }
        Some(ref engine) if engine == "whisper" || engine == "whisper.cpp" => {
            let path = whisper_state
                .read()
                .await
                .get_model_path(model_name)
                .ok_or_else(|| format!("Whisper model '{}' not found", model_name))?;

            Ok(ActiveEngineSelection::Whisper {
                model_name: model_name.to_string(),
                model_path: path,
            })
        }
        Some(engine) => Err(format!("Unknown model engine '{}'.", engine)),
        None => {
            if model_name == "soniox" {
                if crate::secure_store::secure_has(app, "stt_api_key_soniox").unwrap_or(false) {
                    return Ok(ActiveEngineSelection::Soniox {
                        model_name: model_name.to_string(),
                    });
                } else {
                    return Err(
                        "Soniox token not configured. Please configure it in Models.".to_string(),
                    );
                }
            }
            if let Some(path) = whisper_state.read().await.get_model_path(model_name) {
                return Ok(ActiveEngineSelection::Whisper {
                    model_name: model_name.to_string(),
                    model_path: path,
                });
            }

            let status = parakeet_manager
                .list_models()
                .into_iter()
                .find(|m| m.name == model_name);

            if let Some(info) = status {
                if info.downloaded {
                    return Ok(ActiveEngineSelection::Parakeet {
                        model_name: model_name.to_string(),
                    });
                } else {
                    return Err(format!(
                        "Model '{}' is a Parakeet model but not downloaded. Please download it first.",
                        model_name
                    ));
                }
            }

            Err(format!(
                "Model '{}' not found in Whisper or Parakeet registries",
                model_name
            ))
        }
    }
}

/// Helper function to invalidate recording config cache when settings change
pub async fn invalidate_recording_config_cache(app: &AppHandle) {
    let app_state = app.state::<AppState>();
    let mut cache = app_state.recording_config_cache.write().await;
    *cache = None;
    log::debug!("Recording config cache invalidated due to settings change");
}

/// Helper function to get cached recording config or load from store
pub async fn get_recording_config(app: &AppHandle) -> Result<RecordingConfig, String> {
    let app_state = app.state::<AppState>();

    // Try to get from cache first
    {
        let cache = app_state.recording_config_cache.read().await;
        if let Some(config) = cache.as_ref() {
            if config.is_fresh() {
                log::debug!(
                    "Using cached recording config (age: {:?})",
                    config.loaded_at.elapsed()
                );
                return Ok(config.clone());
            } else {
                log::debug!("Recording config cache is stale, will reload");
            }
        }
    }

    // Cache miss or stale - load from store
    let config = RecordingConfig::load_from_store(app).await?;

    // Update cache
    {
        let mut cache = app_state.recording_config_cache.write().await;
        *cache = Some(config.clone());
        log::debug!("Recording config cached successfully");
    }

    Ok(config)
}

// Global audio recorder state
pub struct RecorderState(pub Mutex<AudioRecorder>);

/// Select the best fallback model based on available models
/// Prioritizes models by size (smaller to larger for better performance)
fn select_best_fallback_model(
    available_models: &[String],
    requested: &str,
    model_priority: &[String],
) -> String {
    // First try to find a model similar to the requested one
    if !requested.is_empty() {
        // If requested "large-v3", try other large variants first
        for model in available_models {
            if model.starts_with(&requested.split('-').next().unwrap_or(requested)) {
                return model.clone();
            }
        }
    }

    // Otherwise use priority order from WhisperManager
    for priority_model in model_priority {
        if available_models.contains(priority_model) {
            return priority_model.clone();
        }
    }

    // If no priority model found, return first available
    available_models
        .first()
        .map(|s| s.clone())
        .unwrap_or_else(|| {
            log::error!("No models available for fallback selection");
            // This should never happen as we check for empty models before calling this function
            // But return a default to prevent panic
            "base.en".to_string()
        })
}

/// Pre-recording validation using the readiness state
async fn validate_recording_requirements(app: &AppHandle) -> Result<(), String> {
    // Check if any models are downloaded
    let whisper_manager = app.state::<AsyncRwLock<WhisperManager>>();
    let has_whisper_models = whisper_manager.read().await.has_downloaded_models();
    let parakeet_manager = app.state::<ParakeetManager>();
    let has_parakeet_models = parakeet_manager
        .list_models()
        .into_iter()
        .any(|m| m.downloaded);

    // Consider cloud Soniox as satisfying availability when selected and configured
    let (is_soniox_selected, soniox_ready) = {
        match app.store("settings") {
            Ok(store) => {
                let engine = store
                    .get("current_model_engine")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| "whisper".to_string());
                if engine == "soniox" {
                    let has_key =
                        crate::secure_store::secure_has(app, "stt_api_key_soniox").unwrap_or(false);
                    (true, has_key)
                } else {
                    (false, false)
                }
            }
            Err(_) => (false, false),
        }
    };

    let has_models =
        has_whisper_models || has_parakeet_models || (is_soniox_selected && soniox_ready);

    if !has_models {
        log::error!("No models downloaded");
        // Emit error event with guidance
        let _ = emit_to_window(
            app,
            "main",
            "no-models-error",
            serde_json::json!({
                "title": "No Speech Recognition Models",
                "message": if is_soniox_selected { "Please configure your Soniox token in Models before recording." } else { "Please download at least one model from Models before recording." },
                "action": "open-settings"
            }),
        );
        return Err(if is_soniox_selected {
            "Soniox token missing".to_string()
        } else {
            "No speech recognition models installed. Please download a model first.".to_string()
        });
    }

    // Check license status (with caching to improve performance)
    let license_status = {
        let app_state = app.state::<AppState>();
        let cache = app_state.license_cache.read().await;

        if let Some(cached) = cache.as_ref() {
            if cached.is_valid() {
                log::debug!("Using cached license status (age: {:?})", cached.age());
                Some(cached.status.clone())
            } else {
                log::debug!(
                    "License cache is stale (age: {:?}), will refresh",
                    cached.age()
                );
                None
            }
        } else {
            log::debug!("No license cache found, will perform fresh check");
            None
        }
    };

    let status = if let Some(cached_status) = license_status {
        cached_status
    } else {
        // Cache miss or stale - perform fresh license check
        match check_license_status_internal(app).await {
            Ok(fresh_status) => {
                // Update cache
                let app_state = app.state::<AppState>();
                let mut cache = app_state.license_cache.write().await;
                *cache = Some(crate::commands::license::CachedLicense::new(
                    fresh_status.clone(),
                ));
                log::debug!("License status cached for 6 hours");
                fresh_status
            }
            Err(e) => {
                log::error!("Failed to check license status: {}", e);
                // Allow recording if license check fails (graceful degradation)
                return Ok(());
            }
        }
    };

    if matches!(status.status, LicenseState::Expired | LicenseState::None) {
        log::error!("Invalid license: {:?}", status.status);

        // Show and focus the main window
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }

        // Emit error event with guidance
        let _ = emit_to_window(
            app,
            "main",
            "license-required",
            serde_json::json!({
                "title": "License Required",
                "message": "Your trial has expired. Please purchase a license to continue",
                "action": "purchase"
            }),
        );
        return Err("License required to record".to_string());
    }

    Ok(())
}

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, RecorderState>,
) -> Result<(), String> {
    let recording_start = Instant::now();

    log_start("RECORDING_START");
    log_with_context(
        log::Level::Debug,
        "Recording command started",
        &[
            ("command", "start_recording"),
            ("timestamp", &chrono::Utc::now().to_rfc3339()),
        ],
    );

    // If we're stuck in Error, recover to Idle before attempting a new start
    let current_state = crate::get_recording_state(&app);
    if matches!(current_state, crate::RecordingState::Error) {
        crate::update_recording_state(
            &app,
            crate::RecordingState::Idle,
            Some("recover".to_string()),
        );
    }

    // Validate all requirements upfront
    let validation_start = Instant::now();
    match validate_recording_requirements(&app).await {
        Ok(_) => {
            log_performance(
                "RECORDING_VALIDATION",
                validation_start.elapsed().as_millis() as u64,
                Some("validation_passed"),
            );
        }
        Err(e) => {
            log_failed("RECORDING_START", &e);
            log_with_context(
                log::Level::Debug,
                "Validation failed",
                &[
                    ("stage", "validation"),
                    (
                        "validation_time_ms",
                        &validation_start.elapsed().as_millis().to_string().as_str(),
                    ),
                ],
            );
            return Err(e);
        }
    }

    // All validation passed, update state to starting
    log_state_transition("RECORDING", "idle", "starting", true, None);
    update_recording_state(&app, RecordingState::Starting, None);
    // Ensure transition actually happened; if blocked, abort early
    if !matches!(
        crate::get_recording_state(&app),
        crate::RecordingState::Starting
    ) {
        return Err("Cannot start recording in current state".to_string());
    }

    // Load recording config once to avoid repeated store access
    let config = get_recording_config(&app).await.map_err(|e| {
        log::error!("Failed to load recording config: {}", e);
        format!("Configuration error: {}", e)
    })?;
    log::debug!(
        "Using recording config: show_pill={}, ai_enabled={}, model={}",
        config.show_pill_widget,
        config.ai_enabled,
        config.current_model
    );
    // Get app data directory for recordings
    let recordings_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("recordings");

    // Ensure recordings directory exists
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|e| format!("Failed to create recordings directory: {}", e))?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("Time error: {}", e))?
        .as_secs();
    let audio_path = recordings_dir.join(format!("recording_{}.wav", timestamp));

    // Store path for later use
    let app_state = app.state::<AppState>();
    app_state
        .current_recording_path
        .lock()
        .map_err(|e| format!("Failed to acquire path lock: {}", e))?
        .replace(audio_path.clone());

    // Get selected microphone from settings (before acquiring recorder lock)
    let selected_microphone = match get_settings(app.clone()).await {
        Ok(settings) => {
            if let Some(mic) = settings.selected_microphone {
                log::info!("Using selected microphone: {}", mic);
                Some(mic)
            } else {
                log::info!("Using default microphone");
                None
            }
        }
        Err(e) => {
            log::warn!(
                "Failed to get settings for microphone selection: {}. Using default.",
                e
            );
            None
        }
    };

    // Start recording (scoped to release mutex before async operations)
    {
        let mut recorder = state
            .inner()
            .0
            .lock()
            .map_err(|e| format!("Failed to acquire recorder lock: {}", e))?;

        // Check if already recording
        if recorder.is_recording() {
            log::warn!("Already recording!");
            return Err("Already recording".to_string());
        }

        // Log the current audio device before starting
        log_start("AUDIO_DEVICE_CHECK");
        log_with_context(
            log::Level::Debug,
            "Checking audio device",
            &[("stage", "pre_recording")],
        );

        if let Ok(host) = std::panic::catch_unwind(|| cpal::default_host()) {
            if let Some(device) = host.default_input_device() {
                if let Ok(name) = device.name() {
                    log::info!("🎙️ Audio device available: {}", name);
                    log_with_context(
                        log::Level::Info,
                        "🎮 MICROPHONE",
                        &[("device_name", &name), ("status", "available")],
                    );
                } else {
                    log::warn!("⚠️  Could not get device name, but device is available");
                    log_with_context(
                        log::Level::Info,
                        "🎮 MICROPHONE",
                        &[("status", "available_unnamed")],
                    );
                }
            } else {
                log_failed("AUDIO_DEVICE", "No default input device found");
                log_with_context(
                    log::Level::Debug,
                    "Device detection failed",
                    &[("component", "audio_device"), ("stage", "device_detection")],
                );
            }
        }

        // Try to start recording with graceful error handling
        let recorder_init_start = Instant::now();
        let audio_path_str = audio_path
            .to_str()
            .ok_or_else(|| "Invalid path encoding".to_string())?;

        log_file_operation("RECORDING_START", audio_path_str, false, None, None);

        // Start recording and get audio level receiver
        let audio_level_rx = match recorder
            .start_recording(audio_path_str, selected_microphone.clone())
        {
            Ok(_) => {
                // Verify recording actually started
                let is_recording = recorder.is_recording();

                // Get the audio level receiver before potentially dropping recorder
                let rx = recorder.take_audio_level_receiver();

                if !is_recording {
                    drop(recorder); // Release the lock if we're erroring out
                    log_failed(
                        "RECORDER_INIT",
                        "Recording failed to start after initialization",
                    );
                    log_with_context(
                        log::Level::Debug,
                        "Recorder initialization failed",
                        &[
                            ("audio_path", audio_path_str),
                            (
                                "init_time_ms",
                                &recorder_init_start
                                    .elapsed()
                                    .as_millis()
                                    .to_string()
                                    .as_str(),
                            ),
                        ],
                    );

                    update_recording_state(
                        &app,
                        RecordingState::Error,
                        Some("Microphone initialization failed".to_string()),
                    );

                    // Emit user-friendly error
                    let _ =
                        emit_to_window(&app, "pill", "recording-error", "Microphone access failed");

                    return Err("Failed to start recording".to_string());
                } else {
                    log_performance(
                        "RECORDER_INIT",
                        recorder_init_start.elapsed().as_millis() as u64,
                        Some(&format!("file={}", audio_path_str)),
                    );
                    log::info!("✅ Recording started successfully");

                    // Monitor system resources at recording start
                    #[cfg(debug_assertions)]
                    system_monitor::log_resources_before_operation("RECORDING_START");
                }

                rx // Return the audio level receiver
            }
            Err(e) => {
                log_failed("RECORDER_START", &e);
                log_with_context(
                    log::Level::Debug,
                    "Recorder start failed",
                    &[
                        ("audio_path", audio_path_str),
                        (
                            "init_time_ms",
                            &recorder_init_start
                                .elapsed()
                                .as_millis()
                                .to_string()
                                .as_str(),
                        ),
                    ],
                );

                update_recording_state(&app, RecordingState::Error, Some(e.to_string()));

                // Provide specific error messages for common issues
                let user_message = if e.contains("permission") || e.contains("access") {
                    "Microphone permission denied"
                } else if e.contains("device") || e.contains("not found") {
                    "No microphone found"
                } else if e.contains("in use") || e.contains("busy") {
                    "Microphone busy"
                } else {
                    "Recording failed"
                };

                let _ = emit_to_window(&app, "pill", "recording-error", user_message);

                return Err(e);
            }
        };

        // Release the recorder lock after successful start
        drop(recorder);

        // Start audio level monitoring
        if let Some(audio_level_rx) = audio_level_rx {
            let app_for_levels = app.clone();
            // Use a thread instead of tokio spawn for std::sync::mpsc
            std::thread::spawn(move || {
                let mut last_emit = std::time::Instant::now();
                let emit_interval = std::time::Duration::from_millis(100); // Throttle to 10fps
                let mut last_emitted_level = 0.0f64;
                const LEVEL_CHANGE_THRESHOLD: f64 = 0.05; // Only emit if change > 5%

                while let Ok(level) = audio_level_rx.recv() {
                    // Check both time throttling and significant change
                    let level_changed = (level - last_emitted_level).abs() > LEVEL_CHANGE_THRESHOLD;

                    if last_emit.elapsed() >= emit_interval && level_changed {
                        // Only emit to pill window - main window doesn't need audio levels
                        let _ = emit_to_window(&app_for_levels, "pill", "audio-level", level);
                        last_emit = std::time::Instant::now();
                        last_emitted_level = level;
                    }
                }
            });
        }
    } // MutexGuard dropped here

    // Now perform async operations after mutex is released

    // Clear cancellation flag for new recording
    let app_state = app.state::<AppState>();
    app_state.clear_cancellation();

    // Update state to recording
    update_recording_state(&app, RecordingState::Recording, None);

    // Show pill widget if enabled (graceful degradation)
    if config.show_pill_widget {
        match crate::commands::window::show_pill_widget(app.clone()).await {
            Ok(_) => log::debug!("Pill widget shown successfully"),
            Err(e) => {
                log::warn!("Failed to show pill widget: {}. Recording will continue without visual feedback.", e);

                // Emit event so frontend knows pill isn't visible
                let _ = emit_to_window(
                    &app,
                    "main",
                    "pill-widget-error",
                    "Recording indicator unavailable. Recording is still active.",
                );
            }
        }
    }

    // Also emit legacy event for compatibility
    let _ = emit_to_window(&app, "pill", "recording-started", ());

    // Log successful recording start
    log_complete(
        "RECORDING_START",
        recording_start.elapsed().as_millis() as u64,
    );
    log_with_context(
        log::Level::Debug,
        "Recording started successfully",
        &[
            ("audio_path", &format!("{:?}", audio_path).as_str()),
            ("state", "recording"),
        ],
    );

    // Register global ESC key for cancellation
    let app_state = app.state::<AppState>();
    let escape_shortcut: tauri_plugin_global_shortcut::Shortcut = "Escape"
        .parse()
        .map_err(|e| format!("Failed to parse ESC shortcut: {:?}", e))?;

    log::info!("Attempting to register ESC shortcut: {:?}", escape_shortcut);

    // Clear ESC state
    app_state
        .esc_pressed_once
        .store(false, std::sync::atomic::Ordering::SeqCst);

    // Cancel any existing ESC timeout
    if let Ok(mut timeout_guard) = app_state.esc_timeout_handle.lock() {
        if let Some(handle) = timeout_guard.take() {
            handle.abort();
        }
    }

    // Register the ESC key globally
    match app.global_shortcut().register(escape_shortcut.clone()) {
        Ok(_) => {
            log::info!("Successfully registered global ESC key for recording cancellation");
        }
        Err(e) => {
            log::error!("Failed to register ESC shortcut: {}", e);
            // Don't fail recording start if ESC registration fails
            log::warn!("Recording will continue without ESC cancellation support");
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: State<'_, RecorderState>,
) -> Result<String, String> {
    let stop_start = Instant::now();

    log_start("RECORDING_STOP");
    log_with_context(
        log::Level::Debug,
        "Stop recording command",
        &[
            ("command", "stop_recording"),
            ("timestamp", &chrono::Utc::now().to_rfc3339().as_str()),
        ],
    );

    // Update state to stopping
    log_state_transition("RECORDING", "recording", "stopping", true, None);
    update_recording_state(&app, RecordingState::Stopping, None);
    // DO NOT request cancellation here - we want transcription to complete!
    // Cancellation should only happen in cancel_recording command

    // Stop recording (lock only within this scope to stay Send)
    log::info!("🛑 Stopping recording...");
    {
        let mut recorder = state
            .inner()
            .0
            .lock()
            .map_err(|e| format!("Failed to acquire recorder lock: {}", e))?;

        // Check if actually recording first
        if !recorder.is_recording() {
            log::warn!("stop_recording called but not currently recording");
            // Don't error - just return empty result, but make sure to reset state
            drop(recorder); // Drop the lock before updating state
            update_recording_state(&app, RecordingState::Idle, None);
            return Ok("".to_string());
        }

        let stop_message = recorder
            .stop_recording()
            .map_err(|e| format!("Failed to stop recording: {}", e))?;
        log::info!("{}", stop_message);

        // Monitor system resources after recording stop
        #[cfg(debug_assertions)]
        system_monitor::log_resources_after_operation(
            "RECORDING_STOP",
            stop_start.elapsed().as_millis() as u64,
        );

        // Emit event if recording was stopped due to silence
        if stop_message.contains("silence") {
            let _ = emit_to_window(&app, "pill", "recording-stopped-silence", ());
        }
    } // MutexGuard dropped here BEFORE any await

    // Unregister ESC key
    match "Escape".parse::<tauri_plugin_global_shortcut::Shortcut>() {
        Ok(escape_shortcut) => {
            if let Err(e) = app.global_shortcut().unregister(escape_shortcut) {
                log::debug!(
                    "Failed to unregister ESC shortcut (might not have been registered): {}",
                    e
                );
            } else {
                log::info!("Unregistered ESC shortcut");
            }
        }
        Err(e) => {
            log::debug!("Failed to parse ESC shortcut for unregistration: {:?}", e);
        }
    }

    // Clean up ESC state
    let app_state = app.state::<AppState>();
    app_state
        .esc_pressed_once
        .store(false, std::sync::atomic::Ordering::SeqCst);

    // Cancel any ESC timeout
    if let Ok(mut timeout_guard) = app_state.esc_timeout_handle.lock() {
        if let Some(handle) = timeout_guard.take() {
            handle.abort();
        }
    }

    log::debug!("Unregistered ESC key and cleaned up state");

    // Check if cancellation was requested
    if app_state.is_cancellation_requested() {
        log::info!("Recording was cancelled, skipping transcription");

        // Clean up audio file if it exists
        if let Ok(path_guard) = app_state.current_recording_path.lock() {
            if let Some(audio_path) = path_guard.as_ref() {
                log::info!("Removing cancelled recording file");
                if let Err(e) = std::fs::remove_file(audio_path) {
                    log::warn!("Failed to remove cancelled recording: {}", e);
                }
            }
        }

        // Hide pill window
        if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
            log::error!("Failed to hide pill window: {}", e);
        }

        // Transition to idle
        update_recording_state(&app, RecordingState::Idle, None);

        return Ok("".to_string());
    }

    // Get the audio file path
    let audio_path = app_state
        .current_recording_path
        .lock()
        .map_err(|e| format!("Failed to acquire path lock: {}", e))?
        .take();

    // If no audio path, there was no recording
    let audio_path = match audio_path {
        Some(path) => {
            // Check if file exists and has content
            if let Ok(metadata) = std::fs::metadata(&path) {
                log::debug!("Audio file size: {} bytes", metadata.len());
            } else {
                log::error!("Audio file does not exist at path: {:?}", path);
            }
            path
        }
        None => {
            log::warn!("No audio file found - no recording was made");
            // Make sure to transition back to Idle state
            update_recording_state(&app, RecordingState::Idle, None);
            return Ok("".to_string());
        }
    };

    // Fast-path: handle header-only/empty WAV files before normalization
    if let Ok(meta) = std::fs::metadata(&audio_path) {
        // A valid WAV header is typically 44 bytes; <= 44 implies no audio samples were written
        if meta.len() <= 44 {
            let _ = emit_to_window(&app, "pill", "recording-too-short", "No audio captured");
            if let Err(e) = std::fs::remove_file(&audio_path) {
                log::debug!("Failed to remove empty audio file: {}", e);
            }
            if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
                log::error!("Failed to hide pill window: {}", e);
            }
            update_recording_state(&app, RecordingState::Idle, None);
            return Ok("".to_string());
        }
    }

    // Decide engine early to optionally skip normalization for Soniox
    let config = get_recording_config(&app).await.map_err(|e| {
        log::error!("Failed to load recording config: {}", e);
        format!("Configuration error: {}", e)
    })?;

    let whisper_manager = app.state::<AsyncRwLock<WhisperManager>>();

    let engine_selection = match config.current_engine.as_str() {
        "parakeet" => {
            if config.current_model.is_empty() {
                return abort_due_to_missing_model(
                    &app,
                    &audio_path,
                    "No Parakeet model selected",
                    "Please select a Parakeet model before recording.",
                )
                .await;
            }

            let parakeet_manager = app.state::<ParakeetManager>();
            let models = parakeet_manager.list_models();
            if let Some(status) = models.into_iter().find(|m| m.name == config.current_model) {
                if !status.downloaded {
                    return abort_due_to_missing_model(
                        &app,
                        &audio_path,
                        "Selected Parakeet model is not downloaded",
                        "Please download the selected Parakeet model before recording.",
                    )
                    .await;
                }
            } else {
                return abort_due_to_missing_model(
                    &app,
                    &audio_path,
                    "Selected Parakeet model is not available",
                    "The selected Parakeet model is unavailable. Please download it again.",
                )
                .await;
            }

            ActiveEngineSelection::Parakeet {
                model_name: config.current_model.clone(),
            }
        }
        "soniox" => {
            if config.current_model.is_empty() {
                return abort_due_to_missing_model(
                    &app,
                    &audio_path,
                    "No Soniox model selected",
                    "Please select the Soniox cloud model before recording.",
                )
                .await;
            }

            if !crate::secure_store::secure_has(&app, "stt_api_key_soniox").unwrap_or(false) {
                return abort_due_to_missing_model(
                    &app,
                    &audio_path,
                    "Soniox token not configured",
                    "Please configure your Soniox token in Models before recording.",
                )
                .await;
            }

            ActiveEngineSelection::Soniox {
                model_name: config.current_model.clone(),
            }
        }
        _ => {
            let downloaded_models = whisper_manager.read().await.get_downloaded_model_names();
            log::debug!("Downloaded Whisper models: {:?}", downloaded_models);

            if downloaded_models.is_empty() {
                return abort_due_to_missing_model(
                    &app,
                    &audio_path,
                    "No speech recognition models installed",
                    "Please download at least one speech recognition model from Models to use VoiceTypr.",
                )
                .await;
            }

            log_start("MODEL_SELECTION");
            log_with_context(
                log::Level::Debug,
                "Selecting model",
                &[(
                    "available_count",
                    &downloaded_models.len().to_string().as_str(),
                )],
            );

            let configured_model = if !config.current_model.is_empty() {
                Some(config.current_model.clone())
            } else {
                None
            };

            let chosen_model = if let Some(configured_model) = configured_model {
                if downloaded_models.contains(&configured_model) {
                    log_model_operation(
                        "SELECTION",
                        &configured_model,
                        "CONFIGURED_AVAILABLE",
                        None,
                    );
                    configured_model
                } else {
                    let models_by_size = whisper_manager.read().await.get_models_by_size();
                    let fallback_model = select_best_fallback_model(
                        &downloaded_models,
                        &configured_model,
                        &models_by_size,
                    );

                    log_model_operation(
                        "FALLBACK",
                        &fallback_model,
                        "SELECTED",
                        Some(&{
                            let mut ctx = std::collections::HashMap::new();
                            ctx.insert("requested".to_string(), configured_model.clone());
                            ctx.insert(
                                "reason".to_string(),
                                "configured_not_available".to_string(),
                            );
                            ctx
                        }),
                    );

                    let _ = emit_to_window(
                        &app,
                        "pill",
                        "model-fallback",
                        serde_json::json!({
                            "requested": configured_model,
                            "fallback": fallback_model
                        }),
                    );

                    fallback_model
                }
            } else {
                let models_by_size = whisper_manager.read().await.get_models_by_size();
                let best_model =
                    select_best_fallback_model(&downloaded_models, "", &models_by_size);

                log_model_operation(
                    "AUTO_SELECTION",
                    &best_model,
                    "SELECTED",
                    Some(&{
                        let mut ctx = std::collections::HashMap::new();
                        ctx.insert("reason".to_string(), "no_model_configured".to_string());
                        ctx.insert("strategy".to_string(), "best_available".to_string());
                        ctx
                    }),
                );

                best_model
            };

            let model_path = whisper_manager
                .read()
                .await
                .get_model_path(&chosen_model)
                .ok_or_else(|| format!("Model '{}' path not found", chosen_model))?;

            ActiveEngineSelection::Whisper {
                model_name: chosen_model,
                model_path,
            }
        }
    };

    // For Whisper/Parakeet: normalize and duration gate; for Soniox: skip both
    let audio_path = match &engine_selection {
        ActiveEngineSelection::Soniox { .. } => {
            log::info!("[RECORD] Soniox selected — skipping normalization");
            audio_path
        }
        _ => {
            // Normalize captured audio to Whisper contract (WAV PCM s16, mono, 16k) via ffmpeg sidecar
            let parent_dir = audio_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::Path::new(".").to_path_buf());

            let normalized_path = {
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let out_path = parent_dir.join(format!("normalized_{}.wav", ts));
                if let Err(e) =
                    crate::ffmpeg::normalize_streaming(&app, &audio_path, &out_path).await
                {
                    log::error!("Audio normalization (ffmpeg) failed: {}", e);
                    update_recording_state(
                        &app,
                        RecordingState::Error,
                        Some("Audio normalization failed".to_string()),
                    );
                    let _ = std::fs::remove_file(&audio_path);
                    return Err("Audio normalization failed".to_string());
                }
                out_path
            };

            // Remove raw capture after successful normalization
            if let Err(e) = std::fs::remove_file(&audio_path) {
                log::debug!("Failed to remove raw audio: {}", e);
            }

            // Determine min duration based on recording mode (PTT vs Toggle) once
            let (min_duration_s_f32, min_duration_s_i32) = {
                let app_state = app.state::<AppState>();
                let mode = app_state
                    .recording_mode
                    .lock()
                    .ok()
                    .map(|g| *g)
                    .unwrap_or(RecordingMode::Toggle);
                match mode {
                    RecordingMode::PushToTalk => (1.0f32, 1i32),
                    RecordingMode::Toggle => (3.0f32, 3i32),
                }
            };

            // Duration gate (mode-specific) using normalized file
            let too_short = (|| -> Result<bool, String> {
                let reader = hound::WavReader::open(&normalized_path)
                    .map_err(|e| format!("Failed to open normalized wav: {}", e))?;
                let spec = reader.spec();
                let frames = reader.duration() / spec.channels as u32; // mono expected
                let duration = frames as f32 / spec.sample_rate as f32;
                log_with_context(
                    log::Level::Info,
                    "NORMALIZED_AUDIO",
                    &[
                        ("path", &format!("{:?}", normalized_path).as_str()),
                        ("sample_rate", &spec.sample_rate.to_string().as_str()),
                        ("channels", &spec.channels.to_string().as_str()),
                        ("bits", &spec.bits_per_sample.to_string().as_str()),
                        ("duration_s", &format!("{:.2}", duration).as_str()),
                    ],
                );
                Ok(duration < min_duration_s_f32)
            })();

            if let Ok(true) = too_short {
                // Emit friendly feedback and stop here
                let _ = emit_to_window(
                    &app,
                    "pill",
                    "recording-too-short",
                    format!("Recording shorter than {} seconds", min_duration_s_i32),
                );
                if let Err(e) = std::fs::remove_file(&normalized_path) {
                    log::debug!("Failed to remove short normalized audio: {}", e);
                }
                // Hide pill and return to Idle
                if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
                    log::error!("Failed to hide pill window: {}", e);
                }
                update_recording_state(&app, RecordingState::Idle, None);
                return Ok("".to_string());
            }

            normalized_path
        }
    };

    log_with_context(
        log::Level::Debug,
        "Proceeding to transcription",
        &[
            ("audio_path", &format!("{:?}", audio_path).as_str()),
            ("stage", "pre_transcription"),
        ],
    );
    log::debug!(
        "Using cached config: model={}, language={}, translate={}, ai_enabled={}",
        config.current_model,
        config.language,
        config.translate_to_english,
        config.ai_enabled
    );

    let language = if config.language.is_empty() {
        None
    } else {
        Some(config.language.clone())
    };
    let translate_to_english = config.translate_to_english;

    let engine_label = engine_selection.engine_name().to_string();
    let selected_model_name = engine_selection.model_name().to_string();

    log::info!(
        "🤖 Using {} model for transcription: {}",
        engine_label,
        selected_model_name
    );
    log::info!(
        "[LANGUAGE] stop_recording: language={:?}, translate={}",
        language.as_deref(),
        translate_to_english
    );

    let audio_path_clone = audio_path.clone();
    let engine_selection_for_task = engine_selection;
    let language_for_task = language.clone();
    let selected_model_name_for_task = selected_model_name.clone();

    // Spawn and track the transcription task
    let app_for_task = app.clone();
    let task_handle = tokio::spawn(async move {
        log::debug!("Transcription task started");

        // Update state to transcribing
        update_recording_state(&app_for_task, RecordingState::Transcribing, None);
        // Also emit legacy event to pill window
        let _ = emit_to_window(&app_for_task, "pill", "transcription-started", ());

        // Check for cancellation before loading model
        let app_state = app_for_task.state::<AppState>();
        if app_state.is_cancellation_requested() {
            log::info!("Transcription cancelled before model loading");

            // Hide pill window since we're cancelling
            if let Err(e) = crate::commands::window::hide_pill_widget(app_for_task.clone()).await {
                log::error!("Failed to hide pill window on cancellation: {}", e);
            }

            update_recording_state(&app_for_task, RecordingState::Idle, None);
            return;
        }

        let transcription_result: Result<String, String> = match &engine_selection_for_task {
            ActiveEngineSelection::Whisper { model_path, .. } => {
                let transcriber = {
                    let cache_state = app_for_task.state::<AsyncMutex<TranscriberCache>>();
                    let mut cache = cache_state.lock().await;
                    match cache.get_or_create(model_path) {
                        Ok(t) => t,
                        Err(e) => {
                            update_recording_state(
                                &app_for_task,
                                RecordingState::Error,
                                Some(e.clone()),
                            );
                            let _ = crate::commands::window::hide_pill_widget(app_for_task.clone())
                                .await;
                            let _ = emit_to_window(&app_for_task, "pill", "transcription-error", e);
                            return;
                        }
                    }
                };

                const MAX_RETRIES: u32 = 3;
                const RETRY_DELAY_MS: u64 = 500;

                let mut result = Err("No attempt made".to_string());

                for attempt in 1..=MAX_RETRIES {
                    if app_state.is_cancellation_requested() {
                        log::info!("Transcription cancelled at attempt {}", attempt);
                        result = Err("Transcription cancelled".to_string());
                        break;
                    }

                    result = transcriber.transcribe_with_cancellation(
                        &audio_path_clone,
                        language_for_task.as_deref(),
                        translate_to_english,
                        || app_state.is_cancellation_requested(),
                    );

                    match &result {
                        Ok(_) => {
                            if attempt > 1 {
                                log::info!("Transcription succeeded on attempt {}", attempt);
                            }
                            break;
                        }
                        Err(e) => {
                            if attempt < MAX_RETRIES {
                                log::warn!(
                                    "Transcription attempt {} failed: {}. Retrying in {}ms...",
                                    attempt,
                                    e,
                                    RETRY_DELAY_MS
                                );
                                tokio::time::sleep(std::time::Duration::from_millis(
                                    RETRY_DELAY_MS,
                                ))
                                .await;
                            } else {
                                log::error!(
                                    "Transcription failed after {} attempts: {}",
                                    MAX_RETRIES,
                                    e
                                );
                            }
                        }
                    }
                }

                result
            }
            ActiveEngineSelection::Parakeet { model_name } => {
                let parakeet_manager = app_for_task.state::<ParakeetManager>();
                if let Err(e) = parakeet_manager.load_model(&app_for_task, model_name).await {
                    let message = format!("Parakeet model load failed: {e}");
                    update_recording_state(
                        &app_for_task,
                        RecordingState::Error,
                        Some(message.clone()),
                    );
                    let _ = emit_to_window(&app_for_task, "pill", "transcription-error", message);
                    return;
                }

                match parakeet_manager
                    .transcribe(
                        &app_for_task,
                        model_name,
                        audio_path_clone.clone(),
                        language_for_task.clone(),
                        translate_to_english,
                    )
                    .await
                {
                    Ok(ParakeetResponse::Transcription { text, .. }) => Ok(text),
                    Ok(other) => {
                        let message = format!("Unexpected Parakeet response: {:?}", other);
                        Err(message)
                    }
                    Err(e) => Err(e.to_string()),
                }
            }
            ActiveEngineSelection::Soniox { .. } => {
                match soniox_transcribe_async(
                    &app_for_task,
                    &audio_path_clone,
                    language_for_task.as_deref(),
                )
                .await
                {
                    Ok(text) => Ok(text),
                    Err(e) => Err(e),
                }
            }
        };

        // Clean up temp file regardless of outcome
        if let Err(e) = std::fs::remove_file(&audio_path_clone) {
            log::warn!("Failed to remove temporary audio file: {}", e);
        }

        match transcription_result {
            Ok(text) => {
                // Final cancellation check before processing result
                if app_state.is_cancellation_requested() {
                    log::info!("Transcription completed but was cancelled, discarding result");

                    // Hide pill window since we're cancelling
                    if let Err(e) =
                        crate::commands::window::hide_pill_widget(app_for_task.clone()).await
                    {
                        log::error!("Failed to hide pill window on cancellation: {}", e);
                    }

                    update_recording_state(&app_for_task, RecordingState::Idle, None);
                    return;
                }

                log::debug!("Transcription successful, {} chars", text.len());

                // Check if transcription is empty or just noise
                if text.is_empty() || text.trim().is_empty() || text == "[BLANK_AUDIO]" {
                    log::info!("Whisper returned empty transcription - no speech detected");

                    // Emit graceful feedback to user
                    let _ = emit_to_window(
                        &app_for_task,
                        "pill",
                        "transcription-empty",
                        "No speech detected - try speaking closer to the microphone",
                    );

                    // Wait for feedback to show before hiding pill
                    let app_for_hide = app_for_task.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

                        // Hide pill window
                        if let Err(e) =
                            crate::commands::window::hide_pill_widget(app_for_hide.clone()).await
                        {
                            log::error!("Failed to hide pill window: {}", e);
                        }

                        // Transition back to Idle
                        update_recording_state(&app_for_hide, RecordingState::Idle, None);
                    });

                    return;
                }

                // Check if AI enhancement is enabled from cached config
                let ai_enabled = config.ai_enabled;

                // If AI is enabled, emit enhancing event NOW while pill is still visible
                if ai_enabled {
                    let _ = emit_to_window(&app_for_task, "pill", "enhancing-started", ());
                }

                // Backend handles the complete flow
                let app_for_process = app_for_task.clone();
                let text_for_process = text.clone();
                let model_for_process = selected_model_name_for_task.clone();
                let ai_enabled_for_task = ai_enabled; // Capture from cached config

                tokio::spawn(async move {
                    // 1. Process the transcription and enhancement
                    let final_text = {
                        // Use the captured AI enabled status from cached config
                        if ai_enabled_for_task {
                            match crate::commands::ai::enhance_transcription(
                                text_for_process.clone(),
                                app_for_process.clone(),
                            )
                            .await
                            {
                                Ok(enhanced) => {
                                    // Emit enhancing completed event
                                    let _ = emit_to_window(
                                        &app_for_process,
                                        "pill",
                                        "enhancing-completed",
                                        (),
                                    );

                                    if enhanced != text_for_process {
                                        log::info!("AI enhancement applied successfully");
                                    }
                                    enhanced
                                }
                                Err(e) => {
                                    log::warn!("AI enhancement failed, using original text: {}", e);

                                    // Check error type and create appropriate message
                                    let error_message = e.to_string();
                                    let user_message = if error_message.contains("400")
                                        || error_message.contains("Bad Request")
                                    {
                                        "Enhancement failed: Missing or invalid API key"
                                    } else if error_message.contains("401")
                                        || error_message.contains("Unauthorized")
                                    {
                                        "Enhancement failed: Invalid API key"
                                    } else if error_message.contains("429") {
                                        "Enhancement failed: Rate limit exceeded"
                                    } else if error_message.contains("network")
                                        || error_message.contains("connection")
                                    {
                                        "Enhancement failed: Network error"
                                    } else {
                                        "Enhancement failed: Using original text"
                                    };

                                    // Show short error on pill for visibility, then continue
                                    log::warn!(
                                        "Enhancement failed; showing pill error for 2s before hide"
                                    );
                                    crate::show_pill_error_short(
                                        &app_for_process,
                                        "enhancing-failed",
                                        user_message,
                                        2000,
                                    )
                                    .await;

                                    // Also notify main window for settings update if needed
                                    if error_message.contains("400")
                                        || error_message.contains("401")
                                        || error_message.contains("Bad Request")
                                        || error_message.contains("Unauthorized")
                                    {
                                        let _ = emit_to_window(
                                            &app_for_process,
                                            "main",
                                            "ai-enhancement-auth-error",
                                            "Please check your AI API key in settings.",
                                        );
                                    }

                                    text_for_process.clone() // Fall back to original text
                                }
                            }
                        } else {
                            log::debug!("AI enhancement is disabled, using original text");
                            text_for_process.clone()
                        }
                    };

                    // 2. Hide pill window first, then insert text with reduced delay
                    let app_state = app_for_process.state::<AppState>();

                    // Hide pill window first to avoid UI race conditions
                    if let Some(window_manager) = app_state.get_window_manager() {
                        if let Err(e) = window_manager.hide_pill_window().await {
                            log::error!("Failed to hide pill window: {}", e);
                        }
                    } else {
                        log::error!("WindowManager not initialized");
                    }

                    // Reduced delay to ensure UI is stable (was 100ms, now 50ms)
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                    // Now handle text insertion with stable UI
                    match crate::commands::text::insert_text(
                        app_for_process.clone(),
                        final_text.clone(),
                    )
                    .await
                    {
                        Ok(_) => log::debug!("Text inserted at cursor successfully"),
                        Err(e) => {
                            log::error!("Failed to insert text: {}", e);

                            // Check if it's an accessibility permission issue
                            if e.contains("accessibility") || e.contains("permission") {
                                // Show the pill window again to notify user
                                if let Some(window_manager) = app_state.get_window_manager() {
                                    let _ = window_manager.show_pill_window().await;
                                }

                                // Emit error to pill widget
                                let _ = emit_to_window(
                                    &app_for_process,
                                    "pill",
                                    "paste-error",
                                    "Text copied - grant permission to auto-paste",
                                );

                                // Keep pill visible for 3 seconds with error
                                tokio::time::sleep(std::time::Duration::from_secs(3)).await;

                                // Then hide it
                                if let Some(window_manager) = app_state.get_window_manager() {
                                    let _ = window_manager.hide_pill_window().await;
                                }
                            } else {
                                // Generic paste error
                                let _ = emit_to_window(
                                    &app_for_process,
                                    "main",
                                    "paste-error",
                                    format!("Paste failed - text in clipboard"),
                                );
                            }
                        }
                    }

                    // 5. Save transcription to history (async, non-blocking)
                    let app_for_history = app_for_process.clone();
                    let history_text = final_text.clone();
                    let history_model = model_for_process.clone();
                    tokio::spawn(async move {
                        match save_transcription(
                            app_for_history.clone(),
                            history_text,
                            history_model,
                        )
                        .await
                        {
                            Ok(_) => {
                                // Emit history-updated event to refresh UI
                                let _ =
                                    emit_to_window(&app_for_history, "main", "history-updated", ());
                                log::debug!("Transcription saved to history successfully");
                            }
                            Err(e) => log::error!("Failed to save transcription to history: {}", e),
                        }
                    });

                    // 6. Transition to idle state
                    update_recording_state(&app_for_process, RecordingState::Idle, None);
                });
            }
            Err(e) => {
                // Check if this is a cancellation error
                if e.contains("cancelled") {
                    log::info!("Handling transcription cancellation");
                    // For cancellation, hide pill immediately and go to Idle
                    if let Err(hide_err) =
                        crate::commands::window::hide_pill_widget(app_for_task.clone()).await
                    {
                        log::error!("Failed to hide pill window on cancellation: {}", hide_err);
                    }
                    update_recording_state(&app_for_task, RecordingState::Idle, None);
                } else if e.contains("too short") {
                    // Handle "too short" errors with specific user feedback
                    log::info!("Recording was too short: {}", e);

                    // Clean up the audio file
                    if let Err(cleanup_err) = std::fs::remove_file(&audio_path_clone) {
                        log::warn!("Failed to remove short audio file: {}", cleanup_err);
                    }

                    // Emit specific feedback to pill window
                    let _ = emit_to_window(&app_for_task, "pill", "transcription-empty", &e);

                    // Hide pill after showing feedback
                    let app_for_reset = app_for_task.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

                        if let Err(e) =
                            crate::commands::window::hide_pill_widget(app_for_reset.clone()).await
                        {
                            log::error!("Failed to hide pill window: {}", e);
                        }

                        update_recording_state(&app_for_reset, RecordingState::Idle, None);
                    });
                } else {
                    // For other errors, show error state briefly
                    update_recording_state(&app_for_task, RecordingState::Error, Some(e.clone()));

                    // Also emit legacy event to pill window
                    let _ = emit_to_window(&app_for_task, "pill", "transcription-error", e);

                    // Transition back to Idle after a delay
                    // This ensures we don't get stuck in Error state
                    let app_for_reset = app_for_task.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        log::debug!(
                            "Resetting from Error to Idle state after transcription failure"
                        );

                        // Hide pill window when transitioning to Idle
                        if let Err(e) =
                            crate::commands::window::hide_pill_widget(app_for_reset.clone()).await
                        {
                            log::error!("Failed to hide pill window: {}", e);
                        }

                        update_recording_state(&app_for_reset, RecordingState::Idle, None);
                    });
                }
            }
        }
    });

    // Track the transcription task
    let app_state = app.state::<AppState>();
    if let Ok(mut task_guard) = app_state.transcription_task.lock() {
        // Cancel any existing task
        if let Some(existing_task) = task_guard.take() {
            existing_task.abort();
        }
        // Store the new task handle
        *task_guard = Some(task_handle);
    }

    // Return immediately so front-end promise resolves before timeout
    Ok(String::new())
}

#[tauri::command]
pub async fn get_audio_devices() -> Result<Vec<String>, String> {
    Ok(AudioRecorder::get_devices())
}

/// Get the current default audio input device
#[tauri::command]
pub async fn get_current_audio_device() -> Result<String, String> {
    let host = cpal::default_host();

    host.default_input_device()
        .and_then(|device| device.name().ok())
        .ok_or_else(|| "No default input device found".to_string())
}

#[tauri::command]
pub async fn cleanup_old_transcriptions(app: AppHandle, days: Option<u32>) -> Result<(), String> {
    if let Some(days) = days {
        let store = app.store("transcriptions").map_err(|e| e.to_string())?;

        let cutoff_date = chrono::Utc::now() - chrono::Duration::days(days as i64);

        // Get all keys
        let keys: Vec<String> = store.keys().into_iter().map(|k| k.to_string()).collect();

        // Remove old entries
        for key in keys {
            if let Ok(date) = chrono::DateTime::parse_from_rfc3339(&key) {
                if date < cutoff_date {
                    store.delete(&key);
                }
            }
        }

        store.save().map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn save_transcription(app: AppHandle, text: String, model: String) -> Result<(), String> {
    // De-dup guard: skip saving if the most recent entry matches the same text & model within a short window
    if let Ok(store) = app.store("transcriptions") {
        // Find most recent entry
        let mut latest: Option<(String, serde_json::Value)> = None;
        for key in store.keys() {
            if let Some(value) = store.get(&key) {
                match &latest {
                    Some((ts, _)) => {
                        if key > *ts {
                            latest = Some((key.to_string(), value));
                        }
                    }
                    None => latest = Some((key.to_string(), value)),
                }
            }
        }

        if let Some((ts, v)) = latest {
            let same_text = v
                .get("text")
                .and_then(|x| x.as_str())
                .map(|s| s == text)
                .unwrap_or(false);
            let same_model = v
                .get("model")
                .and_then(|x| x.as_str())
                .map(|s| s == model)
                .unwrap_or(false);
            let within_window = chrono::DateTime::parse_from_rfc3339(&ts)
                .ok()
                .and_then(|t| {
                    t.with_timezone(&chrono::Utc)
                        .signed_duration_since(chrono::Utc::now())
                        .num_seconds()
                        .checked_abs()
                })
                .map(|secs| secs <= 2)
                .unwrap_or(false);
            if same_text && same_model && within_window {
                log::info!("Skipping duplicate transcription save (same text/model within 2s)");
                return Ok(());
            }
        }
    }

    // Save transcription to store with current timestamp
    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;

    let timestamp = chrono::Utc::now().to_rfc3339();
    let transcription_data = serde_json::json!({
        "text": text.clone(),
        "model": model,
        "timestamp": timestamp.clone()
    });

    store.set(&timestamp, transcription_data.clone());

    store
        .save()
        .map_err(|e| format!("Failed to save transcription: {}", e))?;

    // Emit the new transcription data to frontend for append-only update
    let _ = emit_to_window(&app, "main", "transcription-added", transcription_data);

    // Refresh tray menu (best-effort) so Recent Transcriptions stays updated
    if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
        log::warn!(
            "Failed to update tray menu after saving transcription: {}",
            e
        );
    }

    log::info!("Saved transcription with {} characters", text.len());
    Ok(())
}

#[tauri::command]
pub async fn get_transcription_history(
    app: AppHandle,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    let store = app.store("transcriptions").map_err(|e| e.to_string())?;

    let mut entries: Vec<(String, serde_json::Value)> = Vec::new();

    // Collect all entries with their timestamps
    for key in store.keys() {
        if let Some(value) = store.get(&key) {
            entries.push((key.to_string(), value));
        }
    }

    // Sort by timestamp (newest first)
    entries.sort_by(|a, b| b.0.cmp(&a.0));

    // Apply limit if specified
    let limit = limit.unwrap_or(50);
    entries.truncate(limit);

    // Return just the values
    Ok(entries.into_iter().map(|(_, v)| v).collect())
}

#[tauri::command]
pub async fn transcribe_audio_file(
    app: AppHandle,
    file_path: String,
    model_name: String,
    model_engine: Option<String>,
) -> Result<String, String> {
    log::info!(
        "[UPLOAD] transcribe_audio_file START | file_path={:?}, model_name={}, engine_hint={:?}",
        file_path,
        model_name,
        model_engine
    );
    // Validate requirements (includes license check)
    validate_recording_requirements(&app).await?;

    // Use the provided file path directly
    let audio_path = std::path::Path::new(&file_path);

    // Validate file exists
    if !audio_path.exists() {
        return Err(format!("Audio file not found: {}", file_path));
    }

    // Convert to WAV if needed
    let recordings_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("recordings");

    std::fs::create_dir_all(&recordings_dir)
        .map_err(|e| format!("Failed to create recordings directory: {}", e))?;

    // No pre-conversion needed; ffmpeg normalizer can read most formats directly.
    let wav_path = audio_path.to_path_buf();
    log::info!("[UPLOAD] Input ready at {:?}", wav_path);

    // Resolve engine (whisper/parakeet/soniox) for the requested model
    let engine_selection =
        resolve_engine_for_model(&app, &model_name, model_engine.as_deref()).await?;
    log::info!(
        "[UPLOAD] Engine resolved to: {}",
        engine_selection.engine_name()
    );

    // Get language and translation settings
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let language = {
        let lang = store
            .get("language")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "en".to_string());

        validate_language(Some(&lang)).to_string()
    };

    let translate_to_english = store
        .get("translate_to_english")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    log::info!(
        "[LANGUAGE] transcribe_audio_file using language: {}, translate: {}",
        language,
        translate_to_english
    );

    // For Soniox, skip normalization and send original wav_path
    let text = match engine_selection {
        ActiveEngineSelection::Whisper { model_path, .. } => {
            // Normalize to Whisper contract
            log::debug!("[UPLOAD] Normalizing to Whisper WAV (16k mono s16)...");
            let normalized_path = {
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let out_path = recordings_dir.join(format!("normalized_{}.wav", ts));
                crate::ffmpeg::normalize_streaming(&app, &wav_path, &out_path)
                    .await
                    .map_err(|e| format!("Audio normalization (ffmpeg) failed: {}", e))?;
                out_path
            };
            log::info!("[UPLOAD] Normalized WAV at {:?}", normalized_path);
            let transcriber = {
                let cache_state = app.state::<AsyncMutex<TranscriberCache>>();
                let mut cache = cache_state.lock().await;
                cache.get_or_create(&model_path)?
            };

            let result = transcriber.transcribe_with_translation(
                &normalized_path,
                Some(&language),
                translate_to_english,
            )?;
            let _ = std::fs::remove_file(&normalized_path);
            result
        }
        ActiveEngineSelection::Parakeet { model_name } => {
            // Normalize to Whisper/Parakeet contract first
            log::debug!("[UPLOAD] Normalizing to Whisper WAV (16k mono s16)...");
            let normalized_path = {
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let out_path = recordings_dir.join(format!("normalized_{}.wav", ts));
                crate::ffmpeg::normalize_streaming(&app, &wav_path, &out_path)
                    .await
                    .map_err(|e| format!("Audio normalization (ffmpeg) failed: {}", e))?;
                out_path
            };
            log::info!("[UPLOAD] Normalized WAV at {:?}", normalized_path);
            let parakeet_manager = app.state::<ParakeetManager>();

            parakeet_manager
                .load_model(&app, &model_name)
                .await
                .map_err(|e| format!("Failed to load Parakeet model: {}", e))?;

            match parakeet_manager
                .transcribe(
                    &app,
                    &model_name,
                    normalized_path.clone(),
                    Some(language.clone()),
                    translate_to_english,
                )
                .await
            {
                Ok(ParakeetResponse::Transcription { text, .. }) => {
                    let _ = std::fs::remove_file(&normalized_path);
                    text
                }
                Ok(other) => {
                    return Err(format!("Unexpected Parakeet response: {:?}", other));
                }
                Err(err) => {
                    return Err(format!("Parakeet transcription failed: {}", err));
                }
            }
        }
        ActiveEngineSelection::Soniox { .. } => {
            soniox_transcribe_async(&app, &wav_path, Some(&language)).await?
        }
    };

    log::info!(
        "[UPLOAD] Completed transcription, {} characters",
        text.len()
    );
    Ok(text)
}

#[tauri::command]
pub async fn transcribe_audio(
    app: AppHandle,
    audio_data: Vec<u8>,
    model_name: String,
    model_engine: Option<String>,
) -> Result<String, String> {
    log::info!(
        "[UPLOAD] transcribe_audio (bytes) START | bytes={}, model_name={}, engine_hint={:?}",
        audio_data.len(),
        model_name,
        model_engine
    );
    // Validate requirements (includes license check)
    validate_recording_requirements(&app).await?;

    // Save audio data to app data directory
    let recordings_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("recordings");

    // Ensure directory exists
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|e| format!("Failed to create recordings directory: {}", e))?;

    let temp_path = recordings_dir.join("temp_audio.wav");

    std::fs::write(&temp_path, audio_data).map_err(|e| e.to_string())?;

    let engine_selection =
        resolve_engine_for_model(&app, &model_name, model_engine.as_deref()).await?;

    // Get language and translation settings
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let language = {
        let lang = store
            .get("language")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "en".to_string());

        validate_language(Some(&lang)).to_string()
    };

    let translate_to_english = store
        .get("translate_to_english")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    log::info!(
        "[LANGUAGE] transcribe_audio using language: {}, translate: {}",
        language,
        translate_to_english
    );

    let text = match engine_selection {
        ActiveEngineSelection::Whisper { model_path, .. } => {
            let transcriber = {
                let cache_state = app.state::<AsyncMutex<TranscriberCache>>();
                let mut cache = cache_state.lock().await;
                cache.get_or_create(&model_path)?
            };

            transcriber.transcribe_with_translation(
                &temp_path,
                Some(language.as_str()),
                translate_to_english,
            )?
        }
        ActiveEngineSelection::Parakeet { model_name } => {
            let parakeet_manager = app.state::<ParakeetManager>();

            parakeet_manager
                .load_model(&app, &model_name)
                .await
                .map_err(|e| format!("Failed to load Parakeet model: {}", e))?;

            match parakeet_manager
                .transcribe(
                    &app,
                    &model_name,
                    temp_path.clone(),
                    Some(language.clone()),
                    translate_to_english,
                )
                .await
            {
                Ok(ParakeetResponse::Transcription { text, .. }) => text,
                Ok(other) => return Err(format!("Unexpected Parakeet response: {:?}", other)),
                Err(err) => return Err(format!("Parakeet transcription failed: {}", err)),
            }
        }
        ActiveEngineSelection::Soniox { .. } => {
            soniox_transcribe_async(&app, &temp_path, Some(&language)).await?
        }
    };

    // Clean up
    if let Err(e) = std::fs::remove_file(&temp_path) {
        log::warn!("Failed to remove test audio file: {}", e);
    }

    Ok(text)
}

// Soniox async transcription via v1 Files + Transcriptions flow
async fn soniox_transcribe_async(
    app: &AppHandle,
    wav_path: &Path,
    language: Option<&str>,
) -> Result<String, String> {
    use reqwest::multipart::{Form, Part};
    use tokio::fs;

    let key = crate::secure_store::secure_get(app, "stt_api_key_soniox")?
        .ok_or_else(|| "Soniox API key not set".to_string())?;

    let wav_bytes = fs::read(wav_path)
        .await
        .map_err(|e| format!("Failed to read audio file: {}", e))?;

    let client = reqwest::Client::new();
    let base = "https://api.soniox.com/v1";

    // 1) Upload file -> file_id
    let filename = wav_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.wav");
    let file_part = Part::bytes(wav_bytes)
        .file_name(filename.to_string())
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;
    let form = Form::new().part("file", file_part);

    let upload_url = format!("{}/files", base);
    let upload_resp = client
        .post(&upload_url)
        .bearer_auth(&key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Network error (upload): {}", e))?;
    if !upload_resp.status().is_success() {
        let code = upload_resp.status();
        let body = upload_resp.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(300).collect();
        return Err(format!("Soniox upload failed: HTTP {}: {}", code, snippet));
    }
    let upload_json: serde_json::Value = upload_resp.json().await.map_err(|e| e.to_string())?;
    let file_id = upload_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing file_id")?
        .to_string();

    // 2) Create transcription -> transcription_id
    let mut payload = serde_json::json!({
        "model": "stt-async-preview",
        "file_id": file_id,
    });
    if let Some(lang) = language {
        payload["language_hints"] = serde_json::json!([lang]);
    }

    let create_url = format!("{}/transcriptions", base);
    let create_resp = client
        .post(&create_url)
        .bearer_auth(&key)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Network error (create): {}", e))?;
    if !create_resp.status().is_success() {
        let code = create_resp.status();
        let body = create_resp.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(300).collect();
        return Err(format!(
            "Soniox create transcription failed: HTTP {}: {}",
            code, snippet
        ));
    }
    let create_json: serde_json::Value = create_resp.json().await.map_err(|e| e.to_string())?;
    let transcription_id = create_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing transcription id")?
        .to_string();

    // 3) Poll status
    let status_url = format!("{}/transcriptions/{}", base, transcription_id);
    let started = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(180);
    loop {
        let resp = client
            .get(&status_url)
            .bearer_auth(&key)
            .send()
            .await
            .map_err(|e| format!("Network error (status): {}", e))?;
        if !resp.status().is_success() {
            let code = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let snippet: String = body.chars().take(200).collect();
            return Err(format!("Soniox status failed: HTTP {}: {}", code, snippet));
        }
        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let status = json.get("status").and_then(|v| v.as_str()).unwrap_or("");
        match status {
            "completed" => break,
            "error" => {
                let msg = json
                    .get("error_message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Job failed");
                return Err(format!("Soniox job failed: {}", msg));
            }
            _ => {
                if started.elapsed() > timeout {
                    return Err("Soniox transcription timed out".to_string());
                }
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            }
        }
    }

    // 4) Fetch transcript
    let transcript_url = format!("{}/transcriptions/{}/transcript", base, transcription_id);
    let resp = client
        .get(&transcript_url)
        .bearer_auth(&key)
        .send()
        .await
        .map_err(|e| format!("Network error (transcript): {}", e))?;
    if !resp.status().is_success() {
        let code = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(200).collect();
        return Err(format!(
            "Soniox transcript failed: HTTP {}: {}",
            code, snippet
        ));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    // Prefer direct text if present, else join tokens
    if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
        return Ok(text.to_string());
    }
    if let Some(tokens) = json.get("tokens").and_then(|v| v.as_array()) {
        let mut out = String::new();
        let mut first = true;
        for t in tokens {
            if let Some(txt) = t.get("text").and_then(|v| v.as_str()) {
                if !first {
                    out.push(' ');
                } else {
                    first = false;
                }
                out.push_str(txt);
            }
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }
    Err("Soniox transcript format not recognized".to_string())
}

#[tauri::command]
pub async fn cancel_recording(app: AppHandle) -> Result<(), String> {
    log::info!("=== CANCEL RECORDING CALLED ===");

    // Request cancellation FIRST
    let app_state = app.state::<AppState>();
    app_state.request_cancellation();
    log::info!("Cancellation requested in app state");

    // Get current state
    let current_state = app_state.get_current_state();
    log::info!("Current state when cancelling: {:?}", current_state);

    // Abort any ongoing transcription task
    if let Ok(mut task_guard) = app_state.transcription_task.lock() {
        if let Some(task) = task_guard.take() {
            log::info!("Aborting transcription task");
            task.abort();
        }
    }

    // Stop recording if active
    let recorder_state = app.state::<RecorderState>();
    let is_recording = {
        let guard = recorder_state
            .inner()
            .0
            .lock()
            .map_err(|e| format!("Failed to acquire recorder lock: {}", e))?;
        guard.is_recording()
    };

    if is_recording {
        log::info!("Stopping recorder");
        // Just stop the recorder, don't do full stop_recording flow
        {
            let mut recorder = recorder_state
                .inner()
                .0
                .lock()
                .map_err(|e| format!("Failed to acquire recorder lock: {}", e))?;
            let _ = recorder.stop_recording()?;
        }

        // Clean up audio file if it exists
        if let Ok(path_guard) = app_state.current_recording_path.lock() {
            if let Some(audio_path) = path_guard.as_ref() {
                log::info!("Removing cancelled recording file");
                if let Err(e) = std::fs::remove_file(audio_path) {
                    log::warn!("Failed to remove cancelled recording: {}", e);
                }
            }
        }
    }

    // Unregister ESC key
    match "Escape".parse::<tauri_plugin_global_shortcut::Shortcut>() {
        Ok(escape_shortcut) => {
            if let Err(e) = app.global_shortcut().unregister(escape_shortcut) {
                log::debug!("Failed to unregister ESC shortcut: {}", e);
            }
        }
        Err(e) => {
            log::debug!("Failed to parse ESC shortcut: {:?}", e);
        }
    }

    // Clean up ESC state
    app_state
        .esc_pressed_once
        .store(false, std::sync::atomic::Ordering::SeqCst);
    if let Ok(mut timeout_guard) = app_state.esc_timeout_handle.lock() {
        if let Some(handle) = timeout_guard.take() {
            handle.abort();
        }
    }

    // Hide pill window immediately
    if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
        log::error!("Failed to hide pill window: {}", e);
    }

    // Properly transition through states based on current state
    match current_state {
        RecordingState::Recording => {
            // First transition to Stopping
            update_recording_state(&app, RecordingState::Stopping, None);
            // Then transition to Idle
            update_recording_state(&app, RecordingState::Idle, None);
        }
        RecordingState::Starting => {
            // Starting can go directly to Idle
            update_recording_state(&app, RecordingState::Idle, None);
        }
        RecordingState::Stopping => {
            // Already stopping, just go to Idle
            update_recording_state(&app, RecordingState::Idle, None);
        }
        RecordingState::Transcribing => {
            // Can't go directly to Idle from Transcribing, need to go through Error
            update_recording_state(
                &app,
                RecordingState::Error,
                Some("Transcription cancelled".to_string()),
            );
            update_recording_state(&app, RecordingState::Idle, None);
        }
        _ => {
            // For other states (Idle, Error), try to transition to Idle
            update_recording_state(&app, RecordingState::Idle, None);
        }
    }

    log::info!("=== CANCEL RECORDING COMPLETED ===");
    Ok(())
}

#[tauri::command]
pub async fn delete_transcription_entry(app: AppHandle, timestamp: String) -> Result<(), String> {
    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;

    // Delete the entry
    store.delete(&timestamp);

    // Save the store
    store
        .save()
        .map_err(|e| format!("Failed to save store after deletion: {}", e))?;

    // Emit event to update UI
    let _ = emit_to_window(&app, "main", "history-updated", ());

    // Refresh tray menu to reflect removal
    if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
        log::warn!("Failed to update tray menu after deletion: {}", e);
    }

    log::info!("Deleted transcription entry: {}", timestamp);
    Ok(())
}

#[tauri::command]
pub async fn clear_all_transcriptions(app: AppHandle) -> Result<(), String> {
    log::info!("[Clear All] Clearing all transcriptions");

    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;

    // Get all keys and delete them
    let keys: Vec<String> = store.keys().into_iter().map(|k| k.to_string()).collect();
    let count = keys.len();

    for key in keys {
        store.delete(&key);
    }

    // Save the store
    store
        .save()
        .map_err(|e| format!("Failed to save store after clearing: {}", e))?;

    // Emit event to update UI
    let _ = emit_to_window(&app, "main", "history-updated", ());

    // Refresh tray menu after clearing
    if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
        log::warn!("Failed to update tray menu after clearing history: {}", e);
    }

    log::info!("Cleared all transcription entries: {} items", count);
    Ok(())
}

#[derive(serde::Serialize)]
pub struct RecordingStateResponse {
    state: String,
    error: Option<String>,
}

#[tauri::command]
pub fn get_current_recording_state(app: AppHandle) -> RecordingStateResponse {
    let app_state = app.state::<AppState>();
    let current_state = app_state.get_current_state();

    RecordingStateResponse {
        state: match current_state {
            RecordingState::Idle => "idle",
            RecordingState::Starting => "starting",
            RecordingState::Recording => "recording",
            RecordingState::Stopping => "stopping",
            RecordingState::Transcribing => "transcribing",
            RecordingState::Error => "error",
        }
        .to_string(),
        error: None,
    }
}
