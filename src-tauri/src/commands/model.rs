use crate::commands::license::check_license_status_internal;
use crate::emit_to_all;
use crate::license::LicenseState;
use crate::parakeet::{ParakeetManager, ParakeetModelStatus};
use crate::secure_store;
use crate::utils::onboarding_logger;
#[cfg(debug_assertions)]
use crate::utils::system_monitor;
use crate::whisper::manager::{ModelInfo, WhisperManager};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Instant;
use tauri::async_runtime::RwLock;
use tauri::{AppHandle, Emitter, Manager, State};

type ActiveDownloadsState<'a> = State<'a, Arc<StdMutex<HashMap<String, Arc<AtomicBool>>>>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelEngine {
    Whisper,
    Parakeet,
}

impl ModelEngine {
    fn as_str(&self) -> &'static str {
        match self {
            ModelEngine::Whisper => "whisper",
            ModelEngine::Parakeet => "parakeet",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct DownloadTarget {
    engine: ModelEngine,
    size_bytes: u64,
}

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    model_name: String,
    whisper_state: State<'_, RwLock<WhisperManager>>,
    parakeet_manager: State<'_, ParakeetManager>,
    active_downloads: ActiveDownloadsState<'_>,
) -> Result<(), String> {
    let download_start = Instant::now();

    let download_target =
        identify_download_target(&model_name, &whisper_state, &parakeet_manager).await?;

    log::info!("Starting download for model: {}", model_name);

    // Monitor system resources at download start
    #[cfg(debug_assertions)]
    system_monitor::log_resources_before_operation("MODEL_DOWNLOAD");

    // Log to onboarding if in onboarding context
    let model_size_mb = download_target.size_bytes / (1024 * 1024); // bytes → MB
    onboarding_logger::with_onboarding_logger(|logger| {
        logger.log_model_download_start(&model_name, model_size_mb);
    });

    let app_handle = app.clone();

    // Create cancellation flag for this download
    let cancel_flag = Arc::new(AtomicBool::new(false));
    {
        match active_downloads.lock() {
            Ok(mut downloads) => {
                downloads.insert(model_name.clone(), cancel_flag.clone());
            }
            Err(e) => {
                log::error!("Failed to lock active downloads for inserting: {}", e);
                return Err("Failed to initialize download tracking".to_string());
            }
        }
    }

    let model_name_clone = model_name.clone();

    // Create an async-safe wrapper for progress callback
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<(u64, u64)>();

    // Spawn task to handle progress updates
    let progress_handle = tokio::spawn(async move {
        let mut verification_emitted = false;

        while let Some((downloaded, total)) = progress_rx.recv().await {
            let progress = (downloaded as f64 / total as f64) * 100.0;
            log::debug!(
                "Download progress for {}: {:.1}%",
                &model_name_clone,
                progress
            );

            // Log to onboarding if active
            onboarding_logger::with_onboarding_logger(|logger| {
                logger.log_model_download_progress(&model_name_clone, progress as u8);
            });

            // Progress is already being emitted via events, no need for state storage

            if let Err(e) = emit_to_all(
                &app_handle,
                "download-progress",
                serde_json::json!({
                    "model": &model_name_clone,
                    "engine": download_target.engine.as_str(),
                    "downloaded": downloaded,
                    "total": total,
                    "progress": progress
                }),
            ) {
                log::warn!("Failed to emit download progress: {}", e);
            }

            // When download reaches 100%, emit verification event
            if progress >= 100.0 && !verification_emitted {
                verification_emitted = true;
                log::info!(
                    "Download complete, starting verification for model: {}",
                    &model_name_clone
                );
                if let Err(e) = emit_to_all(
                    &app_handle,
                    "model-verifying",
                    serde_json::json!({
                        "model": &model_name_clone,
                        "engine": download_target.engine.as_str()
                    }),
                ) {
                    log::warn!("Failed to emit model-verifying event: {}", e);
                }
            }
        }
    });

    // Execute download (no retry - user can click download again if it fails)
    let download_result = if cancel_flag.load(Ordering::Relaxed) {
        log::info!("Download cancelled for model: {}", model_name);
        Err("Download cancelled by user".to_string())
    } else {
        log::info!("Starting download for model: {}", model_name);

        let progress_tx_clone = progress_tx.clone();
        let result = match download_target.engine {
            ModelEngine::Whisper => {
                let manager = whisper_state.read().await;
                let res = manager
                    .download_model(
                        &model_name,
                        Some(cancel_flag.clone()),
                        move |downloaded, total| {
                            let _ = progress_tx_clone.send((downloaded, total));
                        },
                    )
                    .await;
                drop(manager);
                res
            }
            ModelEngine::Parakeet => {
                parakeet_manager
                    .download_model(
                        &app,
                        &model_name,
                        Some(cancel_flag.clone()),
                        move |downloaded, total| {
                            let _ = progress_tx_clone.send((downloaded, total));
                        },
                    )
                    .await
            }
        };

        match &result {
            Ok(_) => {
                log::info!(
                    "Download succeeded for model {} (engine={})",
                    model_name,
                    download_target.engine.as_str()
                );
            }
            Err(e) => {
                log::error!(
                    "Download failed for model {} (engine={}): {}",
                    model_name,
                    download_target.engine.as_str(),
                    e
                );
            }
        }

        result
    };

    // Close the progress channel to signal completion
    drop(progress_tx);

    // Ensure progress handler completes
    let _ = progress_handle.await;

    // Clean up the cancellation flag
    {
        match active_downloads.lock() {
            Ok(mut downloads) => {
                downloads.remove(&model_name);
            }
            Err(e) => {
                log::warn!("Failed to lock active downloads for cleanup: {}", e);
                // Continue despite cleanup failure
            }
        }
    }

    log::info!("Processing download result for model: {}", model_name);
    match download_result {
        Err(ref e) if e.contains("cancelled") => {
            log::info!("Download was cancelled");
            // Emit download-cancelled event
            if let Err(e) = emit_to_all(
                &app,
                "download-cancelled",
                serde_json::json!({
                    "model": model_name,
                    "engine": download_target.engine.as_str()
                }),
            ) {
                log::warn!("Failed to emit download-cancelled event: {}", e);
            }
            Err(e.clone())
        }
        Ok(_) => {
            log::info!("Download completed successfully for model: {}", model_name);

            // Monitor system resources after download completion
            #[cfg(debug_assertions)]
            system_monitor::log_resources_after_operation(
                "MODEL_DOWNLOAD",
                download_start.elapsed().as_millis() as u64,
            );

            // Log to onboarding if active
            onboarding_logger::with_onboarding_logger(|logger| {
                // Calculate duration if possible
                logger.log_model_download_complete(&model_name, 0); // TODO: track actual duration
            });

            // Refresh/verify downloaded status
            match download_target.engine {
                ModelEngine::Whisper => {
                    let mut manager = whisper_state.write().await;
                    manager.refresh_downloaded_status();
                }
                ModelEngine::Parakeet => {
                    // Verify Parakeet reports the requested model as downloaded
                    let verified = parakeet_manager
                        .list_models()
                        .into_iter()
                        .any(|m| m.name == model_name && m.downloaded);

                    if !verified {
                        let msg = format!(
                            "Parakeet sidecar did not confirm '{}' as downloaded. Please try again.",
                            model_name
                        );
                        log::warn!("{}", msg);
                        // Emit download-error event and return Err
                        if let Err(emit_err) = emit_to_all(
                            &app,
                            "download-error",
                            serde_json::json!({
                                "model": model_name,
                                "engine": download_target.engine.as_str(),
                                "error": msg
                            }),
                        ) {
                            log::warn!("Failed to emit download-error event: {}", emit_err);
                        }
                        return Err("verification_failed".to_string());
                    }
                }
            }

            // Emit success event after verification
            log::info!("Emitting model-downloaded event for {}", model_name);
            if let Err(e) = emit_to_all(
                &app,
                "model-downloaded",
                serde_json::json!({
                    "model": model_name,
                    "engine": download_target.engine.as_str()
                }),
            ) {
                log::warn!("Failed to emit model-downloaded event: {}", e);
            }

            // Refresh tray menu so the new model appears in the tray immediately
            if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
                log::warn!("Failed to update tray menu after model download: {}", e);
            }

            Ok(())
        }
        Err(e) => {
            log::error!("Download failed for model {}: {}", model_name, e);

            // Log to onboarding if active
            onboarding_logger::with_onboarding_logger(|logger| {
                logger.log_model_download_failed(&model_name, &e);
            });

            // Emit download-error event
            if let Err(emit_err) = emit_to_all(
                &app,
                "download-error",
                serde_json::json!({
                    "model": model_name,
                    "engine": download_target.engine.as_str(),
                    "error": e.to_string()
                }),
            ) {
                log::warn!("Failed to emit download-error event: {}", emit_err);
            }

            // Progress tracking is event-based, no state cleanup needed

            Err(e)
        }
    }
}

#[derive(serde::Serialize)]
pub struct ModelStatusResponse {
    pub models: Vec<UnifiedModelInfo>,
}

#[derive(Clone, serde::Serialize)]
pub struct UnifiedModelInfo {
    pub name: String,
    pub display_name: String,
    pub size: u64,
    pub url: String,
    pub sha256: String,
    pub downloaded: bool,
    pub speed_score: u8,
    pub accuracy_score: u8,
    pub recommended: bool,
    pub engine: String,
    pub kind: String,
    pub requires_setup: bool,
}

/// Returns status of all available speech recognition models (Whisper + Parakeet).
///
/// **Platform Behavior**:
/// - **macOS**: Returns both Whisper and Parakeet models
/// - **Windows/Linux**: Returns only Whisper models (Parakeet filtered at compile time)
///
/// Parakeet models are automatically excluded on non-macOS platforms since they
/// require Apple Neural Engine via FluidAudio SDK.
#[tauri::command]
pub async fn get_model_status(
    whisper_state: State<'_, RwLock<WhisperManager>>,
    parakeet_manager: State<'_, ParakeetManager>,
    app: tauri::AppHandle,
) -> Result<ModelStatusResponse, String> {
    log::info!("[GET_MODEL_STATUS] Refreshing downloaded status...");

    let whisper_models_map = {
        let mut manager = whisper_state.write().await;
        manager.refresh_downloaded_status();
        manager.get_models_status()
    };

    let mut models: Vec<UnifiedModelInfo> = whisper_models_map
        .into_iter()
        .map(|(name, info)| convert_whisper_model(name, info))
        .collect();

    let parakeet_models = parakeet_manager.list_models();
    models.extend(parakeet_models.into_iter().map(convert_parakeet_model));

    // Inject cloud providers (e.g., Soniox)
    models.extend(collect_cloud_models(&app));

    // Sort with local models first (by size), cloud models afterwards
    models.sort_by(|a, b| {
        let a_key = (a.kind == "cloud", a.size);
        let b_key = (b.kind == "cloud", b.size);
        a_key.cmp(&b_key)
    });

    log::info!("[GET_MODEL_STATUS] Returning {} models", models.len());

    Ok(ModelStatusResponse { models })
}

#[tauri::command]
pub async fn delete_model(
    app: AppHandle,
    model_name: String,
    whisper_state: State<'_, RwLock<WhisperManager>>,
    parakeet_manager: State<'_, ParakeetManager>,
) -> Result<(), String> {
    let engine = determine_model_engine(&model_name, &whisper_state, &parakeet_manager).await?;

    match engine {
        ModelEngine::Whisper => {
            let mut manager = whisper_state.write().await;
            manager.delete_model_file(&model_name)?;
        }
        ModelEngine::Parakeet => {
            parakeet_manager.delete_model(&app, &model_name).await?;
        }
    }

    // Emit model-deleted event
    use tauri::Emitter;
    let _ = app.emit(
        "model-deleted",
        serde_json::json!({
            "model": model_name.clone(),
            "engine": engine.as_str()
        }),
    );

    // Refresh tray menu so the deleted model is removed from tray selection
    if let Err(e) = crate::commands::settings::update_tray_menu(app.clone()).await {
        log::warn!("Failed to update tray menu after model deletion: {}", e);
    }

    Ok(())
}

#[tauri::command]
pub async fn list_downloaded_models(
    state: State<'_, RwLock<WhisperManager>>,
) -> Result<Vec<String>, String> {
    let manager = state.read().await;
    Ok(manager.list_downloaded_files())
}

async fn identify_download_target(
    model_name: &str,
    whisper_state: &State<'_, RwLock<WhisperManager>>,
    parakeet_manager: &ParakeetManager,
) -> Result<DownloadTarget, String> {
    let engine = determine_model_engine(model_name, whisper_state, parakeet_manager).await?;

    match engine {
        ModelEngine::Whisper => {
            let manager = whisper_state.read().await;
            if let Some(info) = manager.get_models_status().get(model_name) {
                Ok(DownloadTarget {
                    engine,
                    size_bytes: info.size,
                })
            } else {
                Err(format!(
                    "Model '{}' not found in Whisper registry",
                    model_name
                ))
            }
        }
        ModelEngine::Parakeet => {
            if let Some(definition) = parakeet_manager.get_model_definition(model_name) {
                Ok(DownloadTarget {
                    engine,
                    size_bytes: definition.estimated_size,
                })
            } else {
                Err(format!(
                    "Model '{}' not found in Parakeet registry",
                    model_name
                ))
            }
        }
    }
}

async fn determine_model_engine(
    model_name: &str,
    whisper_state: &State<'_, RwLock<WhisperManager>>,
    parakeet_manager: &ParakeetManager,
) -> Result<ModelEngine, String> {
    {
        let manager = whisper_state.read().await;
        if manager.get_models_status().contains_key(model_name) {
            return Ok(ModelEngine::Whisper);
        }
    }

    if parakeet_manager.get_model_definition(model_name).is_some() {
        return Ok(ModelEngine::Parakeet);
    }

    Err(format!("Invalid model name: {}", model_name))
}

fn convert_whisper_model(name: String, info: ModelInfo) -> UnifiedModelInfo {
    UnifiedModelInfo {
        name,
        display_name: info.display_name.clone(),
        size: info.size,
        url: info.url.clone(),
        sha256: info.sha256.clone(),
        downloaded: info.downloaded,
        speed_score: info.speed_score,
        accuracy_score: info.accuracy_score,
        recommended: info.recommended,
        engine: ModelEngine::Whisper.as_str().to_string(),
        kind: "local".to_string(),
        requires_setup: false,
    }
}

fn convert_parakeet_model(status: ParakeetModelStatus) -> UnifiedModelInfo {
    UnifiedModelInfo {
        name: status.name,
        display_name: status.display_name,
        size: status.size,
        url: status.url,
        sha256: status.sha256,
        downloaded: status.downloaded,
        speed_score: status.speed_score,
        accuracy_score: status.accuracy_score,
        recommended: status.recommended,
        engine: ModelEngine::Parakeet.as_str().to_string(),
        kind: "local".to_string(),
        requires_setup: false,
    }
}

fn collect_cloud_models(app: &AppHandle) -> Vec<UnifiedModelInfo> {
    const STT_API_KEY_SONIOX: &str = "stt_api_key_soniox";
    let has_soniox_key = secure_store::secure_has(app, STT_API_KEY_SONIOX).unwrap_or_else(|err| {
        log::warn!(
            "[GET_MODEL_STATUS] Failed to check Soniox key presence: {}",
            err
        );
        false
    });

    vec![UnifiedModelInfo {
        name: "soniox".to_string(),
        display_name: "Soniox (Cloud)".to_string(),
        size: 0,
        url: String::new(),
        sha256: String::new(),
        downloaded: has_soniox_key,
        speed_score: 9,
        accuracy_score: 10,
        recommended: true,
        engine: "soniox".to_string(),
        kind: "cloud".to_string(),
        requires_setup: !has_soniox_key,
    }]
}

#[tauri::command]
pub async fn cancel_download(
    model_name: String,
    active_downloads: ActiveDownloadsState<'_>,
) -> Result<(), String> {
    log::info!("Cancelling download for model: {}", model_name);

    // Set the cancellation flag
    {
        match active_downloads.lock() {
            Ok(downloads) => {
                if let Some(cancel_flag) = downloads.get(&model_name) {
                    cancel_flag.store(true, Ordering::Relaxed);
                    log::info!("Set cancellation flag for model: {}", model_name);
                } else {
                    log::warn!("No active download found for model: {}", model_name);
                    return Ok(()); // Not an error if download doesn't exist
                }
            }
            Err(e) => {
                log::error!("Failed to lock active downloads for cancellation: {}", e);
                return Err("Failed to access download tracking".to_string());
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn verify_model(
    app: AppHandle,
    model_name: String,
    state: State<'_, RwLock<WhisperManager>>,
) -> Result<(), String> {
    log::info!("Verifying model: {}", model_name);

    // Get model info and check if it exists
    let (model_info, model_path) = {
        let manager = state.read().await;
        let info = manager
            .get_models_status()
            .get(&model_name)
            .ok_or(format!("Model '{}' not found", model_name))?
            .clone();
        let path = manager
            .get_model_path(&model_name)
            .ok_or(format!("Model '{}' path not found", model_name))?;
        (info, path)
    };

    // Check if file exists
    if !model_path.exists() {
        log::warn!("Model file does not exist: {:?}", model_path);
        return Err(format!("Model file not found: {}", model_name));
    }

    // Check file size
    let metadata = tokio::fs::metadata(&model_path)
        .await
        .map_err(|e| format!("Cannot read model file metadata: {}", e))?;

    let file_size = metadata.len();
    let expected_size = model_info.size;

    // Allow 5% tolerance for size differences
    let size_tolerance = (expected_size as f64 * 0.05) as u64;
    let min_size = expected_size.saturating_sub(size_tolerance);

    if file_size < min_size {
        log::warn!(
            "Model '{}' file size {} is less than expected minimum {}",
            model_name,
            file_size,
            min_size
        );

        // Delete the corrupted file
        if let Err(e) = tokio::fs::remove_file(&model_path).await {
            log::error!("Failed to delete corrupted model file: {}", e);
        }

        // Update manager status
        {
            let mut manager = state.write().await;
            manager.refresh_downloaded_status();
        }

        return Err(format!(
            "Model '{}' is corrupted and has been deleted. Please re-download.",
            model_name
        ));
    }

    // File looks good - mark as downloaded
    {
        let mut manager = state.write().await;
        if let Some(info) = manager.get_models_status_mut().get_mut(&model_name) {
            info.downloaded = true;
        }
    }

    log::info!("Model '{}' verified successfully", model_name);

    // Emit verification success event
    let _ = app.emit("model-verified", model_name.clone());

    Ok(())
}

#[tauri::command]
pub async fn preload_model(
    app: AppHandle,
    model_name: String,
    state: State<'_, RwLock<WhisperManager>>,
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
        let manager = state.read().await;
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
