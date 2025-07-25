use tauri::{AppHandle, Manager, State};

use crate::audio::recorder::AudioRecorder;
use crate::commands::license::check_license_status_internal;
use crate::license::LicenseState;
use crate::whisper::cache::TranscriberCache;
use crate::whisper::languages::validate_language;
use crate::whisper::manager::WhisperManager;
use crate::{emit_to_window, update_recording_state, AppState, RecordingState};
use serde_json;
use std::sync::Mutex;
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tauri_plugin_store::StoreExt;

// Global audio recorder state
pub struct RecorderState(pub Mutex<AudioRecorder>);

/// Select the best fallback model based on available models
/// Prioritizes models in order: tiny, base, small, medium, large variants
fn select_best_fallback_model(available_models: &[String], requested: &str) -> String {
    // Model priority order (smaller models are often more reliable)
    const MODEL_PRIORITY: &[&str] = &[
        "tiny",
        "base",
        "small",
        "medium",
        "large-v3-turbo-q5_0",
        "large-v3-turbo",
        "large-v3-q5_0",
        "large-v3",
    ];

    // First try to find a model similar to the requested one
    if !requested.is_empty() {
        // If requested "large-v3", try other large variants first
        for model in available_models {
            if model.starts_with(&requested.split('-').next().unwrap_or(requested)) {
                return model.clone();
            }
        }
    }

    // Otherwise use priority order
    for priority_model in MODEL_PRIORITY {
        if available_models.contains(&priority_model.to_string()) {
            return priority_model.to_string();
        }
    }

    // If no priority model found, return first available
    available_models.first().map(|s| s.clone()).unwrap_or_else(|| {
        log::error!("No models available for fallback selection");
        // This should never happen as we check for empty models before calling this function
        // But return a default to prevent panic
        "tiny".to_string()
    })
}

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, RecorderState>,
) -> Result<(), String> {
    // Check if we have any models BEFORE starting to record
    let whisper_manager = app.state::<AsyncRwLock<WhisperManager>>();
    let has_models = whisper_manager.read().await.has_downloaded_models();

    if !has_models {
        log::error!("Cannot start recording - no models downloaded");

        // Emit error event with guidance
        let _ = emit_to_window(
            &app,
            "main",
            "no-models-error",
            serde_json::json!({
                "title": "No Speech Recognition Models",
                "message": "Please download at least one model from Settings before recording.",
                "action": "open-settings"
            }),
        );

        return Err(
            "No speech recognition models installed. Please download a model first.".to_string(),
        );
    }

    // Check license status before allowing recording
    log::info!("[Recording] Checking license status before start_recording");
    let license_status = check_license_status_internal(&app).await?;
    if matches!(
        license_status.status,
        LicenseState::Expired | LicenseState::None
    ) {
        log::error!("Cannot start recording - license required");

        // Emit error event with guidance
        let _ = emit_to_window(
            &app,
            "main",
            "license-required",
            serde_json::json!({
                "title": "License Required",
                "message": "Your trial has expired. Please purchase a license to continue",
                "action": "open-purchase"
            }),
        );

        return Err(
            "License required to record. Please purchase a license."
                .to_string(),
        );
    }

    // Update state to starting
    update_recording_state(&app, RecordingState::Starting, None);
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

        // Try to start recording with graceful error handling
        match recorder.start_recording(
            audio_path
                .to_str()
                .ok_or_else(|| "Invalid path encoding".to_string())?,
        ) {
            Ok(_) => {
                // Verify recording actually started
                if !recorder.is_recording() {
                    log::error!("Recording failed to start!");
                    update_recording_state(
                        &app,
                        RecordingState::Error,
                        Some("Microphone initialization failed".to_string()),
                    );

                    // Emit user-friendly error
                    let _ = emit_to_window(&app, "pill", "recording-error",
                        "Could not access microphone. Please check your audio settings and permissions.");

                    return Err("Failed to start recording".to_string());
                }
            }
            Err(e) => {
                log::error!("Failed to start recording: {}", e);
                update_recording_state(&app, RecordingState::Error, Some(e.to_string()));

                // Provide specific error messages for common issues
                let user_message = if e.contains("permission") || e.contains("access") {
                    "Microphone access denied. Please grant permission in System Preferences."
                } else if e.contains("device") || e.contains("not found") {
                    "No microphone found. Please connect a microphone and try again."
                } else if e.contains("in use") || e.contains("busy") {
                    "Microphone is being used by another application. Please close other recording apps."
                } else {
                    "Could not start recording. Please check your audio settings."
                };

                let _ = emit_to_window(&app, "pill", "recording-error", user_message);

                return Err(e);
            }
        }

        // Start audio level monitoring before releasing the lock
        if let Some(audio_level_rx) = recorder.take_audio_level_receiver() {
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
    match app.store("settings") {
        Ok(store) => {
            let show_pill = store
                .get("show_pill_widget")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            if show_pill {
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
        }
        Err(e) => {
            log::warn!(
                "Could not access settings to check pill widget preference: {}",
                e
            );
            // Continue without pill widget - recording still works
        }
    }

    // Also emit legacy event for compatibility
    let _ = emit_to_window(&app, "pill", "recording-started", ());
    log::debug!("Recording started successfully");

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
    // Update state to stopping
    update_recording_state(&app, RecordingState::Stopping, None);
    // DO NOT request cancellation here - we want transcription to complete!
    // Cancellation should only happen in cancel_recording command

    // Stop recording (lock only within this scope to stay Send)
    log::info!("Stopping recording...");
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

        let stop_message = recorder.stop_recording()?;
        log::info!("{}", stop_message);

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

    // Get current model from settings
    let store = app.store("settings").map_err(|e| e.to_string())?;

    // Get available models
    let whisper_manager = app.state::<AsyncRwLock<WhisperManager>>();
    let downloaded_models = whisper_manager.read().await.get_downloaded_model_names();

    log::debug!("Downloaded models: {:?}", downloaded_models);

    // STOP HERE if no models are downloaded - can't transcribe without models!
    if downloaded_models.is_empty() {
        log::error!("No models downloaded - cannot transcribe");
        update_recording_state(
            &app,
            RecordingState::Error,
            Some("No speech recognition models installed".to_string()),
        );

        // Clean up the recording
        if let Err(e) = std::fs::remove_file(&audio_path) {
            log::warn!("Failed to remove audio file: {}", e);
        }

        // Tell user they MUST download a model
        let _ = emit_to_window(
            &app,
            "pill",
            "no-models-error",
            serde_json::json!({
                "title": "No Models Installed",
                "message": "Please download at least one speech recognition model from Settings to use VoiceTypr.",
                "action": "open-settings"
            }),
        );

        // Hide pill window since we can't proceed
        if let Err(e) = crate::commands::window::hide_pill_widget(app.clone()).await {
            log::error!("Failed to hide pill window: {}", e);
        }

        // Transition back to Idle state
        update_recording_state(&app, RecordingState::Idle, None);

        return Err(
            "No speech recognition models installed. Please download a model from Settings."
                .to_string(),
        );
    }

    // Smart model selection with graceful degradation
    let configured_model = store
        .get("current_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .filter(|s| !s.is_empty()); // Treat empty string as no configured model

    let model_name = if let Some(configured_model) = configured_model {
        // Use configured model if it exists and is downloaded
        if downloaded_models.contains(&configured_model) {
            configured_model
        } else if downloaded_models.is_empty() {
            // This should never happen since we check earlier, but just in case
            log::error!("No models available for fallback");
            return Err("No models available".to_string());
        } else {
            // Fallback to best available model
            let fallback_model = select_best_fallback_model(&downloaded_models, &configured_model);
            log::info!(
                "Configured model '{}' not available, falling back to: {}",
                configured_model,
                fallback_model
            );

            // Notify user about fallback
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
        // No configured model - auto-select the best available
        // We already checked that downloaded_models is not empty above
        let best_model = select_best_fallback_model(&downloaded_models, "");
        log::info!("No model configured, auto-selecting: {}", best_model);
        best_model
    };

    log::info!("Using model for transcription: {}", model_name);

    let model_path = whisper_manager
        .read()
        .await
        .get_model_path(&model_name)
        .ok_or(format!("Model '{}' path not found", model_name))?;

    let model_name_clone = model_name.clone();

    let language = store
        .get("language")
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    let translate_to_english = store
        .get("translate_to_english")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    log::info!("[LANGUAGE] stop_recording: language={:?}, translate={}", language, translate_to_english);

    // clone for move into task
    let audio_path_clone = audio_path.clone();
    let model_path_clone = model_path.clone();

    log::debug!("Spawning transcription task with model: {}", model_name);

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

        // Get (or load) transcriber
        let transcriber = {
            let cache_state = app_for_task.state::<AsyncMutex<TranscriberCache>>();
            let mut cache = cache_state.lock().await;
            match cache.get_or_create(&model_path_clone) {
                Ok(t) => t,
                Err(e) => {
                    // Update state to error
                    update_recording_state(&app_for_task, RecordingState::Error, Some(e.clone()));

                    // Hide pill widget
                    let _ = crate::commands::window::hide_pill_widget(app_for_task.clone()).await;

                    // Also emit legacy event to pill window
                    let _ = emit_to_window(&app_for_task, "pill", "transcription-error", e);
                    return;
                }
            }
        };

        // Retry logic for transcription
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY_MS: u64 = 500;

        let mut result = Err("No attempt made".to_string());

        for attempt in 1..=MAX_RETRIES {
            // Check for cancellation before each attempt
            if app_state.is_cancellation_requested() {
                log::info!("Transcription cancelled at attempt {}", attempt);
                result = Err("Transcription cancelled".to_string());
                break;
            }

            result = transcriber.transcribe_with_cancellation(
                &audio_path_clone,
                language.as_deref(),
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
                        std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
                    } else {
                        log::error!("Transcription failed after {} attempts: {}", MAX_RETRIES, e);
                    }
                }
            }
        }

        // Clean up temp file regardless of outcome
        if let Err(e) = std::fs::remove_file(&audio_path_clone) {
            log::warn!("Failed to remove temporary audio file: {}", e);
        }

        match result {
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

                // Check if transcription is empty or only whitespace
                if text.trim().is_empty() {
                    log::info!("Transcription is empty, no speech detected");

                    // Emit event to pill for user feedback
                    let _ = emit_to_window(
                        &app_for_task,
                        "pill",
                        "transcription-empty",
                        "No speech detected",
                    );

                    // Wait a bit for feedback to show
                    let app_for_empty = app_for_task.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

                        // Hide pill window
                        let app_state = app_for_empty.state::<AppState>();
                        if let Some(window_manager) = app_state.get_window_manager() {
                            if let Err(e) = window_manager.hide_pill_window().await {
                                log::error!("Failed to hide pill window: {}", e);
                            }
                        }

                        // Transition to idle state
                        update_recording_state(&app_for_empty, RecordingState::Idle, None);
                    });

                    return;
                }

                // Backend handles the complete flow
                let app_for_process = app_for_task.clone();
                let text_for_process = text.clone();
                let model_for_process = model_name_clone.clone();

                tokio::spawn(async move {
                    // 1. Hide pill window FIRST

                    // Get window manager through AppState
                    let app_state = app_for_process.state::<AppState>();
                    if let Some(window_manager) = app_state.get_window_manager() {
                        if let Err(e) = window_manager.hide_pill_window().await {
                            log::error!("Failed to hide pill window: {}", e);
                        }
                    } else {
                        log::error!("WindowManager not initialized");
                    }

                    // 2. Wait for pill to be fully hidden and system to stabilize
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                    // 3. NOW handle text insertion - pill is gone, system is stable
                    // Always insert text at cursor position (this also copies to clipboard)
                    match crate::commands::text::insert_text(text_for_process.clone()).await {
                        Ok(_) => log::debug!("Text inserted at cursor successfully"),
                        Err(e) => log::error!("Failed to insert text: {}", e),
                    }

                    // 5. Save transcription to history
                    match save_transcription(
                        app_for_process.clone(),
                        text_for_process,
                        model_for_process,
                    )
                    .await
                    {
                        Ok(_) => {
                            // Emit history-updated event to refresh UI
                            let _ = emit_to_window(&app_for_process, "main", "history-updated", ());
                        }
                        Err(e) => log::error!("Failed to save transcription: {}", e),
                    }

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
    // Check license status before saving
    log::info!("[Save] Checking license status before save_transcription");
    let license_status = check_license_status_internal(&app).await?;
    if matches!(
        license_status.status,
        LicenseState::Expired | LicenseState::None
    ) {
        return Err("License required to save transcriptions".to_string());
    }

    // Save transcription to store with current timestamp
    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;

    let timestamp = chrono::Utc::now().to_rfc3339();
    store.set(
        &timestamp,
        serde_json::json!({
            "text": text,
            "model": model,
            "timestamp": timestamp
        }),
    );

    store
        .save()
        .map_err(|e| format!("Failed to save transcription: {}", e))?;

    // Emit event to main window to notify that history was updated
    let _ = emit_to_window(&app, "main", "history-updated", ());

    log::info!("Saved transcription with {} characters", text.len());
    Ok(())
}

#[tauri::command]
pub async fn get_transcription_history(
    app: AppHandle,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    // Check license status before accessing history
    log::info!("[History] Checking license status before get_transcription_history");
    let license_status = check_license_status_internal(&app).await?;
    if matches!(
        license_status.status,
        LicenseState::Expired | LicenseState::None
    ) {
        return Err("License required to access transcription history".to_string());
    }

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
pub async fn transcribe_audio(
    app: AppHandle,
    audio_data: Vec<u8>,
    model_name: String,
) -> Result<String, String> {
    // Check license status before allowing transcription
    log::info!("[Transcribe] Checking license status before transcribe_audio");
    let license_status = check_license_status_internal(&app).await?;
    if matches!(
        license_status.status,
        LicenseState::Expired | LicenseState::None
    ) {
        log::error!("Cannot transcribe - license required");
        return Err(
            "License required to transcribe. Please purchase a license or start a free trial."
                .to_string(),
        );
    }

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

    // Get model path
    let whisper_manager = app.state::<AsyncRwLock<WhisperManager>>();
    let model_path = whisper_manager
        .read()
        .await
        .get_model_path(&model_name)
        .ok_or("Model not found")?;

    // Get language and translation settings
    let store = app.store("settings").map_err(|e| e.to_string())?;
    let language = {
        let lang = store
            .get("language")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "en".to_string());

        // Validate using centralized function
        validate_language(Some(&lang))
    };

    let translate_to_english = store
        .get("translate_to_english")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    log::info!("[LANGUAGE] transcribe_audio using language: {}, translate: {}", language, translate_to_english);

    // Transcribe (cached)
    let transcriber = {
        let cache_state = app.state::<AsyncMutex<TranscriberCache>>();
        let mut cache = cache_state.lock().await;
        cache.get_or_create(&model_path)?
    };

    let text = transcriber.transcribe_with_translation(&temp_path, Some(&language), translate_to_english)?;

    // Clean up
    if let Err(e) = std::fs::remove_file(&temp_path) {
        log::warn!("Failed to remove test audio file: {}", e);
    }

    Ok(text)
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
            update_recording_state(&app, RecordingState::Error, Some("Transcription cancelled".to_string()));
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
    // Check license status before deleting
    log::info!("[Delete] Checking license status before delete_transcription_entry");
    let license_status = check_license_status_internal(&app).await?;
    if matches!(
        license_status.status,
        LicenseState::Expired | LicenseState::None
    ) {
        return Err("License required to delete transcriptions".to_string());
    }

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

    log::info!("Cleared all transcription entries: {} items", count);
    Ok(())
}
