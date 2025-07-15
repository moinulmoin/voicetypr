use tauri::{AppHandle, Manager, State};

use crate::audio::recorder::AudioRecorder;
use crate::whisper::cache::TranscriberCache;
use crate::whisper::manager::WhisperManager;
use crate::{emit_to_window, update_recording_state, AppState, RecordingState};
use serde_json;
use std::sync::Mutex;
use tauri::async_runtime::Mutex as AsyncMutex;
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
        "large-v3"
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
    available_models.first().unwrap().clone()
}

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, RecorderState>,
) -> Result<(), String> {
    // Check if we have any models BEFORE starting to record
    let whisper_manager = app.state::<AsyncMutex<WhisperManager>>();
    let available_models = whisper_manager.lock().await.get_models_status();
    let has_models = available_models.iter().any(|(_, info)| info.downloaded);
    
    if !has_models {
        log::error!("Cannot start recording - no models downloaded");
        
        // Emit error event with guidance
        let _ = emit_to_window(&app, "main", "no-models-error", 
            serde_json::json!({
                "title": "No Speech Recognition Models",
                "message": "Please download at least one model from Settings before recording.",
                "action": "open-settings"
            }));
        
        return Err("No speech recognition models installed. Please download a model first.".to_string());
    }
    
    // Update state to starting
    update_recording_state(&app, RecordingState::Starting, None);
    // Get app data directory for recordings
    let recordings_dir = app.path().app_data_dir()
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
                    update_recording_state(&app, RecordingState::Error, 
                        Some("Microphone initialization failed".to_string()));
                    
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

                while let Ok(level) = audio_level_rx.recv() {
                    // Throttle events to avoid overwhelming the UI
                    if last_emit.elapsed() >= emit_interval {
                        // Only emit to pill window - main window doesn't need audio levels
                        let _ = emit_to_window(&app_for_levels, "pill", "audio-level", level);
                        last_emit = std::time::Instant::now();
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
                        let _ = emit_to_window(&app, "main", "pill-widget-error", 
                            "Recording indicator unavailable. Recording is still active.");
                    }
                }
            }
        }
        Err(e) => {
            log::warn!("Could not access settings to check pill widget preference: {}", e);
            // Continue without pill widget - recording still works
        }
    }

    // Also emit legacy event for compatibility
    let _ = emit_to_window(&app, "pill", "recording-started", ());
    log::debug!("Recording started successfully");

    // Set up 30-second timeout
    let app_clone = app.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;

        // Check if still recording and emit timeout event
        if let Ok(guard) = app_clone.state::<RecorderState>().inner().0.lock() {
            if guard.is_recording() {
                // Emit timeout warning to pill window
                let _ = emit_to_window(&app_clone, "pill", "recording-timeout", ());
            }
        }
    });

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
            // Don't error - just return empty result
            return Ok("".to_string());
        }

        let stop_message = recorder.stop_recording()?;
        log::info!("{}", stop_message);

        // Emit event if recording was stopped due to silence
        if stop_message.contains("silence") {
            let _ = emit_to_window(&app, "pill", "recording-stopped-silence", ());
        }
    } // MutexGuard dropped here BEFORE any await

    // Get the audio file path
    let app_state = app.state::<AppState>();
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
        },
        None => {
            log::warn!("No audio file found - no recording was made");
            return Ok("".to_string());
        }
    };

    // Get current model from settings
    let store = app.store("settings").map_err(|e| e.to_string())?;

    // Get available models
    let whisper_manager = app.state::<AsyncMutex<WhisperManager>>();
    let available_models = whisper_manager.lock().await.get_models_status();
    let downloaded_models: Vec<String> = available_models
        .iter()
        .filter(|(_, info)| info.downloaded)
        .map(|(name, _)| name.clone())
        .collect();

    log::debug!("Downloaded models: {:?}", downloaded_models);
    
    // STOP HERE if no models are downloaded - can't transcribe without models!
    if downloaded_models.is_empty() {
        log::error!("No models downloaded - cannot transcribe");
        update_recording_state(&app, RecordingState::Error, 
            Some("No speech recognition models installed".to_string()));
        
        // Clean up the recording
        if let Err(e) = std::fs::remove_file(&audio_path) {
            log::warn!("Failed to remove audio file: {}", e);
        }
        
        // Tell user they MUST download a model
        let _ = emit_to_window(&app, "pill", "no-models-error", 
            serde_json::json!({
                "title": "No Models Installed",
                "message": "Please download at least one speech recognition model from Settings to use VoiceTypr.",
                "action": "open-settings"
            }));
        
        return Err("No speech recognition models installed. Please download a model from Settings.".to_string());
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
            let _ = emit_to_window(&app, "pill", "model-fallback", 
                serde_json::json!({
                    "requested": configured_model,
                    "fallback": fallback_model
                }));
            
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
        .lock()
        .await
        .get_model_path(&model_name)
        .ok_or(format!("Model '{}' path not found", model_name))?;

    let model_name_clone = model_name.clone();

    let language = store
        .get("language")
        .and_then(|v| v.as_str().map(|s| s.to_string()));

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
            
            result = transcriber.transcribe(&audio_path_clone, language.as_deref());
            
            match &result {
                Ok(_) => {
                    if attempt > 1 {
                        log::info!("Transcription succeeded on attempt {}", attempt);
                    }
                    break;
                }
                Err(e) => {
                    if attempt < MAX_RETRIES {
                        log::warn!("Transcription attempt {} failed: {}. Retrying in {}ms...", 
                                  attempt, e, RETRY_DELAY_MS);
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
                    update_recording_state(&app_for_task, RecordingState::Idle, None);
                    return;
                }
                
                log::debug!("Transcription successful, {} chars", text.len());
                
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
                    match save_transcription(app_for_process.clone(), text_for_process, model_for_process).await {
                        Ok(_) => {
                            // Emit history-updated event to refresh UI
                            let _ = emit_to_window(&app_for_process, "main", "history-updated", ());
                        },
                        Err(e) => log::error!("Failed to save transcription: {}", e),
                    }
                    
                    // 6. Transition to idle state
                    update_recording_state(&app_for_process, RecordingState::Idle, None);
                });
            }
            Err(e) => {
                // Update state to error
                update_recording_state(&app_for_task, RecordingState::Error, Some(e.clone()));

                // Also emit legacy event to pill window
                let _ = emit_to_window(&app_for_task, "pill", "transcription-error", e);
                
                // Transition back to Idle after a delay
                // This ensures we don't get stuck in Error state
                let app_for_reset = app_for_task.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    log::debug!("Resetting from Error to Idle state after transcription failure");
                    update_recording_state(&app_for_reset, RecordingState::Idle, None);
                });
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
    // Save transcription to store with current timestamp
    let store = app.store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;
    
    let timestamp = chrono::Utc::now().to_rfc3339();
    store.set(
        &timestamp,
        serde_json::json!({
            "text": text,
            "model": model,
            "timestamp": timestamp
        })
    );
    
    store.save()
        .map_err(|e| format!("Failed to save transcription: {}", e))?;
    
    // Emit event to main window to notify that history was updated
    let _ = emit_to_window(&app, "main", "history-updated", ());
    
    log::info!("Saved transcription with {} characters", text.len());
    Ok(())
}

#[tauri::command]
pub async fn get_transcription_history(app: AppHandle, limit: Option<usize>) -> Result<Vec<serde_json::Value>, String> {
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
    // Save audio data to app data directory
    let recordings_dir = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("recordings");
    
    // Ensure directory exists
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|e| format!("Failed to create recordings directory: {}", e))?;
        
    let temp_path = recordings_dir.join("temp_audio.wav");

    std::fs::write(&temp_path, audio_data).map_err(|e| e.to_string())?;

    // Get model path
    let whisper_manager = app.state::<AsyncMutex<WhisperManager>>();
    let model_path = whisper_manager
        .lock()
        .await
        .get_model_path(&model_name)
        .ok_or("Model not found")?;

    // Transcribe (cached)
    let transcriber = {
        let cache_state = app.state::<AsyncMutex<TranscriberCache>>();
        let mut cache = cache_state.lock().await;
        cache.get_or_create(&model_path)?
    };

    let text = transcriber.transcribe(&temp_path, None)?;

    // Clean up
    if let Err(e) = std::fs::remove_file(&temp_path) {
        log::warn!("Failed to remove test audio file: {}", e);
    }

    Ok(text)
}

#[tauri::command]
pub async fn cancel_recording(app: AppHandle) -> Result<(), String> {
    log::info!("Cancel recording requested");
    
    // Request cancellation
    let app_state = app.state::<AppState>();
    app_state.request_cancellation();
    
    // Stop recording if active
    let recorder_state = app.state::<RecorderState>();
    let is_recording = {
        let guard = recorder_state.inner().0.lock()
            .map_err(|e| format!("Failed to acquire recorder lock: {}", e))?;
        guard.is_recording()
    };
    
    if is_recording {
        // Stop the recording
        stop_recording(app.clone(), recorder_state).await?;
    }
    
    Ok(())
}

#[tauri::command]
pub async fn delete_transcription_entry(app: AppHandle, timestamp: String) -> Result<(), String> {
    let store = app.store("transcriptions")
        .map_err(|e| format!("Failed to get transcriptions store: {}", e))?;
    
    // Delete the entry
    store.delete(&timestamp);
    
    // Save the store
    store.save()
        .map_err(|e| format!("Failed to save store after deletion: {}", e))?;
    
    // Emit event to update UI
    let _ = emit_to_window(&app, "main", "history-updated", ());
    
    log::info!("Deleted transcription entry: {}", timestamp);
    Ok(())
}
