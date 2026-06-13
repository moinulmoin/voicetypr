//! Stage 1 transcription executor.
//!
//! `transcribe_with_app` is the single entry point that resolves an
//! [`EngineSelection`] to a concrete engine and runs the job, returning the
//! existing [`TranscriptionResult`] shape or a typed [`TranscriptionError`].
//!
//! Stage 1 (plan 014) is additive: no existing callsite is rewired yet. The
//! executor fully handles `EngineSelection::Explicit` routing to the local and
//! cloud engines (Whisper / Parakeet / Cloud) by delegating to the existing
//! engine helpers. Two routes are intentionally deferred to later migration
//! stages and return a typed error if reached:
//! - `EngineSelection::HostDefault` (remote-server inbound) — Stage 4.
//! - `ActiveEngineSelection::Remote` (send-to-peer) — Stage 5 (multipart wire).
//!
//! Timeout/watchdog enforcement of [`TimeoutPolicy`] is layered at this seam by
//! plan 015; Stage 1 carries the policy on the request but does not abort.

use std::path::Path;

use tauri::{AppHandle, Manager};
use tempfile::NamedTempFile;

use crate::commands::audio::{
    compile_parakeet_custom_vocabulary_for_transcription,
    parakeet_segments_to_transcription_segments, resolve_engine_for_model,
    transcribe_whisper_with_acceleration, ActiveEngineSelection,
};
use crate::parakeet::manager::{ParakeetManager, ParakeetTranscriptionOptions};
use crate::parakeet::messages::ParakeetResponse;
use crate::secure_store::secure_get;
use crate::transcription::error::{
    from_local_engine_string, from_stt_error, TranscriptionError, TranscriptionErrorCode,
};
use crate::transcription::request::{
    CleanupPolicy, EngineSelection, TranscriptionAudio, TranscriptionRequest,
};
use crate::transcription::{
    TranscriptionJob, TranscriptionResult, TranscriptionSource, TranscriptionTask,
};

/// Resolve, route, and run one transcription request.
pub async fn transcribe_with_app(
    app: &AppHandle,
    request: TranscriptionRequest,
) -> Result<TranscriptionResult, TranscriptionError> {
    let source = request.source;

    let active = resolve_active(app, &request.engine, source).await?;
    let job = TranscriptionJob {
        source,
        engine: active.engine_name().to_string(),
        model: active.model_name().to_string(),
        spoken_language: request.spoken_language.clone(),
        task: request.task,
    };

    // Materialize audio to a filesystem path. Inline bytes are staged into an
    // owned temp file that is deleted when `_bytes_temp` drops.
    let (input_path, _bytes_temp, caller_input) = match &request.audio {
        TranscriptionAudio::Path { path, cleanup, .. } => {
            let caller = match cleanup {
                CleanupPolicy::CallerOwns => None,
                other => Some((path.clone(), other.clone())),
            };
            (path.clone(), None, caller)
        }
        TranscriptionAudio::Bytes { bytes, .. } => {
            let temp = NamedTempFile::new().map_err(|e| stage_error(source, "temp create", e))?;
            std::fs::write(temp.path(), bytes).map_err(|e| stage_error(source, "temp write", e))?;
            (temp.path().to_path_buf(), Some(temp), None)
        }
    };

    let outcome = route(app, &active, &job, &input_path, &request).await;

    // Apply the caller's cleanup policy to a caller-provided Path input.
    if let Some((path, policy)) = caller_input {
        let (success, retryable) = match &outcome {
            Ok(_) => (true, false),
            Err(e) => (false, e.retryable),
        };
        if should_delete_input(&policy, success, retryable) {
            if let Err(e) = std::fs::remove_file(&path) {
                log::warn!(
                    "executor: failed to remove input audio {}: {e}",
                    path.display()
                );
            }
        }
    }

    outcome
}

async fn resolve_active(
    app: &AppHandle,
    engine: &EngineSelection,
    source: TranscriptionSource,
) -> Result<ActiveEngineSelection, TranscriptionError> {
    match engine {
        EngineSelection::Explicit { engine, model } => {
            resolve_engine_for_model(app, model, Some(engine.as_str()))
                .await
                .map_err(|e| from_local_engine_string(&e, source))
        }
        // Stage 4: remote-server inbound snapshots its own shared engine/model.
        EngineSelection::HostDefault => Err(TranscriptionError::new(
            TranscriptionErrorCode::Internal,
            source,
            "Host-default engine resolution is wired by the remote-server inbound port (Stage 4).",
        )),
    }
}

async fn route(
    app: &AppHandle,
    active: &ActiveEngineSelection,
    job: &TranscriptionJob,
    input_path: &Path,
    request: &TranscriptionRequest,
) -> Result<TranscriptionResult, TranscriptionError> {
    let source = request.source;
    let language = request.spoken_language.as_deref();
    let translate = matches!(request.task, TranscriptionTask::TranslateToEnglish);

    match active {
        ActiveEngineSelection::Whisper { model_path, .. } => {
            let wav = normalize_to_wav(app, input_path, source).await?;
            let token = request.cancellation.clone();
            let output = transcribe_whisper_with_acceleration(
                app,
                model_path,
                wav.path(),
                language,
                translate,
                None,
                move || token.is_cancelled(),
            )
            .await
            .map_err(|e| from_local_engine_string(&e, source))?;

            Ok(TranscriptionResult::new(job, output.raw_text)
                .with_transcript_language(output.transcript_language)
                .with_segments(output.segments)
                .with_audio_duration_ms(Some(output.audio_duration_ms))
                .with_processing_duration_ms(Some(output.processing_duration_ms)))
        }
        ActiveEngineSelection::Parakeet { model_name } => {
            let wav = normalize_to_wav(app, input_path, source).await?;
            let manager = app.state::<ParakeetManager>();
            let cancel = request.cancellation.as_arc();

            if let Err(e) = manager
                .load_model_with_cancel(app, model_name, Some(cancel.clone()))
                .await
            {
                if request.cancellation.is_cancelled() {
                    return Err(cancelled(source));
                }
                return Err(from_local_engine_string(
                    &format!("Parakeet model load failed: {e}"),
                    source,
                ));
            }
            if request.cancellation.is_cancelled() {
                return Err(cancelled(source));
            }

            let custom_vocabulary =
                compile_parakeet_custom_vocabulary_for_transcription(app, language);
            let options = ParakeetTranscriptionOptions {
                language: request.spoken_language.clone(),
                translate,
                custom_vocabulary,
                cancel_flag: Some(cancel),
            };

            match manager
                .transcribe_with_custom_vocabulary(
                    app,
                    model_name,
                    wav.path().to_path_buf(),
                    options,
                )
                .await
            {
                Ok(ParakeetResponse::Transcription {
                    text,
                    segments,
                    language,
                    duration,
                }) => Ok(TranscriptionResult::new(job, text)
                    .with_transcript_language(language)
                    .with_segments(parakeet_segments_to_transcription_segments(segments))
                    .with_audio_duration_ms(duration.map(|s| (s.max(0.0) * 1000.0) as u64))),
                Ok(ParakeetResponse::Error { code, message, .. }) => Err(TranscriptionError::new(
                    TranscriptionErrorCode::EngineFailed,
                    source,
                    "Transcription failed",
                )
                .with_detail(format!("Parakeet error {code}: {message}"))),
                Ok(_) => Err(TranscriptionError::new(
                    TranscriptionErrorCode::ResponseInvalid,
                    source,
                    "Transcription failed",
                )
                .with_detail("unexpected Parakeet response")),
                Err(e) => Err(from_local_engine_string(
                    &format!("Parakeet transcription failed: {e}"),
                    source,
                )),
            }
        }
        ActiveEngineSelection::Cloud { provider, .. } => {
            let key = match secure_get(app, provider.key_name()) {
                Ok(Some(key)) if !key.trim().is_empty() => key,
                _ => {
                    return Err(TranscriptionError::new(
                        TranscriptionErrorCode::Unauthorized,
                        source,
                        format!("{} API key not set", provider.display_name()),
                    ))
                }
            };
            match provider.transcribe_typed(app, &key, input_path, language).await {
                Ok(text) => Ok(TranscriptionResult::new(job, text)),
                Err(e) => Err(from_stt_error(&e, source)),
            }
        }
        // Stage 5: sending to a peer goes through the multipart remote client.
        ActiveEngineSelection::Remote { .. } => Err(TranscriptionError::new(
            TranscriptionErrorCode::Internal,
            source,
            "Remote (send-to-peer) transcription via the executor is wired by the remote-client port (Stage 5).",
        )),
    }
}

/// Normalize any input to a 16 kHz mono WAV temp file (deleted on drop).
async fn normalize_to_wav(
    app: &AppHandle,
    input_path: &Path,
    source: TranscriptionSource,
) -> Result<NamedTempFile, TranscriptionError> {
    let out = NamedTempFile::new().map_err(|e| stage_error(source, "temp create", e))?;
    crate::ffmpeg::normalize_streaming(app, input_path, out.path())
        .await
        .map_err(|e| from_local_engine_string(&e, source))?;
    Ok(out)
}

fn stage_error(source: TranscriptionSource, what: &str, err: std::io::Error) -> TranscriptionError {
    TranscriptionError::new(
        TranscriptionErrorCode::Internal,
        source,
        "Failed to stage audio for transcription",
    )
    .with_detail(format!("{what}: {err}"))
}

fn cancelled(source: TranscriptionSource) -> TranscriptionError {
    TranscriptionError::new(
        TranscriptionErrorCode::Cancelled,
        source,
        "Transcription cancelled",
    )
}

/// Whether a caller-provided input file should be removed after the attempt.
fn should_delete_input(policy: &CleanupPolicy, success: bool, retryable: bool) -> bool {
    match policy {
        CleanupPolicy::CallerOwns => false,
        CleanupPolicy::DeleteAfterAttempt => true,
        CleanupPolicy::PreserveOnRetryableFailure => success || !retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::should_delete_input;
    use crate::transcription::request::CleanupPolicy;

    const CASES: [(bool, bool); 3] = [(true, false), (false, true), (false, false)];

    #[test]
    fn caller_owns_never_deletes() {
        for (success, retryable) in CASES {
            assert!(!should_delete_input(
                &CleanupPolicy::CallerOwns,
                success,
                retryable
            ));
        }
    }

    #[test]
    fn delete_after_attempt_always_deletes() {
        for (success, retryable) in CASES {
            assert!(should_delete_input(
                &CleanupPolicy::DeleteAfterAttempt,
                success,
                retryable
            ));
        }
    }

    #[test]
    fn preserve_on_retryable_keeps_only_retryable_failures() {
        // success -> delete
        assert!(should_delete_input(
            &CleanupPolicy::PreserveOnRetryableFailure,
            true,
            false
        ));
        // non-retryable failure -> delete
        assert!(should_delete_input(
            &CleanupPolicy::PreserveOnRetryableFailure,
            false,
            false
        ));
        // retryable failure -> keep for retry
        assert!(!should_delete_input(
            &CleanupPolicy::PreserveOnRetryableFailure,
            false,
            true
        ));
    }
}
