use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::audio::recorder::AudioRecorder;
use crate::commands::license::check_license_status_internal;
use crate::commands::settings::{get_settings, resolve_pill_indicator_mode, Settings};
use crate::license::LicenseState;
use crate::parakeet::messages::ParakeetResponse;
use crate::parakeet::ParakeetManager;
use crate::utils::logger::*;
#[cfg(debug_assertions)]
use crate::utils::system_monitor;
use crate::remote::client::RemoteServerConnection;
use crate::remote::settings::RemoteSettings;
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

/// Atomic counter for toast IDs to prevent race conditions
static TOAST_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Payload for pill toast messages
#[derive(serde::Serialize, Clone)]
pub struct PillToastPayload {
    pub id: u64,
    pub message: String,
    pub duration_ms: u64,
}

/// Show a toast message on the pill's toast window (above the pill)
/// This is the single unified API for pill feedback messages.
/// Uses atomic counter to prevent race conditions with overlapping toasts.
pub fn pill_toast(app: &AppHandle, message: &str, duration_ms: u64) {
    let id = TOAST_ID_COUNTER.fetch_add(1, AtomicOrdering::SeqCst).wrapping_add(1);

    // Show toast window
    if let Some(toast_window) = app.get_webview_window("toast") {
        let _ = toast_window.show();

        // Backend controls hide timing - only hide if this is still the latest toast
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(duration_ms)).await;
            // Only hide if we're still the latest toast
            if TOAST_ID_COUNTER.load(AtomicOrdering::SeqCst) == id {
                if let Some(tw) = app_clone.get_webview_window("toast") {
                    let _ = tw.hide();
                }
            }
        });
    } else {
        log::warn!("pill_toast: toast window not found, message not shown: {}", message);
    }

    // Build and emit unified payload
    let payload = PillToastPayload {
        id,
        message: message.to_string(),
        duration_ms,
    };

    let _ = app.emit("toast", payload);
}

fn should_hide_pill_when_idle(mode: &str) -> bool {
    mode != "always"
}

/// Check if pill should be hidden based on pill_indicator_mode setting.
/// Returns true if pill should be hidden, false if it should stay visible.
/// Called when transitioning to idle state (after recording ends).
/// - "never" → always hide (return true)
/// - "always" → never hide (return false)
/// - "when_recording" → hide when idle (return true)
/// Fails open: on error, returns true (default to when_recording behavior).
#[track_caller]
pub async fn should_hide_pill(app: &AppHandle) -> bool {
    let store = match app.store("settings") {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Failed to load settings for pill visibility: {}", e);
            return true; // Default to when_recording behavior (hide when idle)
        }
    };

    let stored_mode = store
        .get("pill_indicator_mode")
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let legacy_show = store.get("show_pill_indicator").and_then(|v| v.as_bool());
    let pill_indicator_mode = resolve_pill_indicator_mode(
        stored_mode.clone(),
        legacy_show,
        Settings::default().pill_indicator_mode,
    );
    let caller = std::panic::Location::caller();
    log::debug!(
        "pill_visibility: should_hide_pill caller={} stored={:?} legacy_show={:?} resolved='{}'",
        caller,
        stored_mode,
        legacy_show,
        pill_indicator_mode
    );

    let result = should_hide_pill_when_idle(&pill_indicator_mode);
    log::debug!(
        "should_hide_pill: pill_indicator_mode='{}', should_hide={}",
        pill_indicator_mode,
        result
    );

    result
}

#[cfg(test)]
mod tests {
    use super::should_hide_pill_when_idle;

    #[test]
    fn should_hide_pill_when_idle_for_never() {
        assert!(should_hide_pill_when_idle("never"));
    }

    #[test]
    fn should_hide_pill_when_idle_for_when_recording() {
        assert!(should_hide_pill_when_idle("when_recording"));
    }

    #[test]
    fn should_hide_pill_when_idle_for_always() {
        assert!(!should_hide_pill_when_idle("always"));
    }
}

/// Play a system sound to confirm recording start (macOS only)
#[cfg(target_os = "macos")]
fn play_recording_start_sound() {
    std::thread::spawn(|| {
        let _ = std::process::Command::new("afplay")
            .arg("/System/Library/Sounds/Tink.aiff")
            .spawn();
    });
}

/// Play a system sound to confirm recording start (Windows)
#[cfg(target_os = "windows")]
fn play_recording_start_sound() {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    std::thread::spawn(|| {
        // Use PowerShell to play a system sound on Windows (hidden console)
        let _ = std::process::Command::new("powershell")
            .args(["-c", "[console]::beep(800, 100)"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    });
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn play_recording_start_sound() {
    // No-op on other platforms
}

/// Play a system sound to confirm recording end (macOS only)
#[cfg(target_os = "macos")]
fn play_recording_end_sound() {
    std::thread::spawn(|| {
        // Use a different sound for recording end - Pop sound
        let _ = std::process::Command::new("afplay")
            .arg("/System/Library/Sounds/Pop.aiff")
            .spawn();
    });
}

/// Play a system sound to confirm recording end (Windows)
#[cfg(target_os = "windows")]
fn play_recording_end_sound() {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    std::thread::spawn(|| {
        // Use PowerShell with a lower frequency tone for recording end (hidden console)
        let _ = std::process::Command::new("powershell")
            .args(["-c", "[console]::beep(600, 100)"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    });
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn play_recording_end_sound() {
    // No-op on other platforms
}

/// Cached recording configuration to avoid repeated store access during transcription flow
/// Cache is invalidated when settings change via update hooks
#[derive(Clone, Debug)]
pub struct RecordingConfig {
    pub show_pill_widget: bool,
    pub pill_indicator_mode: String, // "never", "always", or "when_recording"
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

        let show_pill_widget = store
            .get("show_pill_widget")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let stored_mode = store
            .get("pill_indicator_mode")
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let legacy_show = store.get("show_pill_indicator").and_then(|v| v.as_bool());
        let pill_indicator_mode = resolve_pill_indicator_mode(
            stored_mode.clone(),
            legacy_show,
            Settings::default().pill_indicator_mode,
        );
        log::debug!(
            "pill_visibility: recording config loaded show_pill_widget={} pill_indicator_mode='{}' stored={:?} legacy_show={:?}",
            show_pill_widget,
            pill_indicator_mode,
            stored_mode,
            legacy_show
        );

        Ok(Self {
            show_pill_widget,
            pill_indicator_mode,
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

/// Always save a recording file to persistent storage
/// This is used for the "preserve recordings on failure" feature
/// Returns the saved filename (not full path) if saved, None otherwise
pub async fn save_recording(app: &AppHandle, audio_path: &Path) -> Option<String> {
    save_recording_internal(app, audio_path, false).await
}

/// Save a recording file to persistent storage if save_recordings is enabled
/// Returns the saved filename (not full path) if saved, None otherwise
pub async fn maybe_save_recording(app: &AppHandle, audio_path: &Path) -> Option<String> {
    save_recording_internal(app, audio_path, true).await
}

/// Internal function to save recording with optional settings check
async fn save_recording_internal(app: &AppHandle, audio_path: &Path, check_settings: bool) -> Option<String> {
    // Get settings store for retention count (always needed) and save_recordings check
    let store = match app.store("settings") {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Failed to get settings store: {}", e);
            // If we can't get settings, still save for preservation purposes
            if check_settings {
                return None;
            }
            // For forced saves (preserve on failure), continue without store
            // We'll skip retention cleanup in this case
            return save_recording_without_cleanup(app, audio_path).await;
        }
    };

    if check_settings {
        let save_recordings = store
            .get("save_recordings")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !save_recordings {
            log::debug!("save_recordings is disabled, skipping recording persistence");
            return None;
        }
    }

    // Get recordings directory
    let recordings_dir = match app.path().app_data_dir() {
        Ok(dir) => dir.join("recordings"),
        Err(e) => {
            log::error!("Failed to get app data directory: {}", e);
            return None;
        }
    };

    // Create recordings directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(&recordings_dir) {
        log::error!("Failed to create recordings directory: {}", e);
        return None;
    }

    // Generate filename: timestamp_uuid.wav
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let uuid_part = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let filename = format!("{}_{}.wav", timestamp, uuid_part);
    let dest_path = recordings_dir.join(&filename);

    // Copy the file to persistent storage
    match std::fs::copy(audio_path, &dest_path) {
        Ok(_) => {
            log::info!("Saved recording to: {:?}", dest_path);

            // Cleanup old recordings
            let retention_count = store
                .get("recording_retention_count")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);

            if let Some(max_count) = retention_count {
                cleanup_old_recordings(&recordings_dir, max_count);
            }

            Some(filename)
        }
        Err(e) => {
            log::error!("Failed to save recording: {}", e);
            None
        }
    }
}

/// Save recording without cleanup (fallback when store is unavailable)
async fn save_recording_without_cleanup(app: &AppHandle, audio_path: &Path) -> Option<String> {
    let recordings_dir = match app.path().app_data_dir() {
        Ok(dir) => dir.join("recordings"),
        Err(e) => {
            log::error!("Failed to get app data directory: {}", e);
            return None;
        }
    };

    if let Err(e) = std::fs::create_dir_all(&recordings_dir) {
        log::error!("Failed to create recordings directory: {}", e);
        return None;
    }

    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let uuid_part = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let filename = format!("{}_{}.wav", timestamp, uuid_part);
    let dest_path = recordings_dir.join(&filename);

    match std::fs::copy(audio_path, &dest_path) {
        Ok(_) => {
            log::info!("Saved recording (no cleanup) to: {:?}", dest_path);
            Some(filename)
        }
        Err(e) => {
            log::error!("Failed to save recording: {}", e);
            None
        }
    }
}

/// Clean up old recordings when count exceeds retention limit
fn cleanup_old_recordings(recordings_dir: &Path, max_count: usize) {
    if max_count == 0 {
        // Unlimited retention
        return;
    }

    // Get all .wav files in the recordings directory
    let mut recordings: Vec<_> = match std::fs::read_dir(recordings_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "wav")
                    .unwrap_or(false)
            })
            .collect(),
        Err(e) => {
            log::warn!("Failed to read recordings directory for cleanup: {}", e);
            return;
        }
    };

    // Sort by creation time (oldest first)
    recordings.sort_by(|a, b| {
        let time_a = a.metadata().and_then(|m| m.created()).ok();
        let time_b = b.metadata().and_then(|m| m.created()).ok();
        time_a.cmp(&time_b)
    });

    // Delete oldest files until we're at or below the limit
    while recordings.len() > max_count {
        if let Some(oldest) = recordings.first() {
            let path = oldest.path();
            if let Err(e) = std::fs::remove_file(&path) {
                log::warn!("Failed to remove old recording {:?}: {}", path, e);
            } else {
                log::info!("Cleaned up old recording: {:?}", path);
            }
            recordings.remove(0);
        } else {
            break;
        }
    }
}

/// Get the full path to the recordings directory
#[tauri::command]
pub async fn get_recordings_directory(app: AppHandle) -> Result<String, String> {
    let recordings_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("recordings");

    // Create if it doesn't exist
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|e| format!("Failed to create recordings directory: {}", e))?;

    Ok(recordings_dir.to_string_lossy().to_string())
}

/// Open the recordings directory in the system file manager
#[tauri::command]
pub async fn open_recordings_folder(app: AppHandle) -> Result<(), String> {
    let recordings_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?
        .join("recordings");

    // Create directory if it doesn't exist
    if !recordings_dir.exists() {
        std::fs::create_dir_all(&recordings_dir)
            .map_err(|e| format!("Failed to create recordings directory: {}", e))?;
    }

    // Open the directory using the system's file manager
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&recordings_dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        std::process::Command::new("explorer")
            .arg(&recordings_dir)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    Ok(())
}

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
    Remote {
        server_id: String,
        server_name: String,
        host: String,
        port: u16,
        password: Option<String>,
    },
}

impl ActiveEngineSelection {
    fn engine_name(&self) -> &'static str {
        match self {
            ActiveEngineSelection::Whisper { .. } => "whisper",
            ActiveEngineSelection::Parakeet { .. } => "parakeet",
            ActiveEngineSelection::Soniox { .. } => "soniox",
            ActiveEngineSelection::Remote { .. } => "remote",
        }
    }

    fn model_name(&self) -> &str {
        match self {
            ActiveEngineSelection::Whisper { model_name, .. } => model_name,
            ActiveEngineSelection::Parakeet { model_name } => model_name,
            ActiveEngineSelection::Soniox { model_name } => model_name,
            ActiveEngineSelection::Remote { server_name, .. } => server_name,
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

    // Show pill toast for no models error
    pill_toast(app, user_message, 2000);

    // Also emit domain event for main window
    let _ = emit_to_window(
        app,
        "main",
        "no-models-error",
        serde_json::json!({
            "title": "No Models Installed",
            "message": user_message,
            "action": "open-settings"
        }),
    );

    if should_hide_pill(app).await {
        if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
            log::error!("Failed to hide pill window: {}", e);
        }
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
    let validate_start = std::time::Instant::now();
    log::info!("⏱️ [VALIDATE] starting recognition_availability_snapshot");
    let availability = crate::recognition_availability_snapshot(app).await;
    log::info!("⏱️ [VALIDATE] recognition_availability_snapshot complete (+{}ms)", validate_start.elapsed().as_millis());

    if !availability.any_available() {
        log::error!("No speech recognition engines are ready");
        // Emit error event with guidance
        let _ = emit_to_window(
            app,
            "main",
            "no-models-error",
            serde_json::json!({
                "title": "No Speech Recognition Models",
                "message": if availability.soniox_selected && !availability.soniox_ready {
                    "Please configure your Soniox token in Models before recording."
                } else {
                    "Please download at least one model from Models before recording."
                },
                "action": "open-settings"
            }),
        );
        return Err(
            if availability.soniox_selected && !availability.soniox_ready {
                "Soniox token missing".to_string()
            } else {
                "No speech recognition models installed. Please download a model first.".to_string()
            },
        );
    }

    // Check cached license status (populated at startup - no network call)
    // This is fast because it only reads from memory
    let app_state = app.state::<AppState>();
    let cache = app_state.license_cache.read().await;

    if let Some(cached) = cache.as_ref() {
        if matches!(cached.status.status, LicenseState::Expired | LicenseState::None) {
            log::warn!("Recording blocked: license is {:?}", cached.status.status);

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
    }
    // If no cache exists yet, allow recording (startup check hasn't completed)

    log::info!("⏱️ [VALIDATE] validation complete (+{}ms)", validate_start.elapsed().as_millis());
    Ok(())
}

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, RecorderState>,
) -> Result<(), String> {
    let recording_start = Instant::now();

    log_start("RECORDING_START");
    log::info!("⏱️ [REC TIMING] start_recording called (+0ms)");
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
    log::info!("⏱️ [REC TIMING] state check complete (+{}ms)", recording_start.elapsed().as_millis());

    // Validate all requirements upfront
    let validation_start = Instant::now();
    log::info!("⏱️ [REC TIMING] starting validation (+{}ms)", recording_start.elapsed().as_millis());
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
    log::info!("⏱️ [REC TIMING] validation complete (+{}ms)", recording_start.elapsed().as_millis());
    log_state_transition("RECORDING", "idle", "starting", true, None);
    update_recording_state(&app, RecordingState::Starting, None);
    // Ensure transition actually happened; if blocked, abort early
    if !matches!(
        crate::get_recording_state(&app),
        crate::RecordingState::Starting
    ) {
        return Err("Cannot start recording in current state".to_string());
    }

    // Play sound on recording start if enabled
    log::info!("⏱️ [REC TIMING] about to play sound (+{}ms)", recording_start.elapsed().as_millis());
    if let Ok(store) = app.store("settings") {
        let play_sound = store
            .get("play_sound_on_recording")
            .and_then(|v| v.as_bool())
            .unwrap_or(true); // Default to true
        if play_sound {
            play_recording_start_sound();
            // Delay to let sound complete before microphone initialization
            // This helps with Bluetooth headsets (e.g., AirPods) that switch audio modes
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            log::info!("⏱️ [REC TIMING] sound played + 300ms delay (+{}ms)", recording_start.elapsed().as_millis());
        }
    }

    // Load recording config once to avoid repeated store access
    log::info!("⏱️ [REC TIMING] loading recording config (+{}ms)", recording_start.elapsed().as_millis());
    let config = get_recording_config(&app).await.map_err(|e| {
        log::error!("Failed to load recording config: {}", e);
        format!("Configuration error: {}", e)
    })?;
    log::debug!(
        "Using recording config: show_pill={} pill_indicator_mode='{}' ai_enabled={} model={}",
        config.show_pill_widget,
        config.pill_indicator_mode,
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

    // Store path for later use and reset any leftover pending-toggle flag
    let app_state = app.state::<AppState>();
    app_state
        .pending_stop_after_start
        .store(false, std::sync::atomic::Ordering::SeqCst);

    // Save current recording path
    app_state
        .current_recording_path
        .lock()
        .map_err(|e| format!("Failed to acquire path lock: {}", e))?
        .replace(audio_path.clone());

    // Get selected microphone from settings (before acquiring recorder lock)
    log::info!("⏱️ [REC TIMING] getting microphone settings (+{}ms)", recording_start.elapsed().as_millis());
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
    log::info!("⏱️ [REC TIMING] acquiring recorder lock (+{}ms)", recording_start.elapsed().as_millis());
    {
        let mut recorder = state
            .inner()
            .0
            .lock()
            .map_err(|e| format!("Failed to acquire recorder lock: {}", e))?;
        log::info!("⏱️ [REC TIMING] recorder lock acquired (+{}ms)", recording_start.elapsed().as_millis());

        // Check if already recording
        if recorder.is_recording() {
            log::warn!("Already recording!");
            return Err("Already recording".to_string());
        }

        // Log the current audio device before starting
        log::info!("⏱️ [REC TIMING] checking audio device (+{}ms)", recording_start.elapsed().as_millis());
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
        log::info!("⏱️ [REC TIMING] about to call recorder.start_recording (+{}ms)", recording_start.elapsed().as_millis());
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
                log::info!("⏱️ [REC TIMING] recorder.start_recording returned Ok (+{}ms)", recording_start.elapsed().as_millis());
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

                    // Emit user-friendly error via pill toast
                    pill_toast(&app, "Microphone access failed", 1500);

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

                pill_toast(&app, user_message, 1500);

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
    app_state.clear_cancellation();

    // Update state to recording
    update_recording_state(&app, RecordingState::Recording, None);

    // If a toggle-stop was requested while starting, honor it immediately after entering Recording
    if app_state
        .pending_stop_after_start
        .swap(false, std::sync::atomic::Ordering::SeqCst)
    {
        log::info!("Toggle: pending stop triggered right after start; stopping now");
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let recorder_state = app_handle.state::<RecorderState>();
            if let Err(e) = stop_recording(app_handle.clone(), recorder_state).await {
                log::error!("Toggle: pending stop failed: {}", e);
            }
        });
    }

    // Show pill widget if enabled and mode is not "never" (graceful degradation)
    let should_show_pill = config.show_pill_widget && config.pill_indicator_mode != "never";
    log::info!(
        "pill_visibility: start_recording show_pill_widget={} pill_indicator_mode='{}' should_show={}",
        config.show_pill_widget,
        config.pill_indicator_mode,
        should_show_pill
    );
    if should_show_pill {
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
    } else if config.pill_indicator_mode == "never" {
        log::debug!("Pill widget hidden (pill_indicator_mode=never)");
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

        // Play sound on recording end if enabled
        if let Ok(store) = app.store("settings") {
            let play_sound = store
                .get("play_sound_on_recording_end")
                .and_then(|v| v.as_bool())
                .unwrap_or(true); // Default to true
            if play_sound {
                play_recording_end_sound();
            }
        }

        // Monitor system resources after recording stop
        #[cfg(debug_assertions)]
        system_monitor::log_resources_after_operation(
            "RECORDING_STOP",
            stop_start.elapsed().as_millis() as u64,
        );

        // Emit pill toast if recording was stopped due to silence
        if stop_message.contains("silence") {
            pill_toast(&app, "No sound detected", 1000);
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

        // Hide pill window (only if show_pill_indicator is false)
        if should_hide_pill(&app).await {
            if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
                log::error!("Failed to hide pill window: {}", e);
            }
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
            pill_toast(&app, "No audio captured", 1000);
            if let Err(e) = std::fs::remove_file(&audio_path) {
                log::debug!("Failed to remove empty audio file: {}", e);
            }
            // Frontend will hide pill after showing feedback
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

    // Check for active remote server FIRST - if set, use remote transcription
    let remote_settings = app.state::<AsyncMutex<RemoteSettings>>();
    let active_remote = {
        let settings = remote_settings.lock().await;
        log::info!(
            "🔍 [REMOTE DEBUG] Checking remote settings: active_connection_id={:?}, saved_connections={}",
            settings.active_connection_id,
            settings.saved_connections.len()
        );
        let conn = settings.get_active_connection().cloned();
        log::info!("🔍 [REMOTE DEBUG] get_active_connection returned: {:?}", conn.as_ref().map(|c| &c.id));
        conn
    };

    log::info!("🔍 [REMOTE DEBUG] active_remote is_some={}", active_remote.is_some());

    let engine_selection = if let Some(remote_conn) = active_remote {
        log::info!(
            "🌐 Using remote server for transcription: {} ({}:{})",
            remote_conn.display_name(),
            remote_conn.host,
            remote_conn.port
        );
        ActiveEngineSelection::Remote {
            server_id: remote_conn.id.clone(),
            server_name: remote_conn.display_name(),
            host: remote_conn.host,
            port: remote_conn.port,
            password: remote_conn.password,
        }
    } else {
        match config.current_engine.as_str() {
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
        }
    };

    // For Whisper/Parakeet: normalize and duration gate; for Soniox/Remote: skip both
    let audio_path = match &engine_selection {
        ActiveEngineSelection::Soniox { .. } => {
            log::info!("[RECORD] Soniox selected — skipping normalization");
            audio_path
        }
        ActiveEngineSelection::Remote { server_name, .. } => {
            log::info!("[RECORD] Remote server '{}' selected — skipping normalization", server_name);
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
            let (min_duration_s_f32, min_duration_label) = {
                let app_state = app.state::<AppState>();
                let mode = app_state
                    .recording_mode
                    .lock()
                    .ok()
                    .map(|g| *g)
                    .unwrap_or(RecordingMode::Toggle);
                match mode {
                    RecordingMode::PushToTalk => (0.5f32, "0.5".to_string()),
                    RecordingMode::Toggle => (0.5f32, "0.5".to_string()),
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
                    format!("Recording shorter than {} seconds", min_duration_label),
                );
                if let Err(e) = std::fs::remove_file(&normalized_path) {
                    log::debug!("Failed to remove short normalized audio: {}", e);
                }
                // Frontend will hide pill after showing feedback
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
        // Give UI a moment to render the loader before heavy CPU work
        tokio::task::yield_now().await;

        // Check for cancellation before loading model
        let app_state = app_for_task.state::<AppState>();
        if app_state.is_cancellation_requested() {
            log::info!("Transcription cancelled before model loading");

            // Hide pill window since we're cancelling (only if show_pill_indicator is false)
            if should_hide_pill(&app_for_task).await {
                if let Err(e) = crate::commands::window::hide_pill_widget(app_for_task.clone()).await {
                    log::error!("Failed to hide pill window on cancellation: {}", e);
                }
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
                            if should_hide_pill(&app_for_task).await {
                                let _ = crate::commands::window::hide_pill_widget(app_for_task.clone())
                                    .await;
                            }
                            pill_toast(&app_for_task, &e, 1500);
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
                    pill_toast(&app_for_task, &message, 1500);
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
            ActiveEngineSelection::Remote {
                server_name,
                host,
                port,
                password,
                ..
            } => {
                // Perform remote transcription in an async block that returns Result
                let remote_result: Result<String, String> = async {
                    let remote_start = std::time::Instant::now();
                    log::info!(
                        "🌐 [Remote] Starting transcription to '{}' ({}:{})",
                        server_name,
                        host,
                        port
                    );

                    // Read the audio file
                    let audio_data = std::fs::read(&audio_path_clone)
                        .map_err(|e| format!("Failed to read audio file: {}", e))?;

                    let audio_size_kb = audio_data.len() as f64 / 1024.0;
                    log::info!(
                        "🌐 [Remote] Sending {:.1} KB audio to '{}' (+{}ms)",
                        audio_size_kb,
                        server_name,
                        remote_start.elapsed().as_millis()
                    );

                    // Create HTTP client connection
                    let server_conn = RemoteServerConnection::new(
                        host.clone(),
                        *port,
                        password.clone(),
                    );

                    // Send transcription request with short connection timeout
                    // Use 5 second connect timeout to fail fast on unreachable servers
                    let client = reqwest::Client::builder()
                        .connect_timeout(std::time::Duration::from_secs(5))
                        .timeout(std::time::Duration::from_secs(120)) // Overall timeout for large files
                        .build()
                        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

                    let mut request = client
                        .post(&server_conn.transcribe_url())
                        .header("Content-Type", "audio/wav")
                        .body(audio_data);

                    // Add auth header if password is set
                    if let Some(pwd) = server_conn.password.as_ref() {
                        request = request.header("X-VoiceTypr-Key", pwd);
                    }

                    log::info!(
                        "🌐 [Remote] Sending HTTP request to '{}' (+{}ms)",
                        server_name,
                        remote_start.elapsed().as_millis()
                    );

                    let response = request.send().await.map_err(|e| {
                        log::warn!(
                            "🌐 [Remote] Connection FAILED to '{}' after {}ms: {}",
                            server_name,
                            remote_start.elapsed().as_millis(),
                            e
                        );
                        format!("Failed to connect to remote server: {}", e)
                    })?;

                    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                        log::warn!(
                            "🌐 [Remote] Authentication FAILED to '{}'",
                            server_name
                        );
                        return Err("Remote server authentication failed".to_string());
                    }

                    if !response.status().is_success() {
                        log::warn!(
                            "🌐 [Remote] Server error from '{}': {}",
                            server_name,
                            response.status()
                        );
                        return Err(format!("Remote server error: {}", response.status()));
                    }

                    let result: serde_json::Value = response.json().await.map_err(|e| {
                        format!("Failed to parse remote response: {}", e)
                    })?;

                    let text = result
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .ok_or_else(|| "Invalid response: missing 'text' field".to_string())?;

                    log::info!(
                        "🌐 [Remote] Transcription COMPLETED from '{}': {} chars received",
                        server_name,
                        text.len()
                    );

                    Ok(text)
                }
                .await;

                // Pass through the result like Soniox does
                match remote_result {
                    Ok(text) => Ok(text),
                    Err(e) => Err(e),
                }
            }
        };

        // Try to save recording to persistent storage BEFORE cleanup
        // On success: use maybe_save_recording (respects save_recordings setting)
        // On recoverable failure (remote server errors): always save to preserve for re-transcription
        let recording_file = match &transcription_result {
            Ok(_) => maybe_save_recording(&app_for_task, &audio_path_clone).await,
            Err(e) => {
                // For remote server errors, always preserve the recording for re-transcription
                if e.contains("remote server") || e.contains("Remote server") || e.contains("Connection refused") {
                    log::info!("Preserving recording for failed remote transcription");
                    save_recording(&app_for_task, &audio_path_clone).await
                } else {
                    None
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

                    // Hide pill window since we're cancelling (only if show_pill_indicator is false)
                    if should_hide_pill(&app_for_task).await {
                        if let Err(e) =
                            crate::commands::window::hide_pill_widget(app_for_task.clone()).await
                        {
                            log::error!("Failed to hide pill window on cancellation: {}", e);
                        }
                    }

                    update_recording_state(&app_for_task, RecordingState::Idle, None);
                    return;
                }

                log::debug!("Transcription successful, {} chars", text.len());

                // Check if transcription is empty or just noise
                if text.is_empty() || text.trim().is_empty() || text == "[BLANK_AUDIO]" {
                    log::info!("Whisper returned empty transcription - no speech detected");

                    // Emit graceful feedback to user via pill toast
                    pill_toast(
                        &app_for_task,
                        "No speech detected - try speaking closer to the microphone",
                        1500,
                    );

                    // Wait for feedback to show before hiding pill
                    let app_for_hide = app_for_task.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

                        // Hide pill window (only if show_pill_indicator is false)
                        if should_hide_pill(&app_for_hide).await {
                            if let Err(e) =
                                crate::commands::window::hide_pill_widget(app_for_hide.clone()).await
                            {
                                log::error!("Failed to hide pill window: {}", e);
                            }
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
                    let _ = app_for_task.emit("enhancing-started", ());
                }

                // Backend handles the complete flow
                let app_for_process = app_for_task.clone();
                let text_for_process = text.clone();
                let model_for_process = selected_model_name_for_task.clone();
                let ai_enabled_for_task = ai_enabled; // Capture from cached config
                let recording_file_for_task = recording_file.clone(); // Capture recording file

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
                                    // Emit enhancing completed event (global)
                                    let _ = app_for_process.emit("enhancing-completed", ());

                                    if enhanced != text_for_process {
                                        log::info!("AI enhancement applied successfully");
                                    }
                                    enhanced
                                }
                                Err(e) => {
                                    log::warn!("Formatting failed, using original text: {}", e);

                                    // Emit enhancing failed to reset pill state
                                    let _ = app_for_process.emit("enhancing-failed", ());

                                    // Check error type and create appropriate message
                                    let error_message = e.to_string();
                                    let user_message = if error_message.contains("400")
                                        || error_message.contains("Bad Request")
                                    {
                                        "Formatting failed: API key missing or invalid"
                                    } else if error_message.contains("401")
                                        || error_message.contains("Unauthorized")
                                    {
                                        "Formatting failed: API key unauthorized"
                                    } else if error_message.contains("429") {
                                        "Formatting failed: Rate limit exceeded"
                                    } else if error_message.contains("network")
                                        || error_message.contains("connection")
                                    {
                                        "Formatting failed: Network error"
                                    } else {
                                        "Formatting failed: Service unavailable"
                                    };

                                    // Show pill toast for formatting failure
                                    log::warn!("Formatting failed; showing pill toast");
                                    pill_toast(&app_for_process, user_message, 1500);

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

                    // Hide pill window first (only if show_pill_indicator is false)
                    if should_hide_pill(&app_for_process).await {
                        if let Some(window_manager) = app_state.get_window_manager() {
                            if let Err(e) = window_manager.hide_pill_window().await {
                                log::error!("Failed to hide pill window: {}", e);
                            }
                        } else {
                            log::error!("WindowManager not initialized");
                        }
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
                                // Show pill toast for accessibility permission error
                                pill_toast(
                                    &app_for_process,
                                    "Text copied - grant permission to auto-paste",
                                    1500,
                                );
                            } else {
                                // Generic paste error
                                pill_toast(
                                    &app_for_process,
                                    "Paste failed - text in clipboard",
                                    1500,
                                );
                            }
                        }
                    }

                    // 5. Save transcription to history (async, non-blocking)
                    let app_for_history = app_for_process.clone();
                    let history_text = final_text.clone();
                    let history_model = model_for_process.clone();
                    let recording_file_for_history = recording_file_for_task.clone();
                    tokio::spawn(async move {
                        match save_transcription_with_recording(
                            app_for_history.clone(),
                            history_text,
                            history_model,
                            recording_file_for_history,
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
                    // For cancellation, hide pill (only if show_pill_indicator is false) and go to Idle
                    if should_hide_pill(&app_for_task).await {
                        if let Err(hide_err) =
                            crate::commands::window::hide_pill_widget(app_for_task.clone()).await
                        {
                            log::error!("Failed to hide pill window on cancellation: {}", hide_err);
                        }
                    }
                    update_recording_state(&app_for_task, RecordingState::Idle, None);
                } else if e.contains("too short") {
                    // Handle "too short" errors with specific user feedback
                    log::info!("Recording was too short: {}", e);

                    // Clean up the audio file
                    if let Err(cleanup_err) = std::fs::remove_file(&audio_path_clone) {
                        log::warn!("Failed to remove short audio file: {}", cleanup_err);
                    }

                    // Emit specific feedback via pill toast
                    pill_toast(&app_for_task, &e, 1000);

                    // Hide pill after showing feedback
                    let app_for_reset = app_for_task.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

                        // Only hide if show_pill_indicator is false
                        if should_hide_pill(&app_for_reset).await {
                            if let Err(e) =
                                crate::commands::window::hide_pill_widget(app_for_reset.clone()).await
                            {
                                log::error!("Failed to hide pill window: {}", e);
                            }
                        }

                        update_recording_state(&app_for_reset, RecordingState::Idle, None);
                    });
                } else if e.contains("remote server") || e.contains("Remote server") || e.contains("Connection refused") {
                    // Remote server error - emit specific event for system notification
                    log::warn!("Remote server error: {}", e);

                    // Save failed transcription to history if we preserved the recording
                    if let Some(ref saved_recording) = recording_file {
                        let app_for_history = app_for_task.clone();
                        let error_msg = e.clone();
                        let model_name = selected_model_name_for_task.clone();
                        let recording_filename = saved_recording.clone();
                        tokio::spawn(async move {
                            if let Err(save_err) = save_failed_transcription(
                                &app_for_history,
                                error_msg,
                                model_name,
                                recording_filename,
                            ).await {
                                log::error!("Failed to save failed transcription: {}", save_err);
                            }
                        });

                        // Emit event for frontend to show system notification with guidance
                        let _ = app_for_task.emit("remote-server-error", serde_json::json!({
                            "message": e.clone(),
                            "title": "Remote Server Unreachable"
                        }));

                        // Update pill message to guide user to History
                        pill_toast(&app_for_task, "Remote server unreachable. Go to History to re-transcribe, or select a different model.", 6000);
                    } else {
                        // Emit event for frontend - no recording saved
                        let _ = app_for_task.emit("remote-server-error", serde_json::json!({
                            "message": e.clone(),
                            "title": "Remote Server Unavailable"
                        }));
                        pill_toast(&app_for_task, "Remote server unavailable", 2000);
                    }

                    update_recording_state(&app_for_task, RecordingState::Error, Some(e.clone()));

                    // Transition back to Idle after showing the error
                    let app_for_reset = app_for_task.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        if should_hide_pill(&app_for_reset).await {
                            if let Err(e) =
                                crate::commands::window::hide_pill_widget(app_for_reset.clone()).await
                            {
                                log::error!("Failed to hide pill window: {}", e);
                            }
                        }
                        update_recording_state(&app_for_reset, RecordingState::Idle, None);
                    });
                } else {
                    // For other errors, show error state briefly
                    update_recording_state(&app_for_task, RecordingState::Error, Some(e.clone()));

                    // Emit error via pill toast
                    pill_toast(&app_for_task, &e, 1500);

                    // Transition back to Idle after a delay
                    // This ensures we don't get stuck in Error state
                    let app_for_reset = app_for_task.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        log::debug!(
                            "Resetting from Error to Idle state after transcription failure"
                        );

                        // Hide pill window when transitioning to Idle (only if show_pill_indicator is false)
                        if should_hide_pill(&app_for_reset).await {
                            if let Err(e) =
                                crate::commands::window::hide_pill_widget(app_for_reset.clone()).await
                            {
                                log::error!("Failed to hide pill window: {}", e);
                            }
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

/// Get available audio input devices.
/// Returns empty list if onboarding not completed (to avoid triggering permission prompt).
#[tauri::command]
pub async fn get_audio_devices(app: AppHandle) -> Result<Vec<String>, String> {
    // Check onboarding status - don't enumerate devices until onboarding is complete
    // This prevents early mic permission prompts from CPAL's input_devices() enumeration
    let onboarding_done = {
        use tauri_plugin_store::StoreExt;
        app.store("settings")
            .ok()
            .and_then(|store| store.get("onboarding_completed").and_then(|v| v.as_bool()))
            .unwrap_or(false)
    };

    if !onboarding_done {
        log::debug!("get_audio_devices: onboarding not complete, returning empty list");
        return Ok(Vec::new());
    }

    Ok(AudioRecorder::get_devices())
}

/// Get the current default audio input device.
/// Returns error if onboarding not completed (to avoid triggering permission prompt).
#[tauri::command]
pub async fn get_current_audio_device(app: AppHandle) -> Result<String, String> {
    // Check onboarding status - don't access devices until onboarding is complete
    // This prevents early mic permission prompts from CPAL's default_input_device() access
    let onboarding_done = {
        use tauri_plugin_store::StoreExt;
        app.store("settings")
            .ok()
            .and_then(|store| store.get("onboarding_completed").and_then(|v| v.as_bool()))
            .unwrap_or(false)
    };

    if !onboarding_done {
        log::debug!("get_current_audio_device: onboarding not complete, returning error");
        return Err("Onboarding not completed".to_string());
    }

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

/// Save transcription to history without a recording file
#[tauri::command]
pub async fn save_transcription(app: AppHandle, text: String, model: String) -> Result<(), String> {
    save_transcription_with_recording(app, text, model, None).await
}

/// Save transcription to history with optional recording file reference
pub async fn save_transcription_with_recording(
    app: AppHandle,
    text: String,
    model: String,
    recording_file: Option<String>,
) -> Result<(), String> {
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
    let mut transcription_data = serde_json::json!({
        "text": text.clone(),
        "model": model,
        "timestamp": timestamp.clone()
    });

    // Add recording_file if present
    if let Some(ref file) = recording_file {
        transcription_data["recording_file"] = serde_json::json!(file);
        log::info!("Saving transcription with recording file: {}", file);
    }

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

/// Save a failed transcription to history with recording file preserved for re-transcription
pub async fn save_failed_transcription(
    app: &AppHandle,
    error_message: String,
    model: String,
    recording_file: String,
) -> Result<(), String> {
    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;

    let timestamp = chrono::Utc::now().to_rfc3339();
    let transcription_data = serde_json::json!({
        "text": "Remote server unreachable - re-transcribe to get text",
        "model": model,
        "timestamp": timestamp.clone(),
        "recording_file": recording_file,
        "status": "failed",
        "error_detail": error_message
    });

    store.set(&timestamp, transcription_data.clone());

    store
        .save()
        .map_err(|e| format!("Failed to save failed transcription: {}", e))?;

    // Emit the new transcription data to frontend
    let _ = emit_to_window(app, "main", "transcription-added", transcription_data);

    // Refresh tray menu
    if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
        log::warn!("Failed to update tray menu after saving failed transcription: {}", e);
    }

    log::info!("Saved failed transcription with recording file: {}", recording_file);
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

/// Get the total count of transcriptions in history
/// This is more efficient than loading all history when only the count is needed
#[tauri::command]
pub async fn get_transcription_count(app: AppHandle) -> Result<usize, String> {
    let store = app.store("transcriptions").map_err(|e| e.to_string())?;
    Ok(store.keys().len())
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
        ActiveEngineSelection::Remote {
            server_name,
            host,
            port,
            password,
            ..
        } => {
            // Normalize to Whisper contract (16k mono s16 WAV) for remote transcription
            log::debug!("[UPLOAD] Normalizing to Whisper WAV for remote transcription...");
            let normalized_path = {
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let out_path = recordings_dir.join(format!("normalized_{}.wav", ts));
                crate::ffmpeg::normalize_streaming(&app, &wav_path, &out_path)
                    .await
                    .map_err(|e| format!("Audio normalization (ffmpeg) failed: {}", e))?;
                out_path
            };
            log::info!("[UPLOAD] Normalized WAV at {:?}", normalized_path);

            log::info!(
                "🌐 [Remote Upload] Starting transcription to '{}' ({}:{})",
                server_name,
                host,
                port
            );

            // Read the normalized audio file
            let audio_data = std::fs::read(&normalized_path)
                .map_err(|e| format!("Failed to read audio file: {}", e))?;

            let audio_size_kb = audio_data.len() as f64 / 1024.0;
            log::info!(
                "🌐 [Remote Upload] Sending {:.1} KB audio to '{}'",
                audio_size_kb,
                server_name
            );

            // Create HTTP client connection
            let server_conn = RemoteServerConnection::new(host.clone(), port, password.clone());

            // Send transcription request with extended timeout for large files
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout for large files
                .build()
                .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

            let mut request = client
                .post(&server_conn.transcribe_url())
                .header("Content-Type", "audio/wav")
                .body(audio_data);

            // Add auth header if password is set
            if let Some(pwd) = server_conn.password.as_ref() {
                request = request.header("X-VoiceTypr-Key", pwd);
            }

            let response = request.send().await.map_err(|e| {
                log::warn!(
                    "🌐 [Remote Upload] Connection FAILED to '{}': {}",
                    server_name,
                    e
                );
                format!("Failed to connect to remote server: {}", e)
            })?;

            // Clean up normalized file
            let _ = std::fs::remove_file(&normalized_path);

            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                log::warn!(
                    "🌐 [Remote Upload] Authentication FAILED to '{}'",
                    server_name
                );
                return Err("Remote server authentication failed".to_string());
            }

            if !response.status().is_success() {
                log::warn!(
                    "🌐 [Remote Upload] Server error from '{}': {}",
                    server_name,
                    response.status()
                );
                return Err(format!("Remote server error: {}", response.status()));
            }

            let result: serde_json::Value = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse remote response: {}", e))?;

            let text = result
                .get("text")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| "Invalid response: missing 'text' field".to_string())?;

            log::info!(
                "🌐 [Remote Upload] Transcription COMPLETED from '{}': {} chars received",
                server_name,
                text.len()
            );

            text
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
        ActiveEngineSelection::Remote {
            server_name,
            host,
            port,
            password,
            ..
        } => {
            // Normalize to Whisper contract (16k mono s16 WAV) for remote transcription
            log::debug!("[CLIPBOARD] Normalizing to Whisper WAV for remote transcription...");
            let normalized_path = {
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let out_path = recordings_dir.join(format!("normalized_clipboard_{}.wav", ts));
                crate::ffmpeg::normalize_streaming(&app, &temp_path, &out_path)
                    .await
                    .map_err(|e| format!("Audio normalization (ffmpeg) failed: {}", e))?;
                out_path
            };
            log::info!("[CLIPBOARD] Normalized WAV at {:?}", normalized_path);

            log::info!(
                "🌐 [Remote Clipboard] Starting transcription to '{}' ({}:{})",
                server_name,
                host,
                port
            );

            // Read the normalized audio file
            let audio_data = std::fs::read(&normalized_path)
                .map_err(|e| format!("Failed to read audio file: {}", e))?;

            let audio_size_kb = audio_data.len() as f64 / 1024.0;
            log::info!(
                "🌐 [Remote Clipboard] Sending {:.1} KB audio to '{}'",
                audio_size_kb,
                server_name
            );

            // Create HTTP client connection
            let server_conn = RemoteServerConnection::new(host.clone(), port, password.clone());

            // Send transcription request with extended timeout for large files
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout for large files
                .build()
                .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

            let mut request = client
                .post(&server_conn.transcribe_url())
                .header("Content-Type", "audio/wav")
                .body(audio_data);

            // Add auth header if password is set
            if let Some(pwd) = server_conn.password.as_ref() {
                request = request.header("X-VoiceTypr-Key", pwd);
            }

            let response = request.send().await.map_err(|e| {
                log::warn!(
                    "🌐 [Remote Clipboard] Connection FAILED to '{}': {}",
                    server_name,
                    e
                );
                format!("Failed to connect to remote server: {}", e)
            })?;

            // Clean up normalized file
            let _ = std::fs::remove_file(&normalized_path);

            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                log::warn!(
                    "🌐 [Remote Clipboard] Authentication FAILED to '{}'",
                    server_name
                );
                return Err("Remote server authentication failed".to_string());
            }

            if !response.status().is_success() {
                log::warn!(
                    "🌐 [Remote Clipboard] Server error from '{}': {}",
                    server_name,
                    response.status()
                );
                return Err(format!("Remote server error: {}", response.status()));
            }

            let result: serde_json::Value = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse remote response: {}", e))?;

            let text = result
                .get("text")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| "Invalid response: missing 'text' field".to_string())?;

            log::info!(
                "🌐 [Remote Clipboard] Transcription COMPLETED from '{}': {} chars received",
                server_name,
                text.len()
            );

            text
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
        "model": "stt-async-v3",
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

    // Hide pill window immediately (only if show_pill_indicator is false)
    if should_hide_pill(&app).await {
        if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
            log::error!("Failed to hide pill window: {}", e);
        }
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

/// Check if a recording file exists in the recordings directory
#[tauri::command]
pub async fn check_recording_exists(app: AppHandle, filename: String) -> Result<bool, String> {
    let recordings_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("recordings");
    Ok(recordings_dir.join(&filename).exists())
}

/// Get the full path to a recording file for playback
#[tauri::command]
pub async fn get_recording_path(app: AppHandle, filename: String) -> Result<String, String> {
    let recordings_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("recordings");
    let file_path = recordings_dir.join(&filename);
    if !file_path.exists() {
        return Err(format!("Recording file not found: {}", filename));
    }
    Ok(file_path.to_string_lossy().to_string())
}

/// Save a re-transcription to history, linking to the original recording
#[tauri::command]
pub async fn save_retranscription(
    app: AppHandle,
    text: String,
    model: String,
    recording_file: String,
    source_recording_id: String,
) -> Result<(), String> {
    // Save transcription to store with current timestamp
    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;

    let timestamp = chrono::Utc::now().to_rfc3339();
    let transcription_data = serde_json::json!({
        "text": text.clone(),
        "model": model,
        "timestamp": timestamp.clone(),
        "recording_file": recording_file,
        "source_recording_id": source_recording_id,
        "is_retranscription": true
    });

    store.set(&timestamp, transcription_data.clone());

    store
        .save()
        .map_err(|e| format!("Failed to save retranscription: {}", e))?;

    // Emit the new transcription data to frontend for append-only update
    let _ = emit_to_window(&app, "main", "transcription-added", transcription_data);

    // Refresh tray menu (best-effort) so Recent Transcriptions stays updated
    if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
        log::warn!(
            "Failed to update tray menu after saving retranscription: {}",
            e
        );
    }

    log::info!(
        "Saved retranscription with {} characters (source: {})",
        text.len(),
        source_recording_id
    );
    Ok(())
}

/// Update an existing transcription entry in place (for re-transcription)
#[tauri::command]
pub async fn update_transcription(
    app: AppHandle,
    timestamp: String,
    text: String,
    model: String,
) -> Result<(), String> {
    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;

    // Get the existing entry
    let existing = store
        .get(&timestamp)
        .ok_or_else(|| format!("Transcription not found: {}", timestamp))?;

    // Preserve original fields, update text, model, and status
    let mut updated = existing.clone();
    if let serde_json::Value::Object(ref mut map) = updated {
        map.insert("text".to_string(), serde_json::Value::String(text.clone()));
        map.insert("model".to_string(), serde_json::Value::String(model.clone()));
        map.insert("status".to_string(), serde_json::Value::String("completed".to_string()));
    }

    store.set(&timestamp, updated.clone());

    store
        .save()
        .map_err(|e| format!("Failed to save updated transcription: {}", e))?;

    // Emit update event to frontend
    let _ = emit_to_window(&app, "main", "transcription-updated", serde_json::json!({
        "timestamp": timestamp,
        "text": text,
        "model": model
    }));

    // Refresh tray menu (best-effort)
    if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
        log::warn!(
            "Failed to update tray menu after updating transcription: {}",
            e
        );
    }

    log::info!(
        "Updated transcription {} with {} characters",
        timestamp,
        text.len()
    );
    Ok(())
}

/// Open the file explorer with the specified file selected
#[tauri::command]
pub async fn show_in_folder(path: String) -> Result<(), String> {
    let path = std::path::Path::new(&path);

    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }

    #[cfg(target_os = "windows")]
    {
        // Use explorer.exe /select to open folder with file selected
        std::process::Command::new("explorer.exe")
            .args(["/select,", &path.to_string_lossy()])
            .spawn()
            .map_err(|e| format!("Failed to open explorer: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        // Use open -R to reveal file in Finder
        std::process::Command::new("open")
            .args(["-R", &path.to_string_lossy()])
            .spawn()
            .map_err(|e| format!("Failed to open Finder: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        // Try xdg-open on the parent directory
        if let Some(parent) = path.parent() {
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| format!("Failed to open file manager: {}", e))?;
        }
    }

    Ok(())
}
