use crate::commands::license::check_license_status_internal;
use crate::emit_to_all;
use crate::license::LicenseState;
use crate::whisper::manager::{ModelInfo, WhisperManager};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tauri::async_runtime::Mutex;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    model_name: String,
    state: State<'_, Mutex<WhisperManager>>,
    active_downloads: State<'_, Arc<StdMutex<HashMap<String, Arc<AtomicBool>>>>>,
) -> Result<(), String> {
    // Validate model name
    let valid_models = [
        "base.en",
        "large-v3",
        "large-v3-q5_0",
        "large-v3-turbo",
        "large-v3-turbo-q5_0",
    ];
    if !valid_models.contains(&model_name.as_str()) {
        return Err(format!("Invalid model name: {}", model_name));
    }

    log::info!("Starting download for model: {}", model_name);
    let app_handle = app.clone();

    // Create cancellation flag for this download
    let cancel_flag = Arc::new(AtomicBool::new(false));
    {
        let mut downloads = active_downloads.lock().unwrap();
        downloads.insert(model_name.clone(), cancel_flag.clone());
    }

    let model_name_clone = model_name.clone();

    // Create an async-safe wrapper for progress callback
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<(u64, u64)>();

    // Spawn task to handle progress updates
    let progress_handle = tokio::spawn(async move {
        while let Some((downloaded, total)) = progress_rx.recv().await {
            let progress = (downloaded as f64 / total as f64) * 100.0;
            log::debug!(
                "Download progress for {}: {:.1}%",
                &model_name_clone,
                progress
            );

            // Progress is already being emitted via events, no need for state storage

            if let Err(e) = emit_to_all(
                &app_handle,
                "download-progress",
                serde_json::json!({
                    "model": &model_name_clone,
                    "downloaded": downloaded,
                    "total": total,
                    "progress": progress
                }),
            ) {
                log::warn!("Failed to emit download progress: {}", e);
            }
        }
    });

    // Execute download with retry logic
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY_MS: u64 = 2000;

    let mut download_result = Err("No attempt made".to_string());

    for attempt in 1..=MAX_RETRIES {
        // Check if download was cancelled
        if cancel_flag.load(Ordering::Relaxed) {
            log::info!("Download cancelled for model: {}", model_name);
            download_result = Err("Download cancelled by user".to_string());
            break;
        }

        log::info!(
            "Download attempt {} of {} for model: {}",
            attempt,
            MAX_RETRIES,
            model_name
        );

        let manager = state.lock().await;
        let progress_tx_clone = progress_tx.clone();
        download_result = manager
            .download_model(
                &model_name,
                Some(cancel_flag.clone()),
                move |downloaded, total| {
                    let _ = progress_tx_clone.send((downloaded, total));
                },
            )
            .await;

        drop(manager); // Release lock before sleep

        match &download_result {
            Ok(_) => {
                log::info!("Download succeeded on attempt {}", attempt);
                break;
            }
            Err(e) => {
                if attempt < MAX_RETRIES {
                    log::warn!(
                        "Download attempt {} failed: {}. Retrying in {}ms...",
                        attempt,
                        e,
                        RETRY_DELAY_MS
                    );

                    // Notify UI about retry
                    if let Err(e) = emit_to_all(
                        &app,
                        "download-retry",
                        serde_json::json!({
                            "model": &model_name,
                            "attempt": attempt,
                            "max_attempts": MAX_RETRIES,
                            "error": e.to_string()
                        }),
                    ) {
                        log::warn!("Failed to emit download-retry event: {}", e);
                    }

                    tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
                } else {
                    log::error!("Download failed after {} attempts: {}", MAX_RETRIES, e);
                }
            }
        }
    }

    // Ensure progress handler completes
    let _ = progress_handle.await;

    // Clean up the cancellation flag
    {
        let mut downloads = active_downloads.lock().unwrap();
        downloads.remove(&model_name);
    }

    match download_result {
        Err(ref e) if e.contains("cancelled") => {
            // Emit download-cancelled event
            if let Err(e) = emit_to_all(&app, "download-cancelled", &model_name) {
                log::warn!("Failed to emit download-cancelled event: {}", e);
            }
            Err(e.clone())
        }
        Ok(_) => {
            log::info!("Download completed for model: {}", model_name);

            // Refresh the downloaded status in WhisperManager with retries
            let mut retry_count = 0;
            const MAX_RETRIES: u32 = 3;
            let mut model_actually_downloaded = false;
            
            while retry_count < MAX_RETRIES {
                {
                    let mut manager = state.lock().await;
                    log::info!("[VERIFY] Attempt {} for model '{}'", retry_count + 1, model_name);
                    
                    // Check if the model exists
                    let models_dir = manager.get_models_dir();
                    let expected_path = models_dir.join(format!("{}.bin", model_name));
                    log::info!("[VERIFY] Looking for file at: {:?}", expected_path);
                    log::info!("[VERIFY] File exists: {}", expected_path.exists());
                    if expected_path.exists() {
                        if let Ok(metadata) = std::fs::metadata(&expected_path) {
                            log::info!("[VERIFY] File size: {} bytes", metadata.len());
                        }
                    }
                    
                    manager.refresh_downloaded_status();
                    
                    // Verify the model is actually marked as downloaded
                    let models = manager.get_models_status();
                    
                    // Log all models for debugging
                    for (name, info) in &models {
                        log::info!("[VERIFY] Model '{}': downloaded={}", name, info.downloaded);
                    }
                    
                    model_actually_downloaded = models.get(&model_name).map(|m| m.downloaded).unwrap_or(false);
                    log::info!("[VERIFY] Model '{}' final status: {}", model_name, model_actually_downloaded);
                } // Drop the lock
                
                if model_actually_downloaded {
                    log::info!("Model {} confirmed as downloaded after {} attempts", model_name, retry_count + 1);
                    break;
                }
                
                retry_count += 1;
                if retry_count < MAX_RETRIES {
                    log::warn!("Model {} not found in directory after refresh, retry {}/{}", model_name, retry_count, MAX_RETRIES);
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
            
            if !model_actually_downloaded {
                log::error!("Model {} was downloaded but not found in models directory after {} retries!", model_name, MAX_RETRIES);
                return Err(format!("Model {} file not detected after download completed", model_name));
            }

            // Only emit the event if the model is confirmed as downloaded
            log::info!("Emitting model-downloaded event for {}", model_name);
            if let Err(e) = emit_to_all(&app, "model-downloaded", serde_json::json!({
                "model": model_name
            })) {
                log::warn!("Failed to emit model-downloaded event: {}", e);
            }
            Ok(())
        }
        Err(e) => {
            log::error!("Download failed for model {}: {}", model_name, e);

            // Progress tracking is event-based, no state cleanup needed

            Err(e)
        }
    }
}

#[tauri::command]
pub async fn get_model_status(
    state: State<'_, Mutex<WhisperManager>>,
) -> Result<Vec<(String, ModelInfo)>, String> {
    // Force refresh before returning status
    let mut manager = state.lock().await;
    log::info!("[GET_MODEL_STATUS] Refreshing downloaded status...");
    manager.refresh_downloaded_status();
    let models = manager.get_models_status();

    // Convert HashMap to Vec and sort by accuracy (ascending)
    let mut models_vec: Vec<(String, ModelInfo)> = models.into_iter().collect();
    models_vec.sort_by(|a, b| a.1.accuracy_score.cmp(&b.1.accuracy_score));
    
    // Log what we're returning
    log::info!("[GET_MODEL_STATUS] Returning {} models:", models_vec.len());
    for (name, info) in &models_vec {
        log::info!("[GET_MODEL_STATUS]   Model '{}': downloaded={}", name, info.downloaded);
    }

    Ok(models_vec)
}

#[tauri::command]
pub async fn delete_model(
    model_name: String,
    state: State<'_, Mutex<WhisperManager>>,
) -> Result<(), String> {
    let mut manager = state.lock().await;
    manager.delete_model_file(&model_name)
}

#[tauri::command]
pub async fn list_downloaded_models(
    state: State<'_, Mutex<WhisperManager>>,
) -> Result<Vec<String>, String> {
    let manager = state.lock().await;
    Ok(manager.list_downloaded_files())
}

#[tauri::command]
pub async fn cancel_download(
    model_name: String,
    active_downloads: State<'_, Arc<StdMutex<HashMap<String, Arc<AtomicBool>>>>>,
) -> Result<(), String> {
    log::info!("Cancelling download for model: {}", model_name);

    // Set the cancellation flag
    {
        let downloads = active_downloads.lock().unwrap();
        if let Some(cancel_flag) = downloads.get(&model_name) {
            cancel_flag.store(true, Ordering::Relaxed);
            log::info!("Set cancellation flag for model: {}", model_name);
        } else {
            return Err(format!(
                "No active download found for model: {}",
                model_name
            ));
        }
    }

    // The download loop will handle cleanup when it detects the cancellation flag

    log::info!("Download cancelled for model: {}", model_name);
    Ok(())
}

#[tauri::command]
pub async fn preload_model(
    app: AppHandle,
    model_name: String,
    state: State<'_, Mutex<WhisperManager>>,
) -> Result<(), String> {
    use crate::whisper::cache::TranscriberCache;
    use tauri::async_runtime::Mutex as AsyncMutex;

    // Check license status before preloading
    log::info!("[Preload] Checking license status before preload_model");
    let license_status = check_license_status_internal(&app).await?;
    if matches!(
        license_status.status,
        LicenseState::Expired | LicenseState::None
    ) {
        return Err("License required to preload models".to_string());
    }

    log::info!("Preloading model: {}", model_name);

    // Get model path
    let model_path = {
        let manager = state.lock().await;
        manager
            .get_model_path(&model_name)
            .ok_or(format!("Model '{}' not found", model_name))?
    };

    // Load into cache
    let cache_state = app.state::<AsyncMutex<TranscriberCache>>();
    let mut cache = cache_state.lock().await;

    // This will load the model and cache it
    cache.get_or_create(&model_path)?;

    log::info!("Model '{}' preloaded successfully", model_name);
    Ok(())
}
