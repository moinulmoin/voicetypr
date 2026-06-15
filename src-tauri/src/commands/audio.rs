use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::audio::recorder::AudioRecorder;
use crate::audio::silence_detector::SilenceDetectorEvent;
use crate::commands::license::{check_license_status_internal, CachedLicense};
#[cfg(target_os = "windows")]
use crate::commands::settings::normalize_transcription_acceleration;
use crate::commands::settings::{get_settings, resolve_pill_indicator_mode, Settings};
use crate::license::LicenseState;
use crate::media::MediaPauseController;
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
use once_cell::sync::Lazy;
use serde_json;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tauri_plugin_store::StoreExt;

struct StopInFlightGuard(Arc<AtomicBool>);

impl Drop for StopInFlightGuard {
    fn drop(&mut self) {
        self.0.store(false, AtomicOrdering::SeqCst);
    }
}

/// When `stop_recording` finds no active recorder, only force the state back to
/// Idle if a stop/transcribe flow is NOT already in progress. Writing Idle during
/// Stopping/Transcribing would stomp a flow another caller already started (for
/// example the recorder watchdog racing a user toggle off).
fn stop_should_reset_to_idle(current: RecordingState) -> bool {
    !matches!(
        current,
        RecordingState::Stopping | RecordingState::Transcribing
    )
}

pub(crate) const PTT_START_ABORTED_AFTER_RELEASE: &str =
    "PTT key released before recording could start";
const LICENSE_CHECK_TIMEOUT_SECS: u64 = 3;
const STALE_TRIAL_LICENSE_FALLBACK_MAX_AGE_SECS: u64 = 24 * 60 * 60;
const TRANSCRIPTION_TIMED_OUT: &str = "Transcription timed out";
const CPU_TRANSCRIPTION_TIMEOUT_MIN_SECS: f32 = 20.0;
const CPU_TRANSCRIPTION_TIMEOUT_MAX_SECS: f32 = 60.0;
const CPU_TRANSCRIPTION_TIMEOUT_SECONDS_PER_AUDIO_SECOND: f32 = 4.0;
const CPU_TRANSCRIPTION_ABORT_GRACE_SECS: u64 = 2;
const PREVIOUS_TRANSCRIPTION_STOPPING: &str = "Previous transcription is still stopping";

static RECORDING_WHISPER_CPU_DECODE_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

struct RecordingWhisperCpuDecodeGuard;

impl RecordingWhisperCpuDecodeGuard {
    fn try_acquire() -> Result<Self, String> {
        RECORDING_WHISPER_CPU_DECODE_IN_FLIGHT
            .compare_exchange(false, true, AtomicOrdering::SeqCst, AtomicOrdering::SeqCst)
            .map(|_| Self)
            .map_err(|_| PREVIOUS_TRANSCRIPTION_STOPPING.to_string())
    }
}

impl Drop for RecordingWhisperCpuDecodeGuard {
    fn drop(&mut self) {
        RECORDING_WHISPER_CPU_DECODE_IN_FLIGHT.store(false, AtomicOrdering::SeqCst);
    }
}

fn cpu_transcription_timeout(duration_s: f32) -> Duration {
    let scaled_secs = if duration_s.is_finite() && duration_s > 0.0 {
        duration_s * CPU_TRANSCRIPTION_TIMEOUT_SECONDS_PER_AUDIO_SECOND
    } else {
        CPU_TRANSCRIPTION_TIMEOUT_MIN_SECS
    };

    Duration::from_secs_f32(scaled_secs.clamp(
        CPU_TRANSCRIPTION_TIMEOUT_MIN_SECS,
        CPU_TRANSCRIPTION_TIMEOUT_MAX_SECS,
    ))
}

fn is_non_retryable_transcription_error(error: &str) -> bool {
    error.contains("cancelled")
        || error == TRANSCRIPTION_TIMED_OUT
        || error == PREVIOUS_TRANSCRIPTION_STOPPING
}

async fn wait_for_decode_to_stop(
    decode: &mut tokio::task::JoinHandle<Result<String, String>>,
    grace: Duration,
) -> Result<(), String> {
    match tokio::time::timeout(grace, decode).await {
        Ok(join_result) => {
            if let Err(e) = join_result {
                return Err(format!("Transcription task failed: {e}"));
            }
        }
        Err(_) => {
            log::warn!(
                "CPU transcription did not stop within {:?}; leaving worker to unwind",
                grace
            );
        }
    }

    Ok(())
}

/// Atomic counter for toast IDs to prevent race conditions
static TOAST_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Global media pause controller for pausing/resuming system media during recording
static MEDIA_CONTROLLER: Lazy<MediaPauseController> = Lazy::new(MediaPauseController::new);

/// Pill toast show/clear action for the unified toast event.
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PillToastAction {
    Show,
    Clear,
}

/// Visual variant for pill toast messages.
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PillToastVariant {
    Info,
    Warning,
}

/// Payload for pill toast messages
#[derive(serde::Serialize, Clone)]
pub struct PillToastPayload {
    pub id: u64,
    pub action: PillToastAction,
    pub message: String,
    pub duration_ms: u64,
    pub variant: PillToastVariant,
    pub persistent: bool,
}

fn next_pill_toast_id() -> u64 {
    TOAST_ID_COUNTER
        .fetch_add(1, AtomicOrdering::SeqCst)
        .wrapping_add(1)
}

/// Returns true when a stale clear may hide the toast window (compare_exchange succeeded).
pub(crate) fn pill_toast_clear_may_hide_window(toast_id: u64) -> bool {
    TOAST_ID_COUNTER
        .compare_exchange(
            toast_id,
            toast_id.wrapping_add(1),
            AtomicOrdering::SeqCst,
            AtomicOrdering::SeqCst,
        )
        .is_ok()
}

fn emit_pill_toast(
    app: &AppHandle,
    id: u64,
    message: &str,
    duration_ms: u64,
    variant: PillToastVariant,
    persistent: bool,
) {
    if let Some(toast_window) = app.get_webview_window("toast") {
        let _ = toast_window.show();

        if !persistent {
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(duration_ms)).await;
                if TOAST_ID_COUNTER.load(AtomicOrdering::SeqCst) == id {
                    if let Some(tw) = app_clone.get_webview_window("toast") {
                        let _ = tw.hide();
                    }
                }
            });
        }
    } else {
        log::warn!(
            "pill_toast: toast window not found, message not shown: {}",
            message
        );
    }

    let payload = PillToastPayload {
        id,
        action: PillToastAction::Show,
        message: message.to_string(),
        duration_ms,
        variant,
        persistent,
    };

    let _ = app.emit("toast", payload);
}

/// Show a toast message on the pill's toast window (above the pill).
/// This is the single unified API for pill feedback messages.
pub fn pill_toast(app: &AppHandle, message: &str, duration_ms: u64) -> u64 {
    pill_toast_with_variant(app, message, duration_ms, PillToastVariant::Info)
}

pub fn pill_toast_with_variant(
    app: &AppHandle,
    message: &str,
    duration_ms: u64,
    variant: PillToastVariant,
) -> u64 {
    let id = next_pill_toast_id();
    emit_pill_toast(app, id, message, duration_ms, variant, false);
    id
}

pub fn pill_toast_persistent(app: &AppHandle, message: &str, variant: PillToastVariant) -> u64 {
    let id = next_pill_toast_id();
    emit_pill_toast(app, id, message, 0, variant, true);
    id
}

pub fn clear_pill_toast(app: &AppHandle, toast_id: u64) {
    if !pill_toast_clear_may_hide_window(toast_id) {
        return;
    }

    if let Some(toast_window) = app.get_webview_window("toast") {
        let _ = toast_window.hide();
    }

    let payload = PillToastPayload {
        id: toast_id,
        action: PillToastAction::Clear,
        message: String::new(),
        duration_ms: 0,
        variant: PillToastVariant::Info,
        persistent: false,
    };
    let _ = app.emit("toast", payload);
}

fn should_hide_pill_when_idle(mode: &str) -> bool {
    mode != "always"
}

/// Best-effort warm of the Windows Vulkan sidecar when a Whisper model is preloaded.
/// No-op on non-Windows platforms and when CPU acceleration is selected.
pub(crate) async fn warm_whisper_gpu_sidecar_on_model_preload(
    app: &AppHandle,
    model_path: &Path,
) -> bool {
    #[cfg(target_os = "windows")]
    {
        let mode = get_settings(app.clone())
            .await
            .map(|settings| {
                normalize_transcription_acceleration(Some(&settings.transcription_acceleration))
            })
            .unwrap_or_else(|error| {
                log::warn!(
                    "Failed to read settings for Vulkan sidecar preload warm; defaulting to auto: {error}"
                );
                crate::commands::settings::DEFAULT_TRANSCRIPTION_ACCELERATION.to_string()
            });

        let gpu_client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
        let gpu_available = gpu_client.status().await.gpu_available;
        gpu_client
            .warm_on_preload(app, model_path, &mode, gpu_available)
            .await
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, model_path);
        false
    }
}

async fn transcribe_whisper_with_acceleration(
    app: &AppHandle,
    model_path: &Path,
    audio_path: &Path,
    language: Option<&str>,
    translate: bool,
    cancel_flag: Arc<AtomicBool>,
    cpu_timeout: Option<Duration>,
) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    let mode = {
        let settings = get_settings(app.clone()).await?;
        normalize_transcription_acceleration(Some(&settings.transcription_acceleration))
    };
    #[cfg(target_os = "windows")]
    log::info!("Transcription acceleration mode: {}", mode);

    #[cfg(target_os = "windows")]
    let mut preserve_gpu_status = false;

    #[cfg(target_os = "windows")]
    if mode != "cpu" {
        if is_cancelled(&cancel_flag) {
            return Err("Transcription cancelled".to_string());
        }

        let gpu_client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
        let status = gpu_client.status().await;
        let should_try_gpu = mode == "gpu" || status.gpu_available != Some(false);
        log::info!(
            "Transcription acceleration decision: mode={}, gpu_available={:?}, should_try_gpu={}",
            mode,
            status.gpu_available,
            should_try_gpu
        );

        if should_try_gpu {
            let gpu_result = tokio::select! {
                result = gpu_client.transcribe(
                    app,
                    model_path,
                    audio_path,
                    language,
                    translate,
                    &mode,
                ) => result,
                _ = wait_for_cancellation(cancel_flag.clone()) => {
                    Err("Transcription cancelled".to_string())
                }
            };

            match gpu_result {
                Ok(text) => {
                    log::info!("Whisper transcription completed with Vulkan sidecar");
                    return Ok(text);
                }
                Err(error) if error == "Transcription cancelled" => {
                    log::info!("Cancelling in-flight Whisper Vulkan sidecar transcription");
                    gpu_client.abort_active_process().await;
                    return Err(error);
                }
                Err(error) => {
                    preserve_gpu_status = true;
                    log::warn!(
                        "Whisper Vulkan sidecar unavailable in {} mode; falling back to CPU: {}",
                        mode,
                        error
                    );
                    if mode == "gpu" {
                        pill_toast(
                            app,
                            "GPU acceleration is unavailable. Using CPU mode for this transcription.",
                            4000,
                        );
                    }
                }
            }
        } else {
            preserve_gpu_status = true;
            log::info!("Skipping Vulkan sidecar in auto mode after previous GPU failure");
        }
    }

    let transcriber = {
        let cache_state = app.state::<AsyncMutex<TranscriberCache>>();
        let mut cache = cache_state.lock().await;
        cache.get_or_create(model_path)?
    };

    let audio_path = audio_path.to_path_buf();
    let language = language.map(str::to_owned);
    let local_abort_flag = Arc::new(AtomicBool::new(false));
    let local_abort_for_decode = Arc::clone(&local_abort_flag);
    let cancel_for_decode = cancel_flag.clone();
    let decode_guard = if cpu_timeout.is_some() {
        Some(RecordingWhisperCpuDecodeGuard::try_acquire()?)
    } else {
        None
    };
    let transcriber_for_decode = Arc::clone(&transcriber);
    let mut decode = tokio::task::spawn_blocking(move || {
        let _decode_guard = decode_guard;
        transcriber_for_decode.transcribe_with_cancellation(
            &audio_path,
            language.as_deref(),
            translate,
            cancel_for_decode,
            local_abort_for_decode,
        )
    });

    let result = if let Some(timeout) = cpu_timeout {
        tokio::select! {
            join_result = &mut decode => {
                join_result.map_err(|e| format!("Transcription task failed: {e}"))?
            }
            _ = wait_for_cancellation(cancel_flag.clone()) => {
                local_abort_flag.store(true, AtomicOrdering::SeqCst);
                wait_for_decode_to_stop(
                    &mut decode,
                    Duration::from_secs(CPU_TRANSCRIPTION_ABORT_GRACE_SECS),
                )
                .await?;
                Err("Transcription cancelled".to_string())
            }
            _ = tokio::time::sleep(timeout) => {
                local_abort_flag.store(true, AtomicOrdering::SeqCst);
                log::warn!(
                    "CPU transcription exceeded {:?}; requested Whisper abort",
                    timeout
                );
                wait_for_decode_to_stop(
                    &mut decode,
                    Duration::from_secs(CPU_TRANSCRIPTION_ABORT_GRACE_SECS),
                )
                .await?;
                Err(TRANSCRIPTION_TIMED_OUT.to_string())
            }
        }
    } else {
        tokio::select! {
            join_result = &mut decode => {
                join_result.map_err(|e| format!("Transcription task failed: {e}"))?
            }
            _ = wait_for_cancellation(cancel_flag.clone()) => {
                local_abort_flag.store(true, AtomicOrdering::SeqCst);
                wait_for_decode_to_stop(
                    &mut decode,
                    Duration::from_secs(CPU_TRANSCRIPTION_ABORT_GRACE_SECS),
                )
                .await?;
                Err("Transcription cancelled".to_string())
            }
        }
    };

    #[cfg(target_os = "windows")]
    {
        // If Vulkan failed or was skipped after a previous failure, keep that GPU status visible
        // instead of overwriting it with "CPU" after fallback transcription succeeds.
        if result.is_ok() && !preserve_gpu_status {
            let gpu_client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
            gpu_client
                .set_cpu_status(&mode, "Last transcription used CPU mode.")
                .await;
        }
    }

    result
}

/// Returns true if pill should be hidden, false if it should stay visible.
/// Called when transitioning to idle state (after recording ends).
/// - "never" → always hide (return true)
/// - "always" → never hide (return false)
/// - "when_recording" → hide when idle (return true)
///   Fails open: on error, returns true (default to when_recording behavior).
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
    use std::time::Duration;

    use super::{
        cpu_transcription_timeout, is_likely_silence, is_non_retryable_transcription_error,
        pill_toast_clear_may_hide_window, select_best_fallback_model,
        should_discard_likely_silence, should_hide_pill_when_idle,
        silence_state_allows_terminal_event, silence_state_allows_warning_event,
        NormalizedAudioStats, StopRecordingIntent, TOAST_ID_COUNTER, TRANSCRIPTION_TIMED_OUT,
    };
    use crate::RecordingState;
    use std::sync::atomic::Ordering;

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

    #[test]
    fn select_best_fallback_uses_priority_within_requested_family() {
        let available = vec!["large-v3".to_string(), "large-v3-q5_0".to_string()];
        let priority = vec!["large-v3-q5_0".to_string(), "large-v3".to_string()];

        assert_eq!(
            select_best_fallback_model(&available, "large-v3-turbo", &priority),
            "large-v3-q5_0"
        );
    }

    #[test]
    fn normalized_audio_silence_requires_low_rms_and_peak() {
        assert!(is_likely_silence(NormalizedAudioStats {
            duration_s: 2.0,
            rms: 0.0,
            peak: 0.0,
        }));
        assert!(!is_likely_silence(NormalizedAudioStats {
            duration_s: 2.0,
            rms: 0.01,
            peak: 0.02,
        }));
    }

    #[test]
    fn cpu_transcription_timeout_scales_with_floor_and_ceiling() {
        assert_eq!(cpu_transcription_timeout(4.0), Duration::from_secs(20));
        assert_eq!(cpu_transcription_timeout(10.0), Duration::from_secs(40));
        assert_eq!(cpu_transcription_timeout(20.0), Duration::from_secs(60));
        assert_eq!(cpu_transcription_timeout(f32::NAN), Duration::from_secs(20));
    }

    #[test]
    fn timeout_and_cancellation_do_not_retry() {
        assert!(is_non_retryable_transcription_error(
            TRANSCRIPTION_TIMED_OUT
        ));
        assert!(is_non_retryable_transcription_error(
            "Transcription cancelled"
        ));
        assert!(!is_non_retryable_transcription_error(
            "Whisper inference failed"
        ));
    }

    #[test]
    fn stop_in_flight_guard_blocks_duplicates_and_resets_on_drop() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let flag = Arc::new(AtomicBool::new(false));
        // First entry into stop_recording acquires the flag.
        assert!(!flag.swap(true, Ordering::SeqCst));
        {
            let _guard = super::StopInFlightGuard(flag.clone());
            // A concurrent duplicate entry sees the flag already set and bails out.
            assert!(flag.swap(true, Ordering::SeqCst));
        }
        // Dropping the guard clears the flag so the next stop can proceed.
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[test]
    fn stop_does_not_reset_idle_during_active_flow() {
        use super::stop_should_reset_to_idle;
        use crate::RecordingState;
        // Genuinely stuck/initial states should be reset to Idle.
        assert!(stop_should_reset_to_idle(RecordingState::Recording));
        assert!(stop_should_reset_to_idle(RecordingState::Idle));
        assert!(stop_should_reset_to_idle(RecordingState::Error));
        // An in-progress stop/transcribe flow must NOT be overridden.
        assert!(!stop_should_reset_to_idle(RecordingState::Stopping));
        assert!(!stop_should_reset_to_idle(RecordingState::Transcribing));
    }

    #[test]
    fn silence_terminal_with_speech_routes_to_stop_and_transcribe() {
        use crate::audio::silence_detector::SilenceDetectorEvent;
        assert!(SilenceDetectorEvent::TimeoutWithSpeech.is_terminal());
        assert!(!should_discard_likely_silence(
            StopRecordingIntent::LongSilenceWithSpeech
        ));
    }

    #[test]
    fn silence_terminal_without_speech_routes_to_cancel_and_discard() {
        use crate::audio::silence_detector::SilenceDetectorEvent;
        assert!(SilenceDetectorEvent::TimeoutNoSpeech.is_terminal());
        assert!(should_discard_likely_silence(StopRecordingIntent::Normal));
    }

    #[test]
    fn silence_warnings_have_no_terminal_action() {
        use crate::audio::silence_detector::SilenceDetectorEvent;
        assert!(!SilenceDetectorEvent::DeadMicWarn.is_terminal());
        assert!(!SilenceDetectorEvent::LongSilenceWarn.is_terminal());
        assert!(!SilenceDetectorEvent::Clear.is_terminal());
    }

    #[test]
    fn speech_timeout_never_discards_likely_silence_audio() {
        assert!(!should_discard_likely_silence(
            StopRecordingIntent::LongSilenceWithSpeech
        ));
    }

    #[test]
    fn normal_stop_still_discards_likely_silence_audio() {
        assert!(should_discard_likely_silence(StopRecordingIntent::Normal));
    }

    #[test]
    fn clear_pill_toast_only_clears_matching_current_toast_id() {
        // Simulate "toast id 7 is the current/newest toast".
        TOAST_ID_COUNTER.store(7, Ordering::SeqCst);
        // A stale (older) or not-yet-current id must NOT clear the window.
        assert!(!pill_toast_clear_may_hide_window(6));
        assert!(!pill_toast_clear_may_hide_window(8));
        // The current id clears exactly once and advances the counter.
        assert!(pill_toast_clear_may_hide_window(7));
        // After advancing, the same id no longer matches.
        assert!(!pill_toast_clear_may_hide_window(7));
    }

    #[test]
    fn silence_warning_events_only_apply_while_recording_or_starting() {
        assert!(silence_state_allows_warning_event(
            RecordingState::Recording
        ));
        assert!(silence_state_allows_warning_event(RecordingState::Starting));
        assert!(!silence_state_allows_warning_event(
            RecordingState::Stopping
        ));
    }

    #[test]
    fn silence_terminal_events_ignored_after_stop_flow_begins() {
        assert!(!silence_state_allows_terminal_event(
            RecordingState::Stopping
        ));
        assert!(!silence_state_allows_terminal_event(RecordingState::Idle));
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
                .unwrap_or_default(),
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

/// Select the best fallback model based on the provided priority order.
fn select_best_fallback_model(
    available_models: &[String],
    requested: &str,
    model_priority: &[String],
) -> String {
    if !requested.is_empty() {
        let requested_family = requested.split('-').next().unwrap_or(requested);
        for priority_model in model_priority {
            if available_models.contains(priority_model)
                && priority_model.starts_with(requested_family)
            {
                return priority_model.clone();
            }
        }
    }

    for priority_model in model_priority {
        if available_models.contains(priority_model) {
            return priority_model.clone();
        }
    }

    available_models.first().cloned().unwrap_or_else(|| {
        log::error!("No models available for fallback selection");
        "base.en".to_string()
    })
}

async fn should_prefer_cpu_whisper_models(app: &AppHandle) -> bool {
    #[cfg(not(target_os = "windows"))]
    let _ = app;
    #[cfg(target_os = "windows")]
    {
        let mode = get_settings(app.clone())
            .await
            .map(|settings| {
                normalize_transcription_acceleration(Some(&settings.transcription_acceleration))
            })
            .unwrap_or_else(|_| "auto".to_string());

        if mode == "cpu" {
            true
        } else if mode == "gpu" {
            false
        } else {
            let gpu_client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
            gpu_client.status().await.gpu_available == Some(false)
        }
    }

    #[cfg(target_os = "macos")]
    {
        std::env::consts::ARCH != "aarch64"
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        false
    }
}

#[derive(Clone, Copy, Debug)]
struct NormalizedAudioStats {
    duration_s: f32,
    rms: f64,
    peak: f64,
}

const SILENCE_RMS_THRESHOLD: f64 = 0.0005;
const SILENCE_PEAK_THRESHOLD: f64 = 0.003;

fn is_cancelled(cancel_flag: &AtomicBool) -> bool {
    cancel_flag.load(AtomicOrdering::SeqCst)
}

async fn wait_for_cancellation(cancel_flag: Arc<AtomicBool>) {
    while !is_cancelled(&cancel_flag) {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

fn is_likely_silence(stats: NormalizedAudioStats) -> bool {
    stats.rms < SILENCE_RMS_THRESHOLD && stats.peak < SILENCE_PEAK_THRESHOLD
}

fn inspect_normalized_wav(path: &Path) -> Result<NormalizedAudioStats, String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| format!("Failed to open normalized wav: {e}"))?;
    let spec = reader.spec();
    if spec.channels == 0 {
        return Err("Normalized wav has zero channels".to_string());
    }
    if spec.sample_format != hound::SampleFormat::Int || spec.bits_per_sample != 16 {
        return Err(format!(
            "Normalized wav must be 16-bit PCM, got {:?}/{} bits",
            spec.sample_format, spec.bits_per_sample
        ));
    }

    let frames = reader.duration();
    let duration_s = frames as f32 / spec.sample_rate as f32;
    let mut sum_squares = 0.0f64;
    let mut peak = 0.0f64;
    let mut samples = 0u64;

    for sample in reader.samples::<i16>() {
        let value =
            f64::from(sample.map_err(|e| format!("Failed to read normalized wav: {e}"))?) / 32768.0;
        let abs = value.abs();
        sum_squares += value * value;
        if abs > peak {
            peak = abs;
        }
        samples += 1;
    }

    let rms = if samples == 0 {
        0.0
    } else {
        (sum_squares / samples as f64).sqrt()
    };

    Ok(NormalizedAudioStats {
        duration_s,
        rms,
        peak,
    })
}

fn cached_license_allows_recording(cached: &CachedLicense) -> bool {
    match &cached.status.status {
        LicenseState::Licensed => true,
        LicenseState::Trial => {
            // Trial fallback is intentionally short-lived: it only covers
            // transient offline/timeout failures for a session that recently
            // proved it still had trial time remaining. Do not extend stale
            // trial evidence by the number of days remaining in the trial.
            let has_trial_time_remaining = cached
                .status
                .trial_days_left
                .and_then(|days| u64::try_from(days).ok())
                .is_some_and(|days| days > 0);

            has_trial_time_remaining
                && cached.age()
                    < std::time::Duration::from_secs(STALE_TRIAL_LICENSE_FALLBACK_MAX_AGE_SECS)
        }
        LicenseState::Expired | LicenseState::None => false,
    }
}

/// Pre-recording validation using the readiness state
async fn validate_recording_requirements(app: &AppHandle) -> Result<(), String> {
    let availability = crate::recognition_availability_snapshot(app).await;

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

    // Check license status (with caching to improve performance)
    let (license_status, timeout_fallback_status) = {
        let app_state = app.state::<AppState>();
        let cache = app_state.license_cache.read().await;

        if let Some(cached) = cache.as_ref() {
            if cached.is_valid() {
                log::debug!("Using cached license status (age: {:?})", cached.age());
                (Some(cached.status.clone()), None)
            } else {
                log::debug!(
                    "License cache is stale (age: {:?}), will refresh",
                    cached.age()
                );
                let fallback =
                    cached_license_allows_recording(cached).then(|| cached.status.clone());
                (None, fallback)
            }
        } else {
            log::debug!("No license cache found, will perform fresh check");
            (None, None)
        }
    };

    let status = if let Some(cached_status) = license_status {
        cached_status
    } else {
        // Cache miss or stale - perform license check with a bounded timeout.
        // Timeout fallback is only allowed when this session already has valid
        // local license evidence. Without that, fail closed instead of allowing
        // indefinite recording when the license service is slow or blocked.
        let check_result = tokio::time::timeout(
            std::time::Duration::from_secs(LICENSE_CHECK_TIMEOUT_SECS),
            check_license_status_internal(app),
        )
        .await;

        match check_result {
            Ok(Ok(fresh_status)) => {
                // Update cache
                let app_state = app.state::<AppState>();
                let mut cache = app_state.license_cache.write().await;
                *cache = Some(crate::commands::license::CachedLicense::new(
                    fresh_status.clone(),
                ));
                log::debug!("License status cached for 6 hours");
                fresh_status
            }
            Ok(Err(e)) => {
                log::error!("Failed to check license status: {}", e);
                if let Some(fallback_status) = timeout_fallback_status {
                    log::warn!(
                        "License check failed; using stale in-memory license status for this recording"
                    );
                    fallback_status
                } else {
                    return Err(e);
                }
            }
            Err(_) => {
                if let Some(fallback_status) = timeout_fallback_status {
                    log::warn!(
                        "License check timed out after {}s; using stale in-memory license status for this recording",
                        LICENSE_CHECK_TIMEOUT_SECS
                    );
                    fallback_status
                } else {
                    // Deliberate fail-closed path: without local license evidence,
                    // an offline or slow validation cannot start a new recording.
                    return Err(
                        "License validation timed out. Please check your connection and try again."
                            .to_string(),
                    );
                }
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

pub(crate) fn clear_pending_stop_after_start(app_state: &AppState) {
    app_state
        .pending_stop_after_start
        .store(false, std::sync::atomic::Ordering::SeqCst);
}

pub(crate) fn ptt_key_released(app_state: &AppState) -> bool {
    let mode = match app_state.recording_mode.lock() {
        Ok(guard) => *guard,
        Err(poisoned) => {
            log::warn!("recording_mode mutex poisoned; recovering value for PTT guard");
            *poisoned.into_inner()
        }
    };

    mode == RecordingMode::PushToTalk
        && !app_state
            .ptt_key_held
            .load(std::sync::atomic::Ordering::SeqCst)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopRecordingIntent {
    Normal,
    LongSilenceWithSpeech,
}

fn should_discard_likely_silence(intent: StopRecordingIntent) -> bool {
    matches!(intent, StopRecordingIntent::Normal)
}

fn silence_state_allows_warning_event(state: RecordingState) -> bool {
    matches!(state, RecordingState::Recording | RecordingState::Starting)
}

fn silence_state_allows_terminal_event(state: RecordingState) -> bool {
    matches!(state, RecordingState::Recording | RecordingState::Starting)
}

fn clear_active_silence_toast(app: &AppHandle, active_id: &mut Option<u64>) {
    if let Some(id) = active_id.take() {
        clear_pill_toast(app, id);
    }
}

fn spawn_silence_event_listener(
    app: AppHandle,
    silence_event_rx: std::sync::mpsc::Receiver<SilenceDetectorEvent>,
) {
    std::thread::spawn(move || {
        let mut active_silence_toast_id: Option<u64> = None;

        while let Ok(event) = silence_event_rx.recv() {
            let recording_state = crate::get_recording_state(&app);

            if event.is_terminal() {
                if !silence_state_allows_terminal_event(recording_state) {
                    clear_active_silence_toast(&app, &mut active_silence_toast_id);
                    break;
                }
            } else if !silence_state_allows_warning_event(recording_state) {
                continue;
            }

            match event {
                SilenceDetectorEvent::DeadMicWarn => {
                    active_silence_toast_id = Some(pill_toast_persistent(
                        &app,
                        "No audio detected — check your microphone",
                        PillToastVariant::Warning,
                    ));
                }
                SilenceDetectorEvent::LongSilenceWarn => {
                    active_silence_toast_id = Some(pill_toast_persistent(
                        &app,
                        "Long silence detected",
                        PillToastVariant::Warning,
                    ));
                }
                SilenceDetectorEvent::Clear => {
                    clear_active_silence_toast(&app, &mut active_silence_toast_id);
                }
                SilenceDetectorEvent::TimeoutWithSpeech => {
                    clear_active_silence_toast(&app, &mut active_silence_toast_id);
                    pill_toast_with_variant(
                        &app,
                        "Ended after long silence",
                        1500,
                        PillToastVariant::Info,
                    );
                    let app_for_stop = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let st = app_for_stop.state::<RecorderState>();
                        if let Err(e) =
                            stop_recording_after_long_silence(app_for_stop.clone(), st).await
                        {
                            log::error!("Long-silence stop failed: {}", e);
                        }
                    });
                    break;
                }
                SilenceDetectorEvent::TimeoutNoSpeech => {
                    clear_active_silence_toast(&app, &mut active_silence_toast_id);
                    let app_for_cancel = app.clone();
                    tauri::async_runtime::spawn(async move {
                        match cancel_recording(app_for_cancel.clone()).await {
                            Ok(()) => {
                                pill_toast_with_variant(
                                    &app_for_cancel,
                                    "No audio captured",
                                    1500,
                                    PillToastVariant::Warning,
                                );
                            }
                            Err(e) => {
                                log::error!("No-speech timeout cancel failed: {}", e);
                                pill_toast_with_variant(
                                    &app_for_cancel,
                                    "Recording error",
                                    1500,
                                    PillToastVariant::Warning,
                                );
                            }
                        }
                    });
                    break;
                }
            }
        }

        clear_active_silence_toast(&app, &mut active_silence_toast_id);
    });
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
                        validation_start.elapsed().as_millis().to_string().as_str(),
                    ),
                ],
            );
            return Err(e);
        }
    }

    if RECORDING_WHISPER_CPU_DECODE_IN_FLIGHT.load(AtomicOrdering::SeqCst) {
        log::warn!("Cannot start recording while previous CPU transcription is still stopping");
        return Err(PREVIOUS_TRANSCRIPTION_STOPPING.to_string());
    }

    // PTT guard: if recording mode is PushToTalk and the key was already released
    // while validation was running, abort now. This prevents recording from starting
    // after the user has already released the PTT key (e.g., during slow license checks).
    {
        let app_state = app.state::<AppState>();
        if ptt_key_released(&app_state) {
            log::info!("PTT: Key was released during validation; aborting recording start");
            return Err(PTT_START_ABORTED_AFTER_RELEASE.to_string());
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

    // Play sound on recording start if enabled
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
        }
    }

    // Pause system media if enabled (default: off)
    let mut resume_media_on_error = false;
    if let Ok(store) = app.store("settings") {
        let pause_media = store
            .get("pause_media_during_recording")
            .and_then(|v| v.as_bool())
            .unwrap_or(false); // Default to off
        if pause_media {
            log::info!("🎵 Pause media during recording is enabled");
            let paused = MEDIA_CONTROLLER.pause_if_playing();
            resume_media_on_error = paused;
            log::debug!("🎵 Media pause result: {}", paused);
        } else {
            log::debug!("🎵 Pause media during recording is disabled");
        }
    }

    let resume_media_if_needed = || {
        if resume_media_on_error {
            MEDIA_CONTROLLER.resume_if_we_paused();
        }
    };

    // Load recording config once to avoid repeated store access
    let config = match get_recording_config(&app).await {
        Ok(config) => config,
        Err(e) => {
            log::error!("Failed to load recording config: {}", e);
            resume_media_if_needed();
            return Err(format!("Configuration error: {}", e));
        }
    };
    log::debug!(
        "Using recording config: show_pill={} pill_indicator_mode='{}' ai_enabled={} model={}",
        config.show_pill_widget,
        config.pill_indicator_mode,
        config.ai_enabled,
        config.current_model
    );
    // Get app data directory for recordings
    let recordings_dir = match app.path().app_data_dir() {
        Ok(dir) => dir.join("recordings"),
        Err(e) => {
            resume_media_if_needed();
            return Err(e.to_string());
        }
    };

    // Ensure recordings directory exists
    if let Err(e) = std::fs::create_dir_all(&recordings_dir) {
        resume_media_if_needed();
        return Err(format!("Failed to create recordings directory: {}", e));
    }

    let timestamp = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(e) => {
            resume_media_if_needed();
            return Err(format!("Time error: {}", e));
        }
    };
    let audio_path = recordings_dir.join(format!("recording_{}.wav", timestamp));

    // Store path for later use and reset any leftover pending-toggle flag
    let app_state = app.state::<AppState>();
    clear_pending_stop_after_start(&app_state);

    // Save current recording path
    match app_state.current_recording_path.lock() {
        Ok(mut guard) => {
            guard.replace(audio_path.clone());
        }
        Err(e) => {
            resume_media_if_needed();
            return Err(format!("Failed to acquire path lock: {}", e));
        }
    }

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
        let mut recorder = match state.inner().0.lock() {
            Ok(recorder) => recorder,
            Err(e) => {
                resume_media_if_needed();
                return Err(format!("Failed to acquire recorder lock: {}", e));
            }
        };

        // Check if already recording
        if recorder.is_recording() {
            log::warn!("Already recording!");
            resume_media_if_needed();
            return Err("Already recording".to_string());
        }

        // Log the current audio device before starting
        log_start("AUDIO_DEVICE_CHECK");
        log_with_context(
            log::Level::Debug,
            "Checking audio device",
            &[("stage", "pre_recording")],
        );

        if let Ok(host) = std::panic::catch_unwind(cpal::default_host) {
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
        let audio_path_str = match audio_path.to_str() {
            Some(path) => path,
            None => {
                resume_media_if_needed();
                return Err("Invalid path encoding".to_string());
            }
        };

        log_file_operation("RECORDING_START", audio_path_str, false, None, None);

        // Start recording and take side-channel receivers before releasing the lock.
        let (audio_level_rx, silence_event_rx) =
            match recorder.start_recording(audio_path_str, selected_microphone.clone()) {
                Ok(_) => {
                    // Verify recording actually started
                    let is_recording = recorder.is_recording();

                    let rx = recorder.take_audio_level_receiver();
                    let silence_rx = recorder.take_silence_event_receiver();

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
                                    recorder_init_start
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

                        resume_media_if_needed();
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

                        (rx, silence_rx)
                    }
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
                                recorder_init_start
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

                    resume_media_if_needed();
                    return Err(e);
                }
            };

        // Release the recorder lock after successful start
        drop(recorder);

        if let Some(silence_event_rx) = silence_event_rx {
            spawn_silence_event_listener(app.clone(), silence_event_rx);
        }

        // Start audio level monitoring
        if let Some(audio_level_rx) = audio_level_rx {
            let app_for_levels = app.clone();
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

    // Second PTT guard: check again right before committing to Recording state.
    // Audio capture has already started; if PTT key was released between the first
    // guard (before Starting) and now (e.g., during audio device init), stop immediately.
    if ptt_key_released(&app_state) {
        log::info!("PTT: Key was released during audio init; stopping recorder immediately");
        // Stop the audio recorder synchronously before transitioning state.
        // If this fails, do not pretend the app is idle: propagate the
        // failure so the hotkey handler moves to Error and the recorder
        // remains visible for recovery instead of orphaning capture.
        let recorder_state_handle = app.state::<RecorderState>();
        let stop_result = recorder_state_handle
            .inner()
            .0
            .lock()
            .map_err(|e| format!("Failed to acquire recorder lock: {}", e))
            .and_then(|mut recorder| {
                if recorder.is_recording() {
                    recorder.stop_recording()
                } else {
                    Ok(String::new())
                }
            });

        clear_pending_stop_after_start(&app_state);
        MEDIA_CONTROLLER.resume_if_we_paused();

        let cleanup_recording_path = || {
            if let Ok(mut path_guard) = app_state.current_recording_path.lock() {
                if let Some(path) = path_guard.take() {
                    if let Err(error) = std::fs::remove_file(&path) {
                        log::warn!(
                            "Failed to remove aborted recording file {}: {}",
                            path.display(),
                            error
                        );
                    }
                }
            }
        };

        if let Err(error) = stop_result {
            cleanup_recording_path();
            return Err(error);
        }

        // Clean up the audio file
        cleanup_recording_path();

        update_recording_state(&app, RecordingState::Idle, None);
        return Err(PTT_START_ABORTED_AFTER_RELEASE.to_string());
    }

    // Update state to recording
    update_recording_state(&app, RecordingState::Recording, None);

    // If a stop was requested while starting (toggle or PTT), honor it immediately
    // after entering Recording state. For PTT, key-up in Starting state sets this flag.
    // The second PTT guard above handles key-up during audio init; this handles the
    // narrow window between Starting transition and this point.
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
            ("audio_path", format!("{:?}", audio_path).as_str()),
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
    match app.global_shortcut().register(escape_shortcut) {
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

async fn stop_recording_internal(
    app: AppHandle,
    state: State<'_, RecorderState>,
    intent: StopRecordingIntent,
) -> Result<String, String> {
    let stop_start = Instant::now();

    log_start("RECORDING_STOP");
    log_with_context(
        log::Level::Debug,
        "Stop recording command",
        &[
            ("command", "stop_recording"),
            ("timestamp", chrono::Utc::now().to_rfc3339().as_str()),
        ],
    );

    let app_state = app.state::<AppState>();
    if app_state.stop_in_flight.swap(true, AtomicOrdering::SeqCst) {
        log::debug!("stop_recording: a stop is already in flight; ignoring duplicate call");
        return Ok(String::new());
    }
    let _stop_guard = StopInFlightGuard(app_state.stop_in_flight.clone());
    // Capture the state BEFORE the Stopping write below, so the no-recorder
    // recovery path can distinguish "we entered while Recording" (reset to Idle)
    // from "another stop/transcribe flow already owns the state" (leave it alone).
    let entry_state = app_state.get_current_state();

    // Update state to stopping
    log_state_transition("RECORDING", "recording", "stopping", true, None);
    update_recording_state(&app, RecordingState::Stopping, None);
    // DO NOT request cancellation here - we want transcription to complete!
    // Cancellation should only happen in cancel_recording command

    // Stop recording (lock only within this scope to stay Send)
    let mut recorder_errored = false;
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
            if stop_should_reset_to_idle(entry_state) {
                update_recording_state(&app, RecordingState::Idle, None);
            } else {
                log::debug!(
                    "stop_recording: stop/transcribe already in progress (entry state={:?}); not overriding",
                    entry_state
                );
            }
            return Ok("".to_string());
        }

        let stop_message = match recorder.stop_recording() {
            Ok(msg) => msg,
            Err(e) => {
                log::error!("Recorder stop returned error: {}", e);
                recorder_errored = true;
                format!("Recorder stop error: {}", e)
            }
        };
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

        // Resume system media if we paused it
        MEDIA_CONTROLLER.resume_if_we_paused();

        // Monitor system resources after recording stop
        #[cfg(debug_assertions)]
        system_monitor::log_resources_after_operation(
            "RECORDING_STOP",
            stop_start.elapsed().as_millis() as u64,
        );
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

    // If the recorder thread itself errored (device/write failure), the
    // media-resume and ESC cleanup above have already run. Skip transcription
    // and reset to Idle instead of wedging or leaking paused media / ESC.
    if recorder_errored {
        pill_toast(&app, "Recording error", 1500);
        if should_hide_pill(&app).await {
            if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
                log::error!("Failed to hide pill window: {}", e);
            }
        }
        update_recording_state(&app, RecordingState::Idle, None);
        return Ok(String::new());
    }

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
                    downloaded_models.len().to_string().as_str(),
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
                    let prefer_cpu_models = should_prefer_cpu_whisper_models(&app).await;
                    let strategy = if prefer_cpu_models {
                        "cpu_preference"
                    } else {
                        "quality_preference"
                    };
                    let model_priority = {
                        let manager = whisper_manager.read().await;
                        if prefer_cpu_models {
                            manager.get_models_by_cpu_preference()
                        } else {
                            manager.get_models_by_quality_preference()
                        }
                    };
                    let fallback_model = select_best_fallback_model(
                        &downloaded_models,
                        &configured_model,
                        &model_priority,
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
                            ctx.insert("strategy".to_string(), strategy.to_string());
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
                let prefer_cpu_models = should_prefer_cpu_whisper_models(&app).await;
                let strategy = if prefer_cpu_models {
                    "cpu_preference"
                } else {
                    "quality_preference"
                };
                let model_priority = {
                    let manager = whisper_manager.read().await;
                    if prefer_cpu_models {
                        manager.get_models_by_cpu_preference()
                    } else {
                        manager.get_models_by_quality_preference()
                    }
                };
                let best_model =
                    select_best_fallback_model(&downloaded_models, "", &model_priority);

                log_model_operation(
                    "AUTO_SELECTION",
                    &best_model,
                    "SELECTED",
                    Some(&{
                        let mut ctx = std::collections::HashMap::new();
                        ctx.insert("reason".to_string(), "no_model_configured".to_string());
                        ctx.insert("strategy".to_string(), strategy.to_string());
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
    let (audio_path, recording_audio_duration_s) = match &engine_selection {
        ActiveEngineSelection::Soniox { .. } => {
            log::info!("[RECORD] Soniox selected — skipping normalization");
            (audio_path, None)
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

            let audio_stats = match inspect_normalized_wav(&normalized_path) {
                Ok(stats) => stats,
                Err(e) => {
                    log::error!("Audio inspection failed: {}", e);
                    update_recording_state(
                        &app,
                        RecordingState::Error,
                        Some("Audio inspection failed".to_string()),
                    );
                    let _ = std::fs::remove_file(&normalized_path);
                    return Err(e);
                }
            };

            log_with_context(
                log::Level::Info,
                "NORMALIZED_AUDIO",
                &[
                    ("path", format!("{:?}", normalized_path).as_str()),
                    ("sample_rate", "16000"),
                    ("channels", "1"),
                    ("bits", "16"),
                    (
                        "duration_s",
                        format!("{:.2}", audio_stats.duration_s).as_str(),
                    ),
                ],
            );
            log_audio_metrics(
                "NORMALIZED_AUDIO",
                audio_stats.rms,
                audio_stats.peak,
                audio_stats.duration_s,
                None,
            );

            if audio_stats.duration_s < min_duration_s_f32 {
                let _ = emit_to_window(
                    &app,
                    "pill",
                    "recording-too-short",
                    format!("Recording shorter than {} seconds", min_duration_label),
                );
                if let Err(e) = std::fs::remove_file(&normalized_path) {
                    log::debug!("Failed to remove short normalized audio: {}", e);
                }
                update_recording_state(&app, RecordingState::Idle, None);
                return Ok("".to_string());
            }

            if should_discard_likely_silence(intent) && is_likely_silence(audio_stats) {
                log::info!(
                    "Normalized audio is below speech threshold: rms={:.6}, peak={:.6}",
                    audio_stats.rms,
                    audio_stats.peak
                );
                pill_toast(&app, "No speech detected", 1500);
                let _ = app.emit(
                    "no-speech-detected",
                    serde_json::json!({
                        "severity": "warning",
                        "title": "No Speech Detected",
                        "message": "No speech was detected in the recording. Try speaking closer to the microphone.",
                    }),
                );
                if let Err(e) = std::fs::remove_file(&normalized_path) {
                    log::debug!("Failed to remove silent normalized audio: {}", e);
                }
                update_recording_state(&app, RecordingState::Idle, None);
                return Ok("".to_string());
            } else if !should_discard_likely_silence(intent) && is_likely_silence(audio_stats) {
                log::info!(
                    "Long-silence stop: keeping audio for transcription despite low metrics: rms={:.6}, peak={:.6}",
                    audio_stats.rms,
                    audio_stats.peak
                );
            }

            (normalized_path, Some(audio_stats.duration_s))
        }
    };

    log_with_context(
        log::Level::Debug,
        "Proceeding to transcription",
        &[
            ("audio_path", format!("{:?}", audio_path).as_str()),
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
    let transcription_timeout_for_task = recording_audio_duration_s.map(cpu_transcription_timeout);

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
        let cancel_flag = app_state.should_cancel_recording.clone();
        if app_state.is_cancellation_requested() {
            log::info!("Transcription cancelled before model loading");

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

        let transcription_result: Result<String, String> = match &engine_selection_for_task {
            ActiveEngineSelection::Whisper { model_path, .. } => {
                const MAX_RETRIES: u32 = 3;
                const RETRY_DELAY_MS: u64 = 500;

                let mut result = Err("No attempt made".to_string());

                for attempt in 1..=MAX_RETRIES {
                    if app_state.is_cancellation_requested() {
                        log::info!("Transcription cancelled at attempt {}", attempt);
                        result = Err("Transcription cancelled".to_string());
                        break;
                    }

                    result = transcribe_whisper_with_acceleration(
                        &app_for_task,
                        model_path,
                        &audio_path_clone,
                        language_for_task.as_deref(),
                        translate_to_english,
                        cancel_flag.clone(),
                        transcription_timeout_for_task,
                    )
                    .await;

                    match &result {
                        Ok(_) => {
                            if attempt > 1 {
                                log::info!("Transcription succeeded on attempt {}", attempt);
                            }
                            break;
                        }
                        Err(e) => {
                            if is_non_retryable_transcription_error(e) {
                                log::warn!(
                                    "Transcription attempt {} stopped without retry: {}",
                                    attempt,
                                    e
                                );
                                break;
                            }

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
                if text.trim().is_empty() || text == "[BLANK_AUDIO]" || text == "[SOUND]" {
                    log::info!("Whisper returned empty transcription - no speech detected");

                    // Emit graceful feedback to user via pill toast
                    pill_toast(&app_for_task, "No speech detected", 1500);

                    // Wait for feedback to show before hiding pill
                    let app_for_hide = app_for_task.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

                        // Hide pill window (only if show_pill_indicator is false)
                        if should_hide_pill(&app_for_hide).await {
                            if let Err(e) =
                                crate::commands::window::hide_pill_widget(app_for_hide.clone())
                                    .await
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

                    // Now handle text insertion or clipboard copy based on auto_paste_transcription.
                    // Missing setting keys default inside get_settings; actual settings-read failures fail closed
                    // to avoid surprising paste into the wrong app.
                    let auto_paste = match get_settings(app_for_process.clone()).await {
                        Ok(settings) => settings.auto_paste_transcription,
                        Err(error) => {
                            log::error!("Failed to read auto-paste setting: {}", error);
                            false
                        }
                    };

                    if auto_paste {
                        // Auto-paste enabled: insert text at cursor
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
                    } else {
                        // Auto-paste disabled: copy to clipboard and notify
                        match crate::commands::text::copy_text_to_clipboard(final_text.clone())
                            .await
                        {
                            Ok(_) => {
                                log::debug!("Text copied to clipboard (auto-paste disabled)");
                                pill_toast(&app_for_process, "Transcription copied", 1500);
                            }
                            Err(e) => {
                                log::error!("Failed to copy text to clipboard: {}", e);
                                pill_toast(&app_for_process, "Copy failed", 1500);
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
                                crate::commands::window::hide_pill_widget(app_for_reset.clone())
                                    .await
                            {
                                log::error!("Failed to hide pill window: {}", e);
                            }
                        }

                        update_recording_state(&app_for_reset, RecordingState::Idle, None);
                    });
                } else {
                    // For other errors, show error state briefly
                    update_recording_state(&app_for_task, RecordingState::Error, Some(e.clone()));

                    let toast_message = if e == TRANSCRIPTION_TIMED_OUT {
                        TRANSCRIPTION_TIMED_OUT
                    } else {
                        "Transcription failed"
                    };
                    pill_toast(&app_for_task, toast_message, 1500);

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
                                crate::commands::window::hide_pill_widget(app_for_reset.clone())
                                    .await
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

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: State<'_, RecorderState>,
) -> Result<String, String> {
    stop_recording_internal(app, state, StopRecordingIntent::Normal).await
}

async fn stop_recording_after_long_silence(
    app: AppHandle,
    state: State<'_, RecorderState>,
) -> Result<String, String> {
    stop_recording_internal(app, state, StopRecordingIntent::LongSilenceWithSpeech).await
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
            let result = transcribe_whisper_with_acceleration(
                &app,
                &model_path,
                &normalized_path,
                Some(&language),
                translate_to_english,
                Arc::new(AtomicBool::new(false)),
                None,
            )
            .await?;
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
            transcribe_whisper_with_acceleration(
                &app,
                &model_path,
                &temp_path,
                Some(language.as_str()),
                translate_to_english,
                Arc::new(AtomicBool::new(false)),
                None,
            )
            .await?
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

    // Do not abort the outer transcription task while guarded native CPU Whisper
    // is running. The task owns the per-transcription abort flag and will ask
    // whisper.cpp to stop cooperatively after observing `should_cancel_recording`.
    // Other engines/paths do not observe that flag and must keep the old abort path.
    if matches!(current_state, RecordingState::Transcribing)
        && RECORDING_WHISPER_CPU_DECODE_IN_FLIGHT.load(AtomicOrdering::SeqCst)
    {
        log::info!("CPU Whisper transcription task will observe cancellation cooperatively");
    } else if let Ok(mut task_guard) = app_state.transcription_task.lock() {
        if let Some(task) = task_guard.take() {
            log::info!("Aborting transcription task");
            task.abort();
        }
    }

    #[cfg(target_os = "windows")]
    if matches!(current_state, RecordingState::Transcribing) {
        let gpu_client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
        gpu_client.abort_active_process().await;
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

    // Resume system media if we paused it
    MEDIA_CONTROLLER.resume_if_we_paused();

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
