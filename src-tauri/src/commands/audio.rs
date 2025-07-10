use tauri::{AppHandle, Emitter, Manager, State};

use crate::audio::recorder::AudioRecorder;
use crate::whisper::cache::TranscriberCache;
use crate::whisper::manager::WhisperManager;
use crate::{update_recording_state, AppState, RecordingState};
use serde_json;
use std::sync::Mutex;
use tauri::async_runtime::Mutex as AsyncMutex;
use tauri_plugin_store::StoreExt;

// Global audio recorder state
pub struct RecorderState(pub Mutex<AudioRecorder>);

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, RecorderState>,
) -> Result<(), String> {
    // Update state to starting
    update_recording_state(&app, RecordingState::Starting, None);
    // Get temp file path
    let temp_dir = app.path().temp_dir().map_err(|e| e.to_string())?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("Time error: {}", e))?
        .as_secs();
    let audio_path = temp_dir.join(format!("recording_{}.wav", timestamp));

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

        log::info!("Starting recording to: {:?}", audio_path);
        recorder.start_recording(
            audio_path
                .to_str()
                .ok_or_else(|| "Invalid path encoding".to_string())?,
        )?;

        // Verify recording actually started
        if !recorder.is_recording() {
            log::error!("Recording failed to start!");
            return Err("Failed to start recording".to_string());
        }

        // Start audio level monitoring before releasing the lock
        if let Some(audio_level_rx) = recorder.take_audio_level_receiver() {
            let app_for_levels = app.clone();
            // Use a thread instead of tokio spawn for std::sync::mpsc
            std::thread::spawn(move || {
                let mut last_emit = std::time::Instant::now();
                let emit_interval = std::time::Duration::from_millis(33); // ~30fps

                while let Ok(level) = audio_level_rx.recv() {
                    // Throttle events to avoid overwhelming the UI
                    if last_emit.elapsed() >= emit_interval {
                        let _ = app_for_levels.emit("audio-level", level);
                        last_emit = std::time::Instant::now();
                    }
                }
            });
        }
    } // MutexGuard dropped here

    // Now perform async operations after mutex is released

    // Update state to recording
    update_recording_state(&app, RecordingState::Recording, None);

    // Show pill widget if enabled
    if let Ok(store) = app.store("settings") {
        let show_pill = store
            .get("show_pill_widget")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if show_pill {
            if let Err(e) = crate::commands::window::show_pill_widget(app.clone()).await {
                log::warn!("Failed to show pill widget: {}", e);
            }
        }
    }

    // Also emit legacy event for compatibility
    let _ = app.emit("recording-started", ());
    log::info!("Recording started successfully");

    // Set up 30-second timeout
    let app_clone = app.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;

        // Check if still recording and emit timeout event
        if let Ok(guard) = app_clone.state::<RecorderState>().inner().0.lock() {
            if guard.is_recording() {
                // Emit timeout warning
                let _ = app_clone.emit("recording-timeout", ());
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
            let _ = app.emit("recording-stopped-silence", ());
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
        Some(path) => path,
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

    // Smart model selection
    let configured_model = store
        .get("current_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .filter(|s| !s.is_empty()); // Treat empty string as no configured model

    let model_name = if let Some(configured_model) = configured_model {
        // Use configured model if it exists and is downloaded
        if downloaded_models.contains(&configured_model) {
            configured_model
        } else if downloaded_models.len() == 1 {
            // If only one model is downloaded, use it
            log::info!(
                "Configured model '{}' not found, using only available model: {}",
                configured_model,
                downloaded_models[0]
            );
            downloaded_models[0].clone()
        } else if downloaded_models.is_empty() {
            return Err("No models downloaded. Please download a model first.".to_string());
        } else {
            // Multiple models available but configured one not found
            log::info!(
                "Configured model '{}' not found, using first available: {}",
                configured_model,
                downloaded_models[0]
            );
            downloaded_models[0].clone()
        }
    } else {
        // No configured model or empty string
        if downloaded_models.len() == 1 {
            // If only one model is downloaded, use it
            log::info!(
                "No model configured, using only available model: {}",
                downloaded_models[0]
            );
            downloaded_models[0].clone()
        } else if downloaded_models.is_empty() {
            return Err("No models downloaded. Please download a model first.".to_string());
        } else {
            // Multiple models, pick first one
            log::info!(
                "No model configured, using first available: {}",
                downloaded_models[0]
            );
            downloaded_models[0].clone()
        }
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

    let _auto_insert = store
        .get("auto_insert")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // clone for move into task
    let audio_path_clone = audio_path.clone();
    let model_path_clone = model_path.clone();

    // Spawn and track the transcription task
    let app_for_task = app.clone();
    let task_handle = tokio::spawn(async move {
        // Update state to transcribing
        update_recording_state(&app_for_task, RecordingState::Transcribing, None);
        // Also emit legacy event
        let _ = app_for_task.emit("transcription-started", ());

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

                    // Also emit legacy event
                    let _ = app_for_task.emit("transcription-error", e);
                    return;
                }
            }
        };

        let result = transcriber.transcribe(&audio_path_clone, language.as_deref());

        // Clean up temp file regardless of outcome
        std::fs::remove_file(&audio_path_clone).ok();

        match result {
            Ok(text) => {
                // Update state back to idle
                update_recording_state(&app_for_task, RecordingState::Idle, None);

                // Don't save here - let the pill window save after paste/clipboard
                // This ensures we save the exact text that was pasted

                // Emit transcription complete event
                // The pill window will handle auto-insert, clipboard, and saving
                let _ = app_for_task.emit(
                    "transcription-complete",
                    serde_json::json!({
                        "text": text,
                        "model": model_name_clone
                    }),
                );
            }
            Err(e) => {
                // Update state to error
                update_recording_state(&app_for_task, RecordingState::Error, Some(e.clone()));

                // Also emit legacy event
                let _ = app_for_task.emit("transcription-error", e);
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
    
    // Emit event to notify that history was updated
    let _ = app.emit("history-updated", ());
    
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
    // Save audio data to temp file
    let temp_dir = app.path().temp_dir().map_err(|e| e.to_string())?;
    let temp_path = temp_dir.join("temp_audio.wav");

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
    std::fs::remove_file(temp_path).ok();

    Ok(text)
}
