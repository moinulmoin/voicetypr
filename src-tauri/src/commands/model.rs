use crate::whisper::manager::{ModelInfo, WhisperManager};
use crate::emit_to_window;
use std::collections::HashMap;
use tauri::async_runtime::Mutex;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    model_name: String,
    state: State<'_, Mutex<WhisperManager>>,
) -> Result<(), String> {
    // Validate model name
    let valid_models = [
        "tiny",
        "base",
        "small",
        "medium",
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
            
            if let Err(e) = emit_to_window(
                &app_handle,
                "main",
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
        log::info!("Download attempt {} of {} for model: {}", attempt, MAX_RETRIES, model_name);
        
        let manager = state.lock().await;
        let progress_tx_clone = progress_tx.clone();
        download_result = manager
            .download_model(&model_name, move |downloaded, total| {
                let _ = progress_tx_clone.send((downloaded, total));
            })
            .await;
        
        drop(manager); // Release lock before sleep
        
        match &download_result {
            Ok(_) => {
                log::info!("Download succeeded on attempt {}", attempt);
                break;
            }
            Err(e) => {
                if attempt < MAX_RETRIES {
                    log::warn!("Download attempt {} failed: {}. Retrying in {}ms...", 
                              attempt, e, RETRY_DELAY_MS);
                    
                    // Notify UI about retry
                    if let Err(e) = emit_to_window(
                        &app,
                        "main",
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

    match download_result {
        Ok(_) => {
            log::info!("Download completed for model: {}", model_name);

            // Refresh the downloaded status in WhisperManager
            let mut manager = state.lock().await;
            manager.refresh_downloaded_status();
            
            // Models are refreshed in WhisperManager, no need for duplicate state

            if let Err(e) = emit_to_window(&app, "main", "model-downloaded", &model_name) {
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
) -> Result<HashMap<String, ModelInfo>, String> {
    // Force refresh before returning status
    let mut manager = state.lock().await;
    manager.refresh_downloaded_status();
    let models = manager.get_models_status();
    
    // Models are already stored in WhisperManager, no duplicate state needed
    
    Ok(models)
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
pub async fn preload_model(
    app: AppHandle,
    model_name: String,
    state: State<'_, Mutex<WhisperManager>>,
) -> Result<(), String> {
    use crate::whisper::cache::TranscriberCache;
    use tauri::async_runtime::Mutex as AsyncMutex;
    
    log::info!("Preloading model: {}", model_name);
    
    // Get model path
    let model_path = {
        let manager = state.lock().await;
        manager.get_model_path(&model_name)
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
