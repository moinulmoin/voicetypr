use tauri::{AppHandle, Manager, Emitter};
use tauri_plugin_store::StoreExt;
use std::fs;

#[derive(serde::Serialize)]
pub struct ResetResult {
    pub success: bool,
    pub errors: Vec<String>,
    pub cleared_items: Vec<String>,
}

#[tauri::command]
pub async fn reset_app_data(app: AppHandle) -> Result<ResetResult, String> {
    log::info!("Starting app data reset");
    
    let mut errors = Vec::new();
    let mut cleared_items = Vec::new();
    
    // 1. Clear all stores and delete the store files
    // Clear settings store
    if let Ok(store) = app.store("settings") {
        store.clear();
        if let Err(e) = store.save() {
            errors.push(format!("Failed to save cleared settings store: {}", e));
        } else {
            cleared_items.push("Settings store".to_string());
        }
    }
    
    // Clear transcriptions store
    if let Ok(store) = app.store("transcriptions") {
        store.clear();
        if let Err(e) = store.save() {
            errors.push(format!("Failed to save cleared transcriptions store: {}", e));
        } else {
            cleared_items.push("Transcriptions store".to_string());
        }
    }
    
    // Delete the actual store files from disk
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        let stores_dir = app_data_dir.join("stores");
        if stores_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&stores_dir) {
                errors.push(format!("Failed to delete stores directory: {}", e));
            } else {
                cleared_items.push("Stores directory".to_string());
            }
        }
    }
    
    // 2. Delete app data directories
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        // Delete models directory
        let models_dir = app_data_dir.join("models");
        if models_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&models_dir) {
                errors.push(format!("Failed to delete models directory: {}", e));
            } else {
                cleared_items.push("Downloaded models".to_string());
            }
        }
        
        // Delete recordings directory
        let recordings_dir = app_data_dir.join("recordings");
        if recordings_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&recordings_dir) {
                errors.push(format!("Failed to delete recordings directory: {}", e));
            } else {
                cleared_items.push("Audio recordings".to_string());
            }
        }
    }
    
    // 3. Clear license data from secure store
    if let Err(e) = crate::secure_store::secure_delete(&app, "license") {
        // Only push error if it's not a "store doesn't exist" error
        if !e.contains("Store access failed") {
            errors.push(format!("Failed to clear license: {}", e));
        }
    } else {
        cleared_items.push("License data".to_string());
    }
    
    // 3.5. Clear the secure.dat file itself
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        let secure_store_path = app_data_dir.join("secure.dat");
        if secure_store_path.exists() {
            if let Err(e) = fs::remove_file(&secure_store_path) {
                errors.push(format!("Failed to remove secure storage: {}", e));
            } else {
                cleared_items.push("Secure storage (API keys)".to_string());
            }
        }
    }
    
    // 4. Clear cache data (license validation cache)
    if let Ok(cache_dir) = app.path().cache_dir() {
        if cache_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&cache_dir) {
                errors.push(format!("Failed to clear cache: {}", e));
            } else {
                cleared_items.push("Cache directory".to_string());
            }
        }
    }
    
    // 5. Clear app preferences (macOS defaults system)
    match std::process::Command::new("defaults")
        .args(&["delete", "com.ideaplexa.voicetypr"])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                cleared_items.push("System preferences".to_string());
            }
        }
        Err(_) => {
            // No defaults to clear is not an error
        }
    }
    
    // Also remove the preferences plist file
    if let Ok(home_dir) = app.path().home_dir() {
        let prefs_path = home_dir
            .join("Library")
            .join("Preferences")
            .join("com.ideaplexa.voicetypr.plist");
        if prefs_path.exists() {
            if let Err(e) = fs::remove_file(&prefs_path) {
                errors.push(format!("Failed to remove preferences file: {}", e));
            } else {
                cleared_items.push("Preferences plist".to_string());
            }
        }
    }
    
    // 6. Clear additional system data
    if let Ok(home_dir) = app.path().home_dir() {
        // Clear saved application state (window positions, etc)
        let saved_state_path = home_dir
            .join("Library")
            .join("Saved Application State")
            .join("com.ideaplexa.voicetypr.savedState");
        if saved_state_path.exists() {
            if let Err(e) = fs::remove_dir_all(&saved_state_path) {
                errors.push(format!("Failed to clear saved state: {}", e));
            } else {
                cleared_items.push("Window state".to_string());
            }
        }
        
        // Clear any logs
        let logs_path = home_dir
            .join("Library")
            .join("Logs")
            .join("com.ideaplexa.voicetypr");
        if logs_path.exists() {
            if let Err(e) = fs::remove_dir_all(&logs_path) {
                errors.push(format!("Failed to clear logs: {}", e));
            } else {
                cleared_items.push("Application logs".to_string());
            }
        }
        
        // Clear WebKit data if any
        let webkit_path = home_dir
            .join("Library")
            .join("WebKit")
            .join("com.ideaplexa.voicetypr");
        if webkit_path.exists() {
            if let Err(e) = fs::remove_dir_all(&webkit_path) {
                errors.push(format!("Failed to clear WebKit data: {}", e));
            } else {
                cleared_items.push("WebKit data".to_string());
            }
        }
        
        // Clear NSURLSession downloads cache
        let nsurlsession_path = home_dir
            .join("Library")
            .join("Caches")
            .join("com.apple.nsurlsessiond")
            .join("Downloads")
            .join("com.ideaplexa.voicetypr");
        if nsurlsession_path.exists() {
            if let Err(e) = fs::remove_dir_all(&nsurlsession_path) {
                errors.push(format!("Failed to clear download cache: {}", e));
            } else {
                cleared_items.push("Download cache".to_string());
            }
        }
    }
    
    // 7. Reset system permissions using osascript with admin privileges
    let reset_script = r#"do shell script "tccutil reset All com.ideaplexa.voicetypr" with administrator privileges"#;
    
    match tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(reset_script)
        .output()
        .await
    {
        Ok(output) => {
            if output.status.success() {
                cleared_items.push("System permissions".to_string());
            } else {
                // User might have cancelled - not a critical error
                log::info!("User cancelled permission reset");
            }
        }
        Err(e) => {
            errors.push(format!("Could not reset permissions: {}", e));
        }
    }
    
    // 8. Clear any runtime state
    use tauri::async_runtime::RwLock as AsyncRwLock;
    let whisper_state = app.state::<AsyncRwLock<crate::whisper::manager::WhisperManager>>();
    let mut whisper_manager = whisper_state.write().await;
    whisper_manager.clear_all();
    drop(whisper_manager);
    cleared_items.push("Runtime state".to_string());
    
    // 8.5. Clear API key cache
    if let Err(e) = crate::commands::ai::clear_all_api_key_cache() {
        errors.push(format!("Failed to clear API key cache: {}", e));
    } else {
        cleared_items.push("AI API key cache".to_string());
    }
    
    // 9. Kill cfprefsd to ensure preference changes take effect
    match std::process::Command::new("killall")
        .arg("cfprefsd")
        .output()
    {
        Ok(_) => {
            log::info!("Refreshed cfprefsd");
        }
        Err(_) => {
            // Not critical
        }
    }
    
    // 10. Emit reset event to frontend
    if let Err(e) = app.emit("app-reset", ()) {
        errors.push(format!("Failed to emit reset event: {}", e));
    }
    
    let success = errors.is_empty();
    
    if success {
        log::info!("App data reset completed successfully");
    } else {
        log::warn!("App data reset completed with {} errors", errors.len());
    }
    
    Ok(ResetResult {
        success,
        errors,
        cleared_items,
    })
}