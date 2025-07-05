use tauri::{AppHandle, Emitter, State};
use crate::whisper::manager::{WhisperManager, ModelInfo};
use std::collections::HashMap;
use tauri::async_runtime::Mutex;

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    model_name: String,
    state: State<'_, Mutex<WhisperManager>>,
) -> Result<(), String> {
    // Validate model name
    let valid_models = ["tiny", "base", "small", "medium", "large-v3", "large-v3-q5_0", "large-v3-turbo", "large-v3-turbo-q5_0"];
    if !valid_models.contains(&model_name.as_str()) {
        return Err(format!("Invalid model name: {}", model_name));
    }
    
    println!("Starting download for model: {}", model_name);
    let app_handle = app.clone();

    let model_name_clone = model_name.clone();

    // Create an async-safe wrapper for progress callback
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<(u64, u64)>();

    // Spawn task to handle progress updates
    let progress_handle = tokio::spawn(async move {
        while let Some((downloaded, total)) = progress_rx.recv().await {
            let progress = (downloaded as f64 / total as f64) * 100.0;
            println!("Download progress for {}: {:.1}%", &model_name_clone, progress);

            app_handle.emit("download-progress", serde_json::json!({
                "model": &model_name_clone,
                "downloaded": downloaded,
                "total": total,
                "progress": progress
            })).unwrap();
        }
    });

    // Execute download with async-safe callback
    let download_result = {
        let manager = state.lock().await;
        manager.download_model(&model_name, move |downloaded, total| {
            let _ = progress_tx.send((downloaded, total));
        }).await
    };

    // Ensure progress handler completes
    let _ = progress_handle.await;

    match download_result {
        Ok(_) => {
            println!("Download completed for model: {}", model_name);

            // Refresh the downloaded status in WhisperManager
            state.lock().await.refresh_downloaded_status();

            app.emit("model-downloaded", &model_name).unwrap();
            Ok(())
        }
        Err(e) => {
            println!("Download failed for model {}: {}", model_name, e);
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
    Ok(manager.get_models_status())
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