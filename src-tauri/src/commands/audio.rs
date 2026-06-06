use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::audio::recorder::AudioRecorder;
use crate::commands::settings::{
    get_settings, normalize_final_text_language, normalize_speech_language_for_model,
    normalize_transcription_task, recording_retention_days_from_store, resolve_pill_indicator_mode,
    task_uses_translate_to_english, Settings,
};
use crate::license::LicenseState;
use crate::media::MediaPauseController;
use crate::parakeet::messages::{ParakeetResponse, ParakeetSegment};
use crate::parakeet::ParakeetManager;
use crate::remote::client::{
    self, timeout_ms_for_wav_file, RemoteClientError, RemoteServerConnection,
    TranscriptionRequest as RemoteTranscriptionRequest, TranscriptionSource as RemoteTimeoutSource,
};
use crate::remote::settings::RemoteSettings;
use crate::transcription::{
    TranscriptionJob, TranscriptionResult, TranscriptionSegment, TranscriptionSource,
};
use crate::utils::logger::*;
#[cfg(debug_assertions)]
use crate::utils::system_monitor;
use crate::whisper::cache::TranscriberCache;
use crate::whisper::manager::WhisperManager;
use crate::whisper::transcriber::WhisperTranscriptionOutput;
use crate::{
    emit_to_all, emit_to_window, update_recording_state, AppState, RecordingMode, RecordingState,
};
use cpal::traits::{DeviceTrait, HostTrait};
use once_cell::sync::Lazy;
use serde_json;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tauri_plugin_store::StoreExt;
use uuid::Uuid;

pub(crate) const PTT_START_ABORTED_AFTER_RELEASE: &str =
    "PTT key released before recording could start";

/// Atomic counter for toast IDs to prevent race conditions
static TOAST_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Global media pause controller for pausing/resuming system media during recording
static MEDIA_CONTROLLER: Lazy<MediaPauseController> = Lazy::new(MediaPauseController::new);

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
    let id = TOAST_ID_COUNTER
        .fetch_add(1, AtomicOrdering::SeqCst)
        .wrapping_add(1);

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
        log::warn!(
            "pill_toast: toast window not found, message not shown: {}",
            message
        );
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

struct NormalizedTempFile {
    path: PathBuf,
}

impl NormalizedTempFile {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for NormalizedTempFile {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!(
                    "Failed to remove normalized temp file {:?}: {}",
                    self.path,
                    error
                );
            }
        }
    }
}

const RETRANSCRIPTION_SESSION_MARKER_FIELD: &str = "retranscription_session_marker";

const RETRANSCRIPTION_FAILURE_DETAIL_FIELD: &str = "failure_detail";

const STALE_RETRANSCRIPTION_FAILURE_TEXT: &str = "Retranscription interrupted before completion";

static RETRANSCRIPTION_SESSION_MARKER: Lazy<Uuid> = Lazy::new(Uuid::new_v4);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionStatus {
    InProgress,
    Completed,
    Failed,
}

impl TranscriptionStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "in_progress" => Some(Self::InProgress),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

fn current_retranscription_session_marker() -> String {
    RETRANSCRIPTION_SESSION_MARKER.to_string()
}

fn transcription_status_value(status: TranscriptionStatus) -> serde_json::Value {
    serde_json::Value::String(status.as_str().to_string())
}

fn normalize_transcription_status(status: Option<TranscriptionStatus>) -> TranscriptionStatus {
    status.unwrap_or(TranscriptionStatus::Completed)
}

fn parse_transcription_status(value: Option<&serde_json::Value>) -> Option<TranscriptionStatus> {
    value
        .and_then(serde_json::Value::as_str)
        .and_then(TranscriptionStatus::from_str)
}

fn should_replace_placeholder_text(text: Option<&str>) -> bool {
    text.map(|text| text.is_empty() || text == "In progress...")
        .unwrap_or(true)
}

fn apply_retranscription_status(
    map: &mut serde_json::Map<String, serde_json::Value>,
    status: Option<TranscriptionStatus>,
) -> TranscriptionStatus {
    let effective_status = normalize_transcription_status(status);
    map.insert(
        "status".to_string(),
        transcription_status_value(effective_status),
    );
    map.insert(
        "is_retranscription".to_string(),
        serde_json::Value::Bool(true),
    );

    match effective_status {
        TranscriptionStatus::InProgress => {
            map.insert(
                RETRANSCRIPTION_SESSION_MARKER_FIELD.to_string(),
                serde_json::Value::String(current_retranscription_session_marker()),
            );
        }
        TranscriptionStatus::Completed | TranscriptionStatus::Failed => {
            map.remove(RETRANSCRIPTION_SESSION_MARKER_FIELD);
        }
    }

    effective_status
}

fn sync_retranscription_failure_metadata(
    map: &mut serde_json::Map<String, serde_json::Value>,
    status: TranscriptionStatus,
    text: &str,
) {
    match status {
        TranscriptionStatus::Completed | TranscriptionStatus::InProgress => {
            map.remove("error_kind");
            map.remove("error_detail");
            map.remove("error_body");
            map.remove("can_retry_from_history");
        }
        TranscriptionStatus::Failed => {
            map.remove("error_kind");
            map.remove("error_body");
            map.insert(
                "error_detail".to_string(),
                serde_json::Value::String(text.to_string()),
            );
            if map.contains_key("recording_file") {
                map.insert(
                    "can_retry_from_history".to_string(),
                    serde_json::Value::Bool(true),
                );
            } else {
                map.remove("can_retry_from_history");
            }
        }
    }
}

pub(crate) fn reconcile_transcription_history_entry(
    entry: serde_json::Value,
    current_session_marker: &str,
) -> serde_json::Value {
    let Some(original) = entry.as_object() else {
        return entry;
    };

    let status = match original.get("status") {
        Some(status_value) => parse_transcription_status(Some(status_value)),
        None => None,
    };

    match status {
        None => {
            if original.contains_key("status") {
                return entry;
            }

            let mut reconciled = entry.clone();
            if let Some(map) = reconciled.as_object_mut() {
                map.insert(
                    "status".to_string(),
                    transcription_status_value(TranscriptionStatus::Completed),
                );
                map.remove(RETRANSCRIPTION_SESSION_MARKER_FIELD);
            }
            reconciled
        }
        Some(TranscriptionStatus::InProgress) => {
            let is_retranscription = original
                .get("is_retranscription")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
                || original
                    .get("source_recording_id")
                    .and_then(serde_json::Value::as_str)
                    .is_some();

            if !is_retranscription {
                return entry;
            }

            let stored_marker = original
                .get(RETRANSCRIPTION_SESSION_MARKER_FIELD)
                .and_then(serde_json::Value::as_str);

            if stored_marker == Some(current_session_marker) {
                return entry;
            }

            let mut reconciled = entry.clone();
            if let Some(map) = reconciled.as_object_mut() {
                if should_replace_placeholder_text(
                    map.get("text").and_then(serde_json::Value::as_str),
                ) {
                    map.insert(
                        "text".to_string(),
                        serde_json::Value::String(STALE_RETRANSCRIPTION_FAILURE_TEXT.to_string()),
                    );
                }
                map.insert(
                    "status".to_string(),
                    transcription_status_value(TranscriptionStatus::Failed),
                );
                map.remove(RETRANSCRIPTION_SESSION_MARKER_FIELD);
                map.insert(
                    RETRANSCRIPTION_FAILURE_DETAIL_FIELD.to_string(),
                    serde_json::json!({
                        "kind": "stale_retranscription_session",
                        "current_session_marker": current_session_marker,
                        "stale_session_marker": stored_marker,
                    }),
                );
            }
            reconciled
        }
        Some(TranscriptionStatus::Completed) | Some(TranscriptionStatus::Failed) => entry,
    }
}

#[derive(Debug, Clone)]
enum TranscriptionFailure {
    Local(String),
    Remote(RemoteClientError),
}

impl TranscriptionFailure {
    fn message(&self) -> String {
        match self {
            Self::Local(message) => message.clone(),
            Self::Remote(error) => error.to_string(),
        }
    }

    fn error_kind(&self) -> &'static str {
        match self {
            Self::Local(_) => "local",
            Self::Remote(error) => remote_client_error_kind(error),
        }
    }
}

fn remote_client_error_kind(error: &RemoteClientError) -> &'static str {
    match error {
        RemoteClientError::AuthFailed { .. } => "remote_auth_failed",
        RemoteClientError::Timeout { .. } => "remote_timeout",
        RemoteClientError::ConnectFailed { .. } => "remote_connect_failed",
        RemoteClientError::HttpStatus { .. } => "remote_http_status",
        RemoteClientError::ResponseDecode { .. } => "remote_response_decode",
        RemoteClientError::ResponseSchema { .. } => "remote_response_schema",
        RemoteClientError::RequestBuild { .. } => "remote_request_build",
        RemoteClientError::JoinFailed { .. } => "remote_join_failed",
    }
}

fn remote_server_error_pill_message(can_retry_from_history: bool) -> &'static str {
    if can_retry_from_history {
        "Remote transcription failed. Go to History to re-transcribe, or select a different model."
    } else {
        "Remote transcription failed. Check the remote server and try again."
    }
}

fn build_remote_server_error_payload(
    failure: &TranscriptionFailure,
    can_retry_from_history: bool,
) -> serde_json::Value {
    serde_json::json!({
        "title": "Remote Transcription Failed",
        "message": failure.message(),
        "error_kind": failure.error_kind(),
        "can_retry_from_history": can_retry_from_history,
    })
}

fn build_failed_transcription_row(
    error: &RemoteClientError,
    model: &str,
    recording_file: &str,
) -> serde_json::Value {
    serde_json::json!({
        "text": "Remote transcription failed - re-transcribe after resolving the issue",
        "model": model,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "recording_file": recording_file,
        "status": "failed",
        "error_kind": remote_client_error_kind(error),
        "error_detail": error.to_string(),
        "error_body": error.server_error_body(),
        "can_retry_from_history": true,
    })
}

fn build_transcription_job(
    source: TranscriptionSource,
    engine: impl Into<String>,
    model: impl Into<String>,
    spoken_language: Option<String>,
    translate_to_english: bool,
) -> TranscriptionJob {
    TranscriptionJob::from_legacy_settings(
        source,
        engine,
        model,
        spoken_language,
        translate_to_english,
    )
}

fn seconds_to_duration_ms(duration_seconds: Option<f32>) -> Option<u64> {
    duration_seconds.map(|seconds| (seconds.max(0.0) * 1000.0) as u64)
}

fn parakeet_segments_to_transcription_segments(
    segments: Vec<ParakeetSegment>,
) -> Vec<TranscriptionSegment> {
    segments
        .into_iter()
        .map(|segment| TranscriptionSegment {
            text: segment.text,
            start_ms: seconds_to_duration_ms(segment.start),
            end_ms: seconds_to_duration_ms(segment.end),
            speaker_id: None,
        })
        .collect()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UploadDiarizationSegment {
    pub speaker_id: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

fn build_remote_transcription_result(
    job: &TranscriptionJob,
    response: crate::remote::server::TranscribeResponse,
) -> TranscriptionResult {
    let mut result = TranscriptionResult::new(job, response.text)
        .with_processing_duration_ms(Some(response.duration_ms));
    result.model = response.model;
    result.transcript_language = response.transcript_language;
    result
}

fn build_writing_history_metadata(
    transcription: &TranscriptionResult,
    writing_result: &crate::writing::WritingResult,
) -> serde_json::Value {
    serde_json::json!({
        "mode": writing_result.mode,
        "output_language": writing_result.output_language,
        "transcript_language": transcription.transcript_language,
        "spoken_language": transcription.spoken_language,
        "ai_applied": writing_result.ai_applied,
        "applied_operations": writing_result.applied_operations,
        "warnings": writing_result.warnings,
        "context_hint": writing_result.context_hint,
    })
}

fn load_ai_enabled(app: &AppHandle) -> Result<bool, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    Ok(store
        .get("ai_enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false))
}

fn compile_whisper_initial_prompt(app: &AppHandle, language: Option<&str>) -> Option<String> {
    let settings = crate::writing::load_writing_settings(app).ok()?;
    crate::writing::compile_context_for_target(
        &settings,
        language,
        None,
        crate::writing::ProviderContextTarget::WhisperInitialPrompt,
    )
}

#[cfg(target_os = "windows")]
const DEFAULT_TRANSCRIPTION_ACCELERATION: &str = "auto";
#[cfg(target_os = "windows")]
fn normalize_transcription_acceleration(value: Option<&str>) -> String {
    match value {
        Some("cpu") => "cpu".to_string(),
        Some("gpu") => "gpu".to_string(),
        _ => DEFAULT_TRANSCRIPTION_ACCELERATION.to_string(),
    }
}
#[cfg(target_os = "windows")]
async fn transcription_acceleration_mode(app: &AppHandle) -> String {
    if let Ok(store) = app.store("settings") {
        let value = store
            .get("transcription_acceleration")
            .and_then(|v| v.as_str().map(str::to_owned));
        return normalize_transcription_acceleration(value.as_deref());
    }
    DEFAULT_TRANSCRIPTION_ACCELERATION.to_string()
}

/// Best-effort warm of the Windows Vulkan sidecar when a Whisper model is preloaded.
/// No-op on non-Windows platforms and when CPU acceleration is selected.
pub(crate) async fn warm_whisper_gpu_sidecar_on_model_preload(
    app: &AppHandle,
    model_path: &Path,
) -> bool {
    #[cfg(target_os = "windows")]
    {
        let mode = transcription_acceleration_mode(app).await;
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

async fn transcribe_whisper_with_acceleration<F>(
    app: &AppHandle,
    model_path: &Path,
    audio_path: &Path,
    language: Option<&str>,
    translate: bool,
    initial_prompt: Option<&str>,
    should_cancel: F,
) -> Result<WhisperTranscriptionOutput, String>
where
    F: Fn() -> bool,
{
    #[cfg(target_os = "windows")]
    let mode = transcription_acceleration_mode(app).await;

    #[cfg(target_os = "windows")]
    let mut preserve_gpu_status = false;

    #[cfg(target_os = "windows")]
    if mode != "cpu" {
        if should_cancel() {
            return Err("Transcription cancelled".to_string());
        }

        let gpu_client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
        let status = gpu_client.status().await;
        let should_try_gpu = mode == "gpu" || status.gpu_available != Some(false);

        if should_try_gpu {
            let gpu_result = gpu_client
                .transcribe(
                    app,
                    model_path,
                    audio_path,
                    language,
                    translate,
                    initial_prompt,
                    &mode,
                )
                .await;

            match gpu_result {
                Ok(output) => return Ok(output),
                Err(error) if error == "Transcription cancelled" => {
                    gpu_client.abort_active_process().await;
                    return Err(error);
                }
                Err(error) => {
                    preserve_gpu_status = true;
                    log::warn!("GPU sidecar failed, falling back to CPU: {error}");
                    if mode == "gpu" {
                        pill_toast(app, "GPU unavailable, using CPU", 4000);
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

    let result = transcriber.transcribe_with_metadata_with_prompt(
        audio_path,
        language,
        translate,
        initial_prompt,
        should_cancel,
    );

    #[cfg(target_os = "windows")]
    {
        if result.is_ok() && !preserve_gpu_status {
            let gpu_client = app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>();
            gpu_client
                .set_cpu_status(&mode, "Last transcription used CPU mode.")
                .await;
        }
    }

    result
}

fn build_remote_upload_transcription_request(
    audio_path: &Path,
    audio_data: Vec<u8>,
    job: Option<&TranscriptionJob>,
) -> (RemoteTranscriptionRequest, u64) {
    let audio_path = audio_path.to_string_lossy();
    let timeout_ms = timeout_ms_for_wav_file(audio_path.as_ref(), RemoteTimeoutSource::Upload);
    let request = RemoteTranscriptionRequest::new(audio_data, RemoteTimeoutSource::Upload)
        .with_language_and_task(
            job.and_then(|job| job.spoken_language.clone()),
            job.map(|job| transcription_task_header_value(job.task)),
        );

    (request, timeout_ms)
}

fn transcription_task_header_value(task: crate::transcription::TranscriptionTask) -> String {
    match task {
        crate::transcription::TranscriptionTask::Transcribe => "transcribe".to_string(),
        crate::transcription::TranscriptionTask::TranslateToEnglish => {
            "translate_to_english".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_failed_transcription_row, build_remote_server_error_payload,
        build_remote_transcription_result, build_remote_upload_transcription_request,
        build_soniox_create_payload, build_transcription_job, build_writing_history_metadata,
        recording_license_state, remote_server_error_pill_message, should_hide_pill_when_idle,
        should_use_active_remote, sync_retranscription_failure_metadata, NormalizedTempFile,
        RecordingLicenseState, TranscriptionFailure, TranscriptionStatus,
    };
    use crate::commands::license::CachedLicense;
    use crate::license::{LicenseState, LicenseStatus};
    use crate::remote::client::{
        calculate_timeout_ms, RemoteClientError, RemoteEndpoint, TranscriptionSource,
    };
    use crate::writing::{SonioxContext, SonioxContextField};
    use reqwest::StatusCode;
    use std::fs;

    fn cached_license(status: LicenseState) -> CachedLicense {
        CachedLicense::new(LicenseStatus {
            status,
            trial_days_left: None,
            license_type: None,
            license_key: None,
            expires_at: None,
        })
    }

    #[test]
    fn soniox_create_payload_includes_language_and_structured_context() {
        let payload = build_soniox_create_payload(
            "file_123",
            Some(" en "),
            Some(SonioxContext {
                general: vec![SonioxContextField {
                    key: "domain".to_string(),
                    value: "Software".to_string(),
                }],
                terms: vec!["VoiceTypr".to_string(), "Tauri".to_string()],
                text: Some(
                    "Spoken forms map to canonical spellings: voice typer -> VoiceTypr."
                        .to_string(),
                ),
            }),
        );

        assert_eq!(payload["model"].as_str(), Some("stt-async-v3"));
        assert_eq!(payload["file_id"].as_str(), Some("file_123"));
        assert_eq!(
            payload["language_hints"].as_array().unwrap()[0].as_str(),
            Some("en")
        );
        assert_eq!(
            payload["context"]["terms"].as_array().unwrap()[0].as_str(),
            Some("VoiceTypr")
        );
        assert_eq!(
            payload["context"]["text"].as_str(),
            Some("Spoken forms map to canonical spellings: voice typer -> VoiceTypr.")
        );
    }

    #[test]
    fn soniox_create_payload_omits_empty_optional_fields() {
        let payload = build_soniox_create_payload(
            "file_123",
            Some(" "),
            Some(SonioxContext {
                general: Vec::new(),
                terms: Vec::new(),
                text: None,
            }),
        );

        assert_eq!(payload["model"].as_str(), Some("stt-async-v3"));
        assert!(payload.get("language_hints").is_none());
        assert!(payload.get("context").is_none());
    }

    #[test]
    fn remote_upload_transcription_request_uses_upload_timeout_policy() {
        let audio_path = std::path::Path::new("missing-remote-upload.wav");
        let audio_data = vec![0x12, 0x34, 0x56];

        let (request, timeout_ms) =
            build_remote_upload_transcription_request(audio_path, audio_data.clone(), None);

        assert_eq!(request.audio_data, audio_data);
        assert_eq!(request.source, TranscriptionSource::Upload);
        assert_eq!(
            timeout_ms,
            calculate_timeout_ms(0, TranscriptionSource::Upload)
        );
    }

    #[test]
    fn remote_clipboard_transcription_request_uses_upload_timeout_policy() {
        let audio_path = std::path::Path::new("missing-remote-clipboard.wav");
        let audio_data = vec![0x9a, 0xbc, 0xde];

        let (request, timeout_ms) =
            build_remote_upload_transcription_request(audio_path, audio_data.clone(), None);

        assert_eq!(request.audio_data, audio_data);
        assert_eq!(request.source, TranscriptionSource::Upload);
        assert_eq!(
            timeout_ms,
            calculate_timeout_ms(0, TranscriptionSource::Upload)
        );
    }

    #[test]
    fn remote_upload_transcription_request_includes_language_and_task() {
        let audio_path = std::path::Path::new("sample.wav");
        let audio_data = vec![1, 2, 3];
        let job = build_transcription_job(
            crate::transcription::TranscriptionSource::AudioFile,
            "whisper",
            "base",
            Some("es".to_string()),
            true,
        );

        let (request, _) =
            build_remote_upload_transcription_request(audio_path, audio_data, Some(&job));

        assert_eq!(request.spoken_language.as_deref(), Some("es"));
        assert_eq!(
            request.transcription_task.as_deref(),
            Some("translate_to_english")
        );
    }

    #[test]
    fn remote_transcription_result_preserves_server_metadata() {
        let job = build_transcription_job(
            crate::transcription::TranscriptionSource::DesktopRecording,
            "remote",
            "remote-placeholder",
            Some("en".to_string()),
            false,
        );
        let result = build_remote_transcription_result(
            &job,
            crate::remote::server::TranscribeResponse {
                text: "hello world".to_string(),
                duration_ms: 1234,
                model: "base.en".to_string(),
                transcript_language: Some("en".to_string()),
            },
        );

        assert_eq!(result.raw_text, "hello world");
        assert_eq!(result.model, "base.en");
        assert_eq!(result.transcript_language.as_deref(), Some("en"));
        assert_eq!(result.timings.processing_duration_ms, Some(1234));
    }

    #[test]
    fn build_writing_history_metadata_uses_safe_fields_only() {
        let transcription = crate::transcription::TranscriptionResult::new(
            &build_transcription_job(
                crate::transcription::TranscriptionSource::DesktopRecording,
                "whisper",
                "base",
                Some("en".to_string()),
                false,
            ),
            "raw transcript",
        )
        .with_transcript_language(Some("en".to_string()));
        let writing_result = crate::writing::WritingResult {
            raw_text: "raw transcript".to_string(),
            final_text: "final transcript".to_string(),
            output_language: "en".to_string(),
            mode: crate::writing::WritingMode::CleanDictation,
            ai_applied: true,
            applied_operations: vec![crate::writing::AppliedWritingOperation {
                kind: crate::writing::WritingOperationKind::AiCleanup,
                detail: "Applied cleanup".to_string(),
            }],
            warnings: vec![],
            context_hint: None,
        };

        let metadata = build_writing_history_metadata(&transcription, &writing_result);
        assert_eq!(metadata["output_language"], "en");
        assert!(metadata.get("raw_text").is_none());
        assert!(metadata.get("final_text").is_none());
    }

    #[test]
    fn completed_retranscription_clears_stale_failure_metadata() {
        let mut map = serde_json::Map::new();
        map.insert(
            "recording_file".to_string(),
            serde_json::Value::String("sample.wav".to_string()),
        );
        map.insert(
            "error_kind".to_string(),
            serde_json::Value::String("remote_timeout".to_string()),
        );
        map.insert(
            "error_detail".to_string(),
            serde_json::Value::String("timed out".to_string()),
        );
        map.insert(
            "error_body".to_string(),
            serde_json::Value::String("body".to_string()),
        );
        map.insert(
            "can_retry_from_history".to_string(),
            serde_json::Value::Bool(true),
        );

        sync_retranscription_failure_metadata(&mut map, TranscriptionStatus::Completed, "done");

        assert!(!map.contains_key("error_kind"));
        assert!(!map.contains_key("error_detail"));
        assert!(!map.contains_key("error_body"));
        assert!(!map.contains_key("can_retry_from_history"));
    }

    #[test]
    fn failed_retranscription_rewrites_failure_metadata() {
        let mut map = serde_json::Map::new();
        map.insert(
            "recording_file".to_string(),
            serde_json::Value::String("sample.wav".to_string()),
        );
        map.insert(
            "error_kind".to_string(),
            serde_json::Value::String("remote_timeout".to_string()),
        );
        map.insert(
            "error_detail".to_string(),
            serde_json::Value::String("timed out".to_string()),
        );
        map.insert(
            "error_body".to_string(),
            serde_json::Value::String("body".to_string()),
        );

        sync_retranscription_failure_metadata(
            &mut map,
            TranscriptionStatus::Failed,
            "Re-transcription failed: Error: remote offline",
        );

        assert!(!map.contains_key("error_kind"));
        assert!(!map.contains_key("error_body"));
        assert_eq!(
            map.get("error_detail").and_then(serde_json::Value::as_str),
            Some("Re-transcription failed: Error: remote offline")
        );
        assert_eq!(
            map.get("can_retry_from_history")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
    }

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
    fn recording_license_state_is_loading_when_cache_absent() {
        assert_eq!(
            recording_license_state(None),
            RecordingLicenseState::Loading
        );
    }

    #[test]
    fn recording_license_state_blocks_expired_license() {
        let cached = cached_license(LicenseState::Expired);
        assert_eq!(
            recording_license_state(Some(&cached)),
            RecordingLicenseState::Blocked
        );
    }

    #[test]
    fn recording_license_state_blocks_missing_license() {
        let cached = cached_license(LicenseState::None);
        assert_eq!(
            recording_license_state(Some(&cached)),
            RecordingLicenseState::Blocked
        );
    }

    #[test]
    fn recording_license_state_allows_trial_and_licensed() {
        let trial = cached_license(LicenseState::Trial);
        let licensed = cached_license(LicenseState::Licensed);
        assert_eq!(
            recording_license_state(Some(&trial)),
            RecordingLicenseState::Ready
        );
        assert_eq!(
            recording_license_state(Some(&licensed)),
            RecordingLicenseState::Ready
        );
    }

    #[test]
    fn normalized_temp_file_removes_file_on_drop() {
        let path =
            std::env::temp_dir().join(format!("voicetypr-normalized-{}.wav", std::process::id()));
        fs::write(&path, b"temp audio").unwrap();

        {
            let temp_file = NormalizedTempFile::new(path.clone());
            assert!(temp_file.path().exists());
        }

        assert!(!path.exists());
    }

    #[test]
    fn explicit_engine_hint_bypasses_active_remote() {
        assert!(!should_use_active_remote(Some("whisper")));
        assert!(!should_use_active_remote(Some("parakeet")));
        assert!(!should_use_active_remote(Some("soniox")));
    }

    #[test]
    fn missing_engine_hint_allows_active_remote() {
        assert!(should_use_active_remote(None));
    }

    #[test]
    fn retryable_preserved_remote_failure_emits_history_capable_payload_and_copy() {
        let failure = TranscriptionFailure::Remote(RemoteClientError::Timeout {
            endpoint: RemoteEndpoint::Transcribe,
            timeout_ms: 120_000,
            detail: "timed out while waiting for response".to_string(),
        });
        let payload = build_remote_server_error_payload(&failure, true);

        assert_eq!(
            payload["title"].as_str().unwrap(),
            "Remote Transcription Failed"
        );
        assert!(payload["message"].as_str().unwrap().contains("timed out"));
        assert_eq!(payload["error_kind"].as_str().unwrap(), "remote_timeout");
        assert!(payload["can_retry_from_history"].as_bool().unwrap());
        assert!(remote_server_error_pill_message(true).contains("History"));
    }

    #[test]
    fn non_retryable_no_recording_remote_failure_omits_history_guidance() {
        let failure = TranscriptionFailure::Remote(RemoteClientError::ConnectFailed {
            endpoint: RemoteEndpoint::Transcribe,
            detail: "connection refused".to_string(),
        });
        let payload = build_remote_server_error_payload(&failure, false);

        assert_eq!(
            payload["title"].as_str().unwrap(),
            "Remote Transcription Failed"
        );
        assert!(payload["message"]
            .as_str()
            .unwrap()
            .contains("connection refused"));
        assert_eq!(
            payload["error_kind"].as_str().unwrap(),
            "remote_connect_failed"
        );
        assert!(!payload["can_retry_from_history"].as_bool().unwrap());
        assert!(!remote_server_error_pill_message(false).contains("History"));
    }

    #[test]
    fn failed_history_row_content_is_truthful_and_structured() {
        let row = build_failed_transcription_row(
            &RemoteClientError::HttpStatus {
                endpoint: RemoteEndpoint::Transcribe,
                status: StatusCode::BAD_GATEWAY,
                body: Some("upstream unavailable".to_string()),
            },
            "base.en",
            "recordings/failure.wav",
        );

        assert_eq!(row["status"].as_str().unwrap(), "failed");
        assert_eq!(row["error_kind"].as_str().unwrap(), "remote_http_status");
        assert_eq!(
            row["error_detail"].as_str().unwrap(),
            "Server error: 502 Bad Gateway"
        );
        assert_eq!(
            row["recording_file"].as_str().unwrap(),
            "recordings/failure.wav"
        );
        assert_eq!(row["model"].as_str().unwrap(), "base.en");
        assert_eq!(row["can_retry_from_history"].as_bool().unwrap(), true);
        assert_ne!(
            row["text"].as_str().unwrap(),
            "Remote server unreachable - re-transcribe to get text"
        );
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
    pub speech_language: String,
    pub transcription_task: String,
    pub final_text_language: String,
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

        let legacy_speech_language = store
            .get("language")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().speech_language.clone());
        let legacy_translate_to_english = store
            .get("translate_to_english")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let speech_language = store
            .get("speech_language")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or(legacy_speech_language);
        let stored_transcription_task = store
            .get("transcription_task")
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let transcription_task = normalize_transcription_task(
            stored_transcription_task.as_deref(),
            legacy_translate_to_english,
        );
        let stored_final_text_language = store
            .get("final_text_language")
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let final_text_language = normalize_final_text_language(
            stored_final_text_language.as_deref(),
            &transcription_task,
        );

        let config = Self {
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
            speech_language,
            transcription_task,
            final_text_language,
            show_recording_status: store
                .get("show_recording_status")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            loaded_at: Instant::now(),
        };
        let mut config = config;
        config.speech_language = normalize_speech_language_for_model(
            &config.current_engine,
            &config.current_model,
            &config.speech_language,
        );
        Ok(config)
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
async fn save_recording_internal(
    app: &AppHandle,
    audio_path: &Path,
    check_settings: bool,
) -> Option<String> {
    // Get settings store for retention policy and save_recordings check.
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

            // Cleanup old recordings by retention period.
            let retention_days = recording_retention_days_from_store(&store);

            if let Some(days) = retention_days {
                cleanup_old_recordings(&recordings_dir, days);
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

/// Clean up recordings older than the retention period.
fn cleanup_old_recordings(recordings_dir: &Path, retention_days: u32) {
    let cutoff = match std::time::SystemTime::now().checked_sub(std::time::Duration::from_secs(
        u64::from(retention_days) * 24 * 60 * 60,
    )) {
        Some(cutoff) => cutoff,
        None => return,
    };

    let recordings = match std::fs::read_dir(recordings_dir) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!("Failed to read recordings directory for cleanup: {}", e);
            return;
        }
    };

    for entry in recordings.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        let is_wav = path.extension().map(|ext| ext == "wav").unwrap_or(false);

        if !is_wav {
            continue;
        }

        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified().or_else(|_| metadata.created()));

        let Ok(modified) = modified else {
            continue;
        };

        if modified >= cutoff {
            continue;
        }

        if let Err(e) = std::fs::remove_file(&path) {
            log::warn!("Failed to remove old recording {:?}: {}", path, e);
        } else {
            log::info!("Cleaned up old recording: {:?}", path);
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

fn should_use_active_remote(engine_hint: Option<&str>) -> bool {
    engine_hint.is_none()
}

async fn resolve_engine_for_model(
    app: &AppHandle,
    model_name: &str,
    engine_hint: Option<&str>,
) -> Result<ActiveEngineSelection, String> {
    let remote_settings = app.state::<AsyncMutex<RemoteSettings>>();
    let active_remote = {
        let settings = remote_settings.lock().await;
        settings.get_active_connection().cloned()
    };

    if should_use_active_remote(engine_hint) {
        if let Some(remote_conn) = active_remote {
            if matches!(
                remote_conn.status,
                crate::remote::settings::ConnectionStatus::Online
            ) {
                return Ok(ActiveEngineSelection::Remote {
                    server_name: remote_conn.display_name(),
                    host: remote_conn.host,
                    port: remote_conn.port,
                    password: remote_conn.password,
                });
            }

            return Err(
                "Selected remote unavailable. Reconnect or choose another source.".to_string(),
            );
        }
    }

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
            if model.starts_with(requested.split('-').next().unwrap_or(requested)) {
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
    available_models.first().cloned().unwrap_or_else(|| {
        log::error!("No models available for fallback selection");
        // This should never happen as we check for empty models before calling this function
        // But return a default to prevent panic
        "base.en".to_string()
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordingLicenseState {
    Ready,
    Loading,
    Blocked,
}

fn recording_license_state(
    cache: Option<&crate::commands::license::CachedLicense>,
) -> RecordingLicenseState {
    match cache {
        Some(cached)
            if matches!(
                cached.status.status,
                LicenseState::Expired | LicenseState::None
            ) =>
        {
            RecordingLicenseState::Blocked
        }
        Some(_) => RecordingLicenseState::Ready,
        None => RecordingLicenseState::Loading,
    }
}
/// Pre-recording validation using the readiness state
async fn validate_recording_requirements(app: &AppHandle) -> Result<(), String> {
    let validate_start = std::time::Instant::now();
    log::info!("⏱️ [VALIDATE] starting recognition_availability_snapshot");
    let availability = crate::recognition_availability_snapshot(app).await;
    log::info!(
        "⏱️ [VALIDATE] recognition_availability_snapshot complete (+{}ms)",
        validate_start.elapsed().as_millis()
    );

    if !availability.any_available()
        || (availability.remote_selected && !availability.remote_available)
    {
        log::error!("No usable speech recognition engines are ready");
        let (title, message, error_text) =
            if availability.remote_selected && !availability.remote_available {
                (
                    "Selected Remote Unavailable",
                    "Selected remote unavailable. Reconnect or choose another source.",
                    "Selected remote unavailable. Reconnect or choose another source.".to_string(),
                )
            } else if availability.soniox_selected && !availability.soniox_ready {
                (
                    "No Speech Recognition Sources",
                    "Please configure your Soniox token in Models before recording.",
                    "Soniox token missing".to_string(),
                )
            } else {
                (
                "No Speech Recognition Sources",
                "Connect a cloud provider or download a local model in Models before recording.",
                "No speech recognition sources available. Please configure a source first."
                    .to_string(),
            )
            };
        let _ = emit_to_window(
            app,
            "main",
            "no-models-error",
            serde_json::json!({
                "title": title,
                "message": message,
                "action": "open-settings"
            }),
        );
        return Err(error_text);
    }

    // Check cached license status (warmed during startup/license transitions - no network call)
    let app_state = app.state::<AppState>();
    let cache = app_state.license_cache.read().await;

    match recording_license_state(cache.as_ref()) {
        RecordingLicenseState::Blocked => {
            if let Some(cached) = cache.as_ref() {
                log::warn!("Recording blocked: license is {:?}", cached.status.status);
            }

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }

            let _ = emit_to_all(
                app,
                "license-required",
                serde_json::json!({
                    "title": "License Required",
                    "message": "Your trial has expired. Please purchase a license to continue",
                    "action": "purchase"
                }),
            );
            return Err("License required to record".to_string());
        }
        RecordingLicenseState::Ready => {}
        RecordingLicenseState::Loading => {
            log::warn!("Recording blocked: license cache not initialized yet");
            let _ = emit_to_window(
                app,
                "main",
                "license-loading",
                serde_json::json!({
                    "title": "Checking License",
                    "message": "License status is still loading. Please try again in a moment.",
                    "action": "wait"
                }),
            );
            return Err(
                "License status is still loading. Please try again in a moment.".to_string(),
            );
        }
    }

    log::info!(
        "⏱️ [VALIDATE] validation complete (+{}ms)",
        validate_start.elapsed().as_millis()
    );
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
    log::info!(
        "⏱️ [REC TIMING] state check complete (+{}ms)",
        recording_start.elapsed().as_millis()
    );

    // Validate all requirements upfront
    let validation_start = Instant::now();
    log::info!(
        "⏱️ [REC TIMING] starting validation (+{}ms)",
        recording_start.elapsed().as_millis()
    );
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
    log::info!(
        "⏱️ [REC TIMING] validation complete (+{}ms)",
        recording_start.elapsed().as_millis()
    );
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
    log::info!(
        "⏱️ [REC TIMING] about to play sound (+{}ms)",
        recording_start.elapsed().as_millis()
    );
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
            log::info!(
                "⏱️ [REC TIMING] sound played + 300ms delay (+{}ms)",
                recording_start.elapsed().as_millis()
            );
        }
    }

    // Pause system media if enabled (default: true)
    let mut resume_media_on_error = false;
    if let Ok(store) = app.store("settings") {
        let pause_media = store
            .get("pause_media_during_recording")
            .and_then(|v| v.as_bool())
            .unwrap_or(true); // Default to true
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
    log::info!(
        "⏱️ [REC TIMING] loading recording config (+{}ms)",
        recording_start.elapsed().as_millis()
    );
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
    log::info!(
        "⏱️ [REC TIMING] getting microphone settings (+{}ms)",
        recording_start.elapsed().as_millis()
    );
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
    log::info!(
        "⏱️ [REC TIMING] acquiring recorder lock (+{}ms)",
        recording_start.elapsed().as_millis()
    );
    {
        let mut recorder = match state.inner().0.lock() {
            Ok(recorder) => recorder,
            Err(e) => {
                resume_media_if_needed();
                return Err(format!("Failed to acquire recorder lock: {}", e));
            }
        };
        log::info!(
            "⏱️ [REC TIMING] recorder lock acquired (+{}ms)",
            recording_start.elapsed().as_millis()
        );

        // Check if already recording
        if recorder.is_recording() {
            log::warn!("Already recording!");
            resume_media_if_needed();
            return Err("Already recording".to_string());
        }

        // Log the current audio device before starting
        log::info!(
            "⏱️ [REC TIMING] checking audio device (+{}ms)",
            recording_start.elapsed().as_millis()
        );
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
        log::info!(
            "⏱️ [REC TIMING] about to call recorder.start_recording (+{}ms)",
            recording_start.elapsed().as_millis()
        );
        let recorder_init_start = Instant::now();
        let audio_path_str = match audio_path.to_str() {
            Some(path) => path,
            None => {
                resume_media_if_needed();
                return Err("Invalid path encoding".to_string());
            }
        };

        log_file_operation("RECORDING_START", audio_path_str, false, None, None);

        // Start recording and get audio level receiver
        let audio_level_rx =
            match recorder.start_recording(audio_path_str, selected_microphone.clone()) {
                Ok(_) => {
                    log::info!(
                        "⏱️ [REC TIMING] recorder.start_recording returned Ok (+{}ms)",
                        recording_start.elapsed().as_millis()
                    );
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
            ("timestamp", chrono::Utc::now().to_rfc3339().as_str()),
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

        // Resume system media if we paused it
        MEDIA_CONTROLLER.resume_if_we_paused();

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
        log::info!(
            "🔍 [REMOTE DEBUG] get_active_connection returned: {:?}",
            conn.as_ref().map(|c| &c.id)
        );
        conn
    };

    log::info!(
        "🔍 [REMOTE DEBUG] active_remote is_some={}",
        active_remote.is_some()
    );

    let engine_selection = if let Some(remote_conn) = active_remote {
        if matches!(
            remote_conn.status,
            crate::remote::settings::ConnectionStatus::Online
        ) {
            log::info!(
                "🌐 Using remote server for transcription: {} ({}:{})",
                remote_conn.display_name(),
                remote_conn.host,
                remote_conn.port
            );
            ActiveEngineSelection::Remote {
                server_name: remote_conn.display_name(),
                host: remote_conn.host,
                port: remote_conn.port,
                password: remote_conn.password,
            }
        } else {
            return abort_due_to_missing_model(
                &app,
                &audio_path,
                "Selected remote unavailable",
                "Selected remote unavailable. Reconnect or choose another source.",
            )
            .await;
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
            log::info!(
                "[RECORD] Remote server '{}' selected — skipping normalization",
                server_name
            );
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
                        ("path", format!("{:?}", normalized_path).as_str()),
                        ("sample_rate", spec.sample_rate.to_string().as_str()),
                        ("channels", spec.channels.to_string().as_str()),
                        ("bits", spec.bits_per_sample.to_string().as_str()),
                        ("duration_s", format!("{:.2}", duration).as_str()),
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
            ("audio_path", format!("{:?}", audio_path).as_str()),
            ("stage", "pre_transcription"),
        ],
    );
    log::debug!(
        "Using cached config: model={}, speech_language={}, transcription_task={}, final_text_language={}, ai_enabled={}",
        config.current_model,
        config.speech_language,
        config.transcription_task,
        config.final_text_language,
        config.ai_enabled
    );

    let language = if config.speech_language.is_empty() {
        None
    } else {
        Some(normalize_speech_language_for_model(
            engine_selection.engine_name(),
            engine_selection.model_name(),
            &config.speech_language,
        ))
    };
    let translate_to_english = task_uses_translate_to_english(&config.transcription_task);

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

    let transcription_job = build_transcription_job(
        TranscriptionSource::DesktopRecording,
        engine_label.clone(),
        selected_model_name.clone(),
        language.clone(),
        translate_to_english,
    );
    let audio_path_clone = audio_path.clone();
    let engine_selection_for_task = engine_selection;
    let language_for_task = language.clone();
    let selected_model_name_for_task = selected_model_name.clone();
    let transcription_job_for_task = transcription_job.clone();
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
                if let Err(e) =
                    crate::commands::window::hide_pill_widget(app_for_task.clone()).await
                {
                    log::error!("Failed to hide pill window on cancellation: {}", e);
                }
            }

            update_recording_state(&app_for_task, RecordingState::Idle, None);
            return;
        }

        let transcription_result: Result<TranscriptionResult, TranscriptionFailure> =
            match &engine_selection_for_task {
                ActiveEngineSelection::Whisper { model_path, .. } => {
                    let initial_prompt =
                        compile_whisper_initial_prompt(&app_for_task, language_for_task.as_deref());

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
                            initial_prompt.as_deref(),
                            || app_state.is_cancellation_requested(),
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
                                if e == "Transcription cancelled" {
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
                        .map(|output| {
                            TranscriptionResult::new(&transcription_job_for_task, output.raw_text)
                                .with_transcript_language(output.transcript_language)
                                .with_segments(output.segments)
                                .with_audio_duration_ms(Some(output.audio_duration_ms))
                                .with_processing_duration_ms(Some(output.processing_duration_ms))
                        })
                        .map_err(TranscriptionFailure::Local)
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
                        Ok(ParakeetResponse::Transcription {
                            text,
                            segments,
                            language,
                            duration,
                        }) => Ok(TranscriptionResult::new(&transcription_job_for_task, text)
                            .with_transcript_language(language)
                            .with_segments(parakeet_segments_to_transcription_segments(segments))
                            .with_audio_duration_ms(seconds_to_duration_ms(duration))),
                        Ok(other) => {
                            let message = format!("Unexpected Parakeet response: {:?}", other);
                            Err(TranscriptionFailure::Local(message))
                        }
                        Err(e) => Err(TranscriptionFailure::Local(e.to_string())),
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
                        Ok(text) => {
                            let soniox_job = build_transcription_job(
                                TranscriptionSource::DesktopRecording,
                                transcription_job_for_task.engine.clone(),
                                transcription_job_for_task.model.clone(),
                                transcription_job_for_task.spoken_language.clone(),
                                false,
                            );
                            Ok(TranscriptionResult::new(&soniox_job, text))
                        }
                        Err(e) => Err(TranscriptionFailure::Local(e)),
                    }
                }
                ActiveEngineSelection::Remote {
                    server_name,
                    host,
                    port,
                    password,
                    ..
                } => {
                    async {
                        let remote_start = std::time::Instant::now();
                        log::info!(
                            "🌐 [Remote] Starting transcription to '{}' ({}:{})",
                            server_name,
                            host,
                            port
                        );

                        let audio_data = std::fs::read(&audio_path_clone).map_err(|e| {
                            TranscriptionFailure::Local(format!("Failed to read audio file: {}", e))
                        })?;

                        let audio_size_kb = audio_data.len() as f64 / 1024.0;
                        log::info!(
                            "🌐 [Remote] Sending {:.1} KB audio to '{}' (+{}ms)",
                            audio_size_kb,
                            server_name,
                            remote_start.elapsed().as_millis()
                        );

                        let server_conn =
                            RemoteServerConnection::new(host.clone(), *port, password.clone());

                        let request = RemoteTranscriptionRequest::new(
                            audio_data,
                            RemoteTimeoutSource::LiveRecording,
                        )
                        .with_language_and_task(
                            transcription_job_for_task.spoken_language.clone(),
                            Some(transcription_task_header_value(
                                transcription_job_for_task.task,
                            )),
                        );
                        let timeout_ms = timeout_ms_for_wav_file(
                            audio_path_clone.to_string_lossy().as_ref(),
                            RemoteTimeoutSource::LiveRecording,
                        );
                        match client::transcribe_audio(&server_conn, request, timeout_ms).await {
                            Ok(response) => {
                                log::info!(
                                "🌐 [Remote] Transcription COMPLETED from '{}': {} chars received",
                                server_name,
                                response.text.len()
                            );
                                Ok(build_remote_transcription_result(
                                    &transcription_job_for_task,
                                    response,
                                ))
                            }
                            Err(error) => {
                                log::warn!(
                                "🌐 [Remote] Remote transcription FAILED to '{}' after {}ms: {}",
                                server_name,
                                remote_start.elapsed().as_millis(),
                                error
                            );
                                Err(TranscriptionFailure::Remote(error))
                            }
                        }
                    }
                    .await
                }
            };

        // Try to save recording to persistent storage BEFORE cleanup
        // On success: use maybe_save_recording (respects save_recordings setting)
        // On recoverable failure (remote server errors): always save to preserve for re-transcription
        let recording_file = match &transcription_result {
            Ok(_) => maybe_save_recording(&app_for_task, &audio_path_clone).await,
            Err(TranscriptionFailure::Remote(_)) => {
                log::info!("Preserving recording for failed remote transcription");
                save_recording(&app_for_task, &audio_path_clone).await
            }
            Err(TranscriptionFailure::Local(_)) => None,
        };

        // Clean up temp file regardless of outcome
        if let Err(e) = std::fs::remove_file(&audio_path_clone) {
            log::warn!("Failed to remove temporary audio file: {}", e);
        }

        match transcription_result {
            Ok(transcription) => {
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

                log::debug!(
                    "Transcription successful, {} chars",
                    transcription.raw_text.len()
                );

                // Check if transcription is empty or just noise
                if transcription.raw_text.is_empty()
                    || transcription.raw_text.trim().is_empty()
                    || transcription.raw_text == "[BLANK_AUDIO]"
                {
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
                let text_for_process = transcription.raw_text.clone();
                let model_for_process = transcription.model.clone();
                let transcription_for_process = transcription.clone();
                let ai_enabled_for_task = ai_enabled; // Capture from cached config
                let recording_file_for_task = recording_file.clone(); // Capture recording file

                tokio::spawn(async move {
                    // 1. Process the transcription and enhancement
                    let (final_text, writing_metadata) =
                        match crate::writing::process_transcription(
                            app_for_process.clone(),
                            transcription_for_process.clone(),
                            ai_enabled_for_task,
                        )
                        .await
                        {
                            Ok(writing_result) => {
                                if ai_enabled_for_task {
                                    // Emit enhancing completed event (global)
                                    let _ = app_for_process.emit("enhancing-completed", ());
                                }

                                if writing_result.ai_applied {
                                    log::info!("AI enhancement applied successfully");
                                } else if !ai_enabled_for_task {
                                    log::debug!("AI enhancement is disabled, using original text");
                                }
                                (
                                    writing_result.final_text.clone(),
                                    Some(build_writing_history_metadata(
                                        &transcription_for_process,
                                        &writing_result,
                                    )),
                                )
                            }
                            Err(e) => {
                                log::warn!("Formatting failed, using original text: {}", e);

                                if ai_enabled_for_task {
                                    // Emit enhancing failed to reset pill state
                                    let _ = app_for_process.emit("enhancing-failed", ());
                                }

                                // Check error type and create appropriate message
                                let error_message = e.to_string();
                                let user_message =
                                    if error_message.contains("Final output language") {
                                        "Final output language requires AI enhancement"
                                    } else if error_message.contains("400")
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

                                (text_for_process.clone(), None)
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
                    let recording_file_for_history = recording_file_for_task.clone();
                    let writing_metadata_for_history = writing_metadata.clone();
                    tokio::spawn(async move {
                        match save_transcription_with_recording(
                            app_for_history.clone(),
                            history_text,
                            history_model,
                            recording_file_for_history,
                            writing_metadata_for_history,
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
            Err(failure) => {
                match &failure {
                    TranscriptionFailure::Local(e) if e.contains("cancelled") => {
                        log::info!("Handling transcription cancellation");
                        // For cancellation, hide pill (only if show_pill_indicator is false) and go to Idle
                        if should_hide_pill(&app_for_task).await {
                            if let Err(hide_err) =
                                crate::commands::window::hide_pill_widget(app_for_task.clone())
                                    .await
                            {
                                log::error!(
                                    "Failed to hide pill window on cancellation: {}",
                                    hide_err
                                );
                            }
                        }
                        update_recording_state(&app_for_task, RecordingState::Idle, None);
                    }
                    TranscriptionFailure::Local(e) if e.contains("too short") => {
                        // Handle "too short" errors with specific user feedback
                        log::info!("Recording was too short: {}", e);

                        // Clean up the audio file
                        if let Err(cleanup_err) = std::fs::remove_file(&audio_path_clone) {
                            log::warn!("Failed to remove short audio file: {}", cleanup_err);
                        }

                        // Emit specific feedback via pill toast
                        pill_toast(&app_for_task, e, 1000);

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
                    }
                    TranscriptionFailure::Remote(remote_error) => {
                        // Remote server error - emit specific event for system notification
                        log::warn!("Remote server error: {}", remote_error);

                        let can_retry_from_history =
                            if let Some(ref saved_recording) = recording_file {
                                let app_for_history = app_for_task.clone();
                                let model_name = selected_model_name_for_task.clone();
                                let recording_filename = saved_recording.clone();
                                match save_failed_transcription(
                                    &app_for_history,
                                    remote_error,
                                    model_name,
                                    recording_filename,
                                )
                                .await
                                {
                                    Ok(_) => true,
                                    Err(save_err) => {
                                        log::error!(
                                            "Failed to save failed transcription: {}",
                                            save_err
                                        );
                                        false
                                    }
                                }
                            } else {
                                false
                            };

                        // Emit event for frontend to show system notification with guidance
                        let _ = app_for_task.emit(
                            "remote-server-error",
                            build_remote_server_error_payload(&failure, can_retry_from_history),
                        );

                        // Update pill message to guide user to History only when retry is durable
                        pill_toast(
                            &app_for_task,
                            remote_server_error_pill_message(can_retry_from_history),
                            if can_retry_from_history { 6000 } else { 2000 },
                        );

                        update_recording_state(
                            &app_for_task,
                            RecordingState::Error,
                            Some(failure.message()),
                        );

                        // Transition back to Idle after showing the error
                        let app_for_reset = app_for_task.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
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
                    TranscriptionFailure::Local(e) => {
                        // For other errors, show error state briefly
                        update_recording_state(
                            &app_for_task,
                            RecordingState::Error,
                            Some(e.clone()),
                        );

                        // Emit error via pill toast
                        pill_toast(&app_for_task, e, 1500);

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
    save_transcription_with_recording(app, text, model, None, None).await
}

/// Save transcription to history with optional recording file reference
pub async fn save_transcription_with_recording(
    app: AppHandle,
    text: String,
    model: String,
    recording_file: Option<String>,
    writing_metadata: Option<serde_json::Value>,
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
    if let Some(metadata) = writing_metadata {
        transcription_data["writing"] = metadata;
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
    error: &RemoteClientError,
    model: String,
    recording_file: String,
) -> Result<(), String> {
    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;

    let transcription_data = build_failed_transcription_row(error, &model, &recording_file);
    let timestamp = transcription_data["timestamp"]
        .as_str()
        .ok_or_else(|| "Failed to build failed transcription timestamp".to_string())?
        .to_string();

    store.set(&timestamp, transcription_data.clone());

    store
        .save()
        .map_err(|e| format!("Failed to save failed transcription: {}", e))?;

    // Emit the new transcription data to frontend
    let _ = emit_to_window(app, "main", "transcription-added", transcription_data);

    // Refresh tray menu
    if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
        log::warn!(
            "Failed to update tray menu after saving failed transcription: {}",
            e
        );
    }

    log::info!(
        "Saved failed transcription with recording file: {}",
        recording_file
    );
    Ok(())
}

#[tauri::command]
pub async fn get_transcription_history(
    app: AppHandle,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    let store = app.store("transcriptions").map_err(|e| e.to_string())?;
    let current_session_marker = current_retranscription_session_marker();

    let mut entries: Vec<(String, serde_json::Value)> = Vec::new();
    let mut pending_updates: Vec<(String, serde_json::Value)> = Vec::new();

    // Collect all entries, reconciling stale retranscription rows before sorting.
    for key in store.keys() {
        if let Some(value) = store.get(&key) {
            let reconciled =
                reconcile_transcription_history_entry(value.clone(), &current_session_marker);
            if reconciled != value {
                pending_updates.push((key.to_string(), reconciled.clone()));
            }
            entries.push((key.to_string(), reconciled));
        }
    }

    if !pending_updates.is_empty() {
        for (key, value) in pending_updates {
            store.set(&key, value);
        }

        store
            .save()
            .map_err(|e| format!("Failed to save reconciled transcription history: {}", e))?;

        if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
            log::warn!(
                "Failed to update tray menu after reconciling transcription history: {}",
                e
            );
        }
    }

    entries.sort_by(|a, b| b.0.cmp(&a.0));

    let limit = limit.unwrap_or(50);
    entries.truncate(limit);

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
    transcribe_audio_file_impl(app, file_path, model_name, model_engine, true).await
}

pub async fn transcribe_audio_file_for_cli(
    app: AppHandle,
    file_path: String,
    model_name: String,
    model_engine: Option<String>,
) -> Result<String, String> {
    transcribe_audio_file_impl(app, file_path, model_name, model_engine, false).await
}

async fn transcribe_audio_file_impl(
    app: AppHandle,
    file_path: String,
    model_name: String,
    model_engine: Option<String>,
    validate_requirements: bool,
) -> Result<String, String> {
    log::info!(
        "[UPLOAD] transcribe_audio_file START | file_path={:?}, model_name={}, engine_hint={:?}",
        file_path,
        model_name,
        model_engine
    );
    if validate_requirements {
        validate_recording_requirements(&app).await?;
    }

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
    let legacy_speech_language = store
        .get("language")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "en".to_string());
    let legacy_translate_to_english = store
        .get("translate_to_english")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let language = store
        .get("speech_language")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or(legacy_speech_language);
    let stored_transcription_task = store
        .get("transcription_task")
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let transcription_task = normalize_transcription_task(
        stored_transcription_task.as_deref(),
        legacy_translate_to_english,
    );
    let translate_to_english = task_uses_translate_to_english(&transcription_task);

    let language = normalize_speech_language_for_model(
        engine_selection.engine_name(),
        engine_selection.model_name(),
        &language,
    );

    log::info!(
        "[LANGUAGE] transcribe_audio_file using language: {}, translate: {}",
        language,
        translate_to_english
    );

    let transcription_job = build_transcription_job(
        TranscriptionSource::AudioFile,
        engine_selection.engine_name().to_string(),
        engine_selection.model_name().to_string(),
        Some(language.clone()),
        translate_to_english,
    );

    // For Soniox, skip normalization and send original wav_path
    let transcription_result = match engine_selection {
        ActiveEngineSelection::Whisper { model_path, .. } => {
            // Normalize to Whisper contract
            log::debug!("[UPLOAD] Normalizing to Whisper WAV (16k mono s16)...");
            let normalized_file = NormalizedTempFile::new({
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let out_path = recordings_dir.join(format!("normalized_{}.wav", ts));
                crate::ffmpeg::normalize_streaming(&app, &wav_path, &out_path)
                    .await
                    .map_err(|e| format!("Audio normalization (ffmpeg) failed: {}", e))?;
                out_path
            });
            log::info!("[UPLOAD] Normalized WAV at {:?}", normalized_file.path());
            let initial_prompt = compile_whisper_initial_prompt(&app, Some(&language));
            let output = transcribe_whisper_with_acceleration(
                &app,
                &model_path,
                normalized_file.path(),
                Some(&language),
                translate_to_english,
                initial_prompt.as_deref(),
                || false,
            )
            .await?;
            TranscriptionResult::new(&transcription_job, output.raw_text)
                .with_transcript_language(output.transcript_language)
                .with_segments(output.segments)
                .with_audio_duration_ms(Some(output.audio_duration_ms))
                .with_processing_duration_ms(Some(output.processing_duration_ms))
        }
        ActiveEngineSelection::Parakeet { model_name } => {
            // Normalize to Whisper/Parakeet contract first
            log::debug!("[UPLOAD] Normalizing to Whisper WAV (16k mono s16)...");
            let normalized_file = NormalizedTempFile::new({
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let out_path = recordings_dir.join(format!("normalized_{}.wav", ts));
                crate::ffmpeg::normalize_streaming(&app, &wav_path, &out_path)
                    .await
                    .map_err(|e| format!("Audio normalization (ffmpeg) failed: {}", e))?;
                out_path
            });
            log::info!("[UPLOAD] Normalized WAV at {:?}", normalized_file.path());
            let parakeet_manager = app.state::<ParakeetManager>();

            parakeet_manager
                .load_model(&app, &model_name)
                .await
                .map_err(|e| format!("Failed to load Parakeet model: {}", e))?;

            match parakeet_manager
                .transcribe(
                    &app,
                    &model_name,
                    normalized_file.path().to_path_buf(),
                    Some(language.clone()),
                    translate_to_english,
                )
                .await
            {
                Ok(ParakeetResponse::Transcription {
                    text,
                    segments,
                    language,
                    duration,
                }) => TranscriptionResult::new(&transcription_job, text)
                    .with_transcript_language(language)
                    .with_segments(parakeet_segments_to_transcription_segments(segments))
                    .with_audio_duration_ms(seconds_to_duration_ms(duration)),
                Ok(other) => {
                    return Err(format!("Unexpected Parakeet response: {:?}", other));
                }
                Err(err) => {
                    return Err(format!("Parakeet transcription failed: {}", err));
                }
            }
        }
        ActiveEngineSelection::Soniox { .. } => {
            let text = soniox_transcribe_async(&app, &wav_path, Some(&language)).await?;
            let soniox_job = build_transcription_job(
                TranscriptionSource::AudioFile,
                transcription_job.engine.clone(),
                transcription_job.model.clone(),
                transcription_job.spoken_language.clone(),
                false,
            );
            TranscriptionResult::new(&soniox_job, text)
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
            let normalized_file = NormalizedTempFile::new({
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let out_path = recordings_dir.join(format!("normalized_{}.wav", ts));
                crate::ffmpeg::normalize_streaming(&app, &wav_path, &out_path)
                    .await
                    .map_err(|e| format!("Audio normalization (ffmpeg) failed: {}", e))?;
                out_path
            });
            log::info!("[UPLOAD] Normalized WAV at {:?}", normalized_file.path());

            log::info!(
                "🌐 [Remote Upload] Starting transcription to '{}' ({}:{})",
                server_name,
                host,
                port
            );

            // Read the normalized audio file
            let audio_data = std::fs::read(normalized_file.path())
                .map_err(|e| format!("Failed to read audio file: {}", e))?;

            let audio_size_kb = audio_data.len() as f64 / 1024.0;
            log::info!(
                "🌐 [Remote Upload] Sending {:.1} KB audio to '{}'",
                audio_size_kb,
                server_name
            );

            // Create HTTP client connection
            let server_conn = RemoteServerConnection::new(host.clone(), port, password.clone());

            let (request, timeout_ms) = build_remote_upload_transcription_request(
                normalized_file.path(),
                audio_data,
                Some(&transcription_job),
            );

            let response = client::transcribe_audio(&server_conn, request, timeout_ms)
                .await
                .map_err(|e| {
                    log::warn!(
                        "🌐 [Remote Upload] Remote transcription FAILED to '{}': {}",
                        server_name,
                        e
                    );
                    e.to_string()
                })?;

            log::info!(
                "🌐 [Remote Upload] Transcription COMPLETED from '{}': {} chars received",
                server_name,
                response.text.len()
            );

            build_remote_transcription_result(&transcription_job, response)
        }
    };

    log::info!(
        "[UPLOAD] Completed transcription, {} characters",
        transcription_result.raw_text.len()
    );
    let ai_enabled = load_ai_enabled(&app)?;
    let writing_result =
        crate::writing::process_transcription(app.clone(), transcription_result, ai_enabled)
            .await?;
    Ok(writing_result.final_text)
}

#[tauri::command]
pub async fn diarize_audio_file(
    app: AppHandle,
    file_path: String,
) -> Result<Vec<UploadDiarizationSegment>, String> {
    validate_recording_requirements(&app).await?;

    let audio_path = std::path::PathBuf::from(&file_path);
    if !audio_path.exists() {
        return Err(format!("Audio file not found: {}", file_path));
    }

    let parakeet_manager = app.state::<ParakeetManager>();
    match parakeet_manager
        .diarize(&app, audio_path)
        .await
        .map_err(|err| format!("Parakeet diarization failed: {}", err))?
    {
        ParakeetResponse::Diarization { segments } => Ok(segments
            .into_iter()
            .map(|segment| UploadDiarizationSegment {
                speaker_id: segment.speaker_id,
                start_ms: seconds_to_duration_ms(Some(segment.start)).unwrap_or_default(),
                end_ms: seconds_to_duration_ms(Some(segment.end)).unwrap_or_default(),
            })
            .collect()),
        other => Err(format!(
            "Unexpected Parakeet diarization response: {:?}",
            other
        )),
    }
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
    let legacy_speech_language = store
        .get("language")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "en".to_string());
    let legacy_translate_to_english = store
        .get("translate_to_english")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let language = store
        .get("speech_language")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or(legacy_speech_language);
    let stored_transcription_task = store
        .get("transcription_task")
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let transcription_task = normalize_transcription_task(
        stored_transcription_task.as_deref(),
        legacy_translate_to_english,
    );
    let translate_to_english = task_uses_translate_to_english(&transcription_task);

    let language = normalize_speech_language_for_model(
        engine_selection.engine_name(),
        engine_selection.model_name(),
        &language,
    );

    log::info!(
        "[LANGUAGE] transcribe_audio using language: {}, translate: {}",
        language,
        translate_to_english
    );

    let transcription_job = build_transcription_job(
        TranscriptionSource::AudioBytes,
        engine_selection.engine_name().to_string(),
        engine_selection.model_name().to_string(),
        Some(language.clone()),
        translate_to_english,
    );

    let transcription_result = match engine_selection {
        ActiveEngineSelection::Whisper { model_path, .. } => {
            let initial_prompt = compile_whisper_initial_prompt(&app, Some(language.as_str()));
            let output = transcribe_whisper_with_acceleration(
                &app,
                &model_path,
                &temp_path,
                Some(language.as_str()),
                translate_to_english,
                initial_prompt.as_deref(),
                || false,
            )
            .await?;
            TranscriptionResult::new(&transcription_job, output.raw_text)
                .with_transcript_language(output.transcript_language)
                .with_segments(output.segments)
                .with_audio_duration_ms(Some(output.audio_duration_ms))
                .with_processing_duration_ms(Some(output.processing_duration_ms))
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
                Ok(ParakeetResponse::Transcription {
                    text,
                    segments,
                    language,
                    duration,
                }) => TranscriptionResult::new(&transcription_job, text)
                    .with_transcript_language(language)
                    .with_segments(parakeet_segments_to_transcription_segments(segments))
                    .with_audio_duration_ms(seconds_to_duration_ms(duration)),
                Ok(other) => return Err(format!("Unexpected Parakeet response: {:?}", other)),
                Err(err) => return Err(format!("Parakeet transcription failed: {}", err)),
            }
        }
        ActiveEngineSelection::Soniox { .. } => {
            let text = soniox_transcribe_async(&app, &temp_path, Some(&language)).await?;
            let soniox_job = build_transcription_job(
                TranscriptionSource::AudioBytes,
                transcription_job.engine.clone(),
                transcription_job.model.clone(),
                transcription_job.spoken_language.clone(),
                false,
            );
            TranscriptionResult::new(&soniox_job, text)
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
            let normalized_file = NormalizedTempFile::new({
                let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let out_path = recordings_dir.join(format!("normalized_clipboard_{}.wav", ts));
                crate::ffmpeg::normalize_streaming(&app, &temp_path, &out_path)
                    .await
                    .map_err(|e| format!("Audio normalization (ffmpeg) failed: {}", e))?;
                out_path
            });
            log::info!("[CLIPBOARD] Normalized WAV at {:?}", normalized_file.path());

            log::info!(
                "🌐 [Remote Clipboard] Starting transcription to '{}' ({}:{})",
                server_name,
                host,
                port
            );

            // Read the normalized audio file
            let audio_data = std::fs::read(normalized_file.path())
                .map_err(|e| format!("Failed to read audio file: {}", e))?;

            let audio_size_kb = audio_data.len() as f64 / 1024.0;
            log::info!(
                "🌐 [Remote Clipboard] Sending {:.1} KB audio to '{}'",
                audio_size_kb,
                server_name
            );

            // Create HTTP client connection
            let server_conn = RemoteServerConnection::new(host.clone(), port, password.clone());

            let (request, timeout_ms) = build_remote_upload_transcription_request(
                normalized_file.path(),
                audio_data,
                Some(&transcription_job),
            );

            let response = client::transcribe_audio(&server_conn, request, timeout_ms)
                .await
                .map_err(|e| {
                    log::warn!(
                        "🌐 [Remote Clipboard] Remote transcription FAILED to '{}': {}",
                        server_name,
                        e
                    );
                    e.to_string()
                })?;

            log::info!(
                "🌐 [Remote Clipboard] Transcription COMPLETED from '{}': {} chars received",
                server_name,
                response.text.len()
            );

            build_remote_transcription_result(&transcription_job, response)
        }
    };

    // Clean up
    if let Err(e) = std::fs::remove_file(&temp_path) {
        log::warn!("Failed to remove test audio file: {}", e);
    }

    let ai_enabled = load_ai_enabled(&app)?;
    let writing_result =
        crate::writing::process_transcription(app.clone(), transcription_result, ai_enabled)
            .await?;
    Ok(writing_result.final_text)
}

fn build_soniox_create_payload(
    file_id: &str,
    language: Option<&str>,
    context: Option<crate::writing::SonioxContext>,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "model": "stt-async-v3",
        "file_id": file_id,
    });

    if let Some(lang) = language.map(str::trim).filter(|lang| !lang.is_empty()) {
        payload["language_hints"] = serde_json::json!([lang]);
    }

    if let Some(context) = context {
        if let Ok(context_value) = serde_json::to_value(context) {
            if context_value
                .as_object()
                .is_some_and(|object| !object.is_empty())
            {
                payload["context"] = context_value;
            }
        }
    }

    payload
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
    let soniox_context = match crate::writing::load_writing_settings(app) {
        Ok(settings) => crate::writing::compile_soniox_context(&settings, language),
        Err(err) => {
            log::warn!(
                "Failed to load writing settings for Soniox context; continuing without context: {err}"
            );
            None
        }
    };
    let payload = build_soniox_create_payload(&file_id, language, soniox_context);

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

/// Validate that a recording filename is safe (no path traversal)
fn validate_recording_filename(filename: &str) -> Result<(), String> {
    use std::path::Component;
    let path = std::path::Path::new(filename);

    // Reject empty filenames
    if filename.is_empty() {
        return Err("Empty filename".to_string());
    }

    // Reject absolute paths
    if path.is_absolute() {
        return Err("Absolute paths are not allowed".to_string());
    }

    // Reject any non-Normal components (../, ./, prefix, root)
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            other => {
                return Err(format!("Invalid path component: {:?}", other));
            }
        }
    }

    Ok(())
}

/// Check if a recording file exists in the recordings directory
#[tauri::command]
pub async fn check_recording_exists(app: AppHandle, filename: String) -> Result<bool, String> {
    validate_recording_filename(&filename)?;
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
    validate_recording_filename(&filename)?;
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
    status: Option<TranscriptionStatus>,
) -> Result<String, String> {
    // Save transcription to store with current timestamp
    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;

    let timestamp = chrono::Utc::now().to_rfc3339();
    let mut transcription_data = serde_json::json!({
        "text": text.clone(),
        "model": model,
        "timestamp": timestamp.clone(),
        "recording_file": recording_file.clone(),
        "source_recording_id": source_recording_id.clone(),
        "is_retranscription": true,
    });

    let effective_status = transcription_data
        .as_object_mut()
        .ok_or_else(|| "Failed to build retranscription payload".to_string())
        .map(|map| apply_retranscription_status(map, status))?;

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
        "Saved retranscription with {} characters (source: {}, status: {})",
        text.len(),
        source_recording_id,
        effective_status.as_str()
    );
    Ok(timestamp)
}

/// Update an existing transcription entry in place (for re-transcription)
#[tauri::command]
pub async fn update_transcription(
    app: AppHandle,
    timestamp: String,
    text: String,
    model: String,
    status: Option<TranscriptionStatus>,
) -> Result<(), String> {
    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;

    // Get the existing entry
    let existing = store
        .get(&timestamp)
        .ok_or_else(|| format!("Transcription not found: {}", timestamp))?;

    // Preserve original fields, update text, model, and status.
    let mut updated = existing.clone();
    let effective_status = updated
        .as_object_mut()
        .ok_or_else(|| "Transcription entry is not an object".to_string())
        .map(|map| {
            map.insert("text".to_string(), serde_json::Value::String(text.clone()));
            map.insert(
                "model".to_string(),
                serde_json::Value::String(model.clone()),
            );
            let effective_status = apply_retranscription_status(map, status);
            sync_retranscription_failure_metadata(map, effective_status, &text);
            effective_status
        })?;

    store.set(&timestamp, updated.clone());

    store
        .save()
        .map_err(|e| format!("Failed to save updated transcription: {}", e))?;

    // Emit update event to frontend
    let _ = emit_to_window(
        &app,
        "main",
        "transcription-updated",
        serde_json::json!({
            "timestamp": timestamp,
            "text": text,
            "model": model,
            "status": transcription_status_value(effective_status)
        }),
    );

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
