use tauri::{AppHandle, Manager, Emitter};
use tauri_plugin_store::StoreExt;
use std::fs;

#[tauri::command]
pub async fn reset_app_data(app: AppHandle) -> Result<(), String> {
    log::info!("Starting app data reset");
    
    // 1. Clear all stores and delete the store files
    // Clear settings store
    if let Ok(store) = app.store("settings") {
        store.clear();
        if let Err(e) = store.save() {
            log::error!("Failed to save cleared settings store: {}", e);
        }
    }
    
    // Clear transcriptions store
    if let Ok(store) = app.store("transcriptions") {
        store.clear();
        if let Err(e) = store.save() {
            log::error!("Failed to save cleared transcriptions store: {}", e);
        }
    }
    
    // Delete the actual store files from disk
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        let stores_dir = app_data_dir.join("stores");
        if stores_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&stores_dir) {
                log::warn!("Failed to delete stores directory: {}", e);
            } else {
                log::info!("Deleted stores directory");
            }
        }
    }
    
    // 2. Delete app data directories
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        // Delete models directory
        let models_dir = app_data_dir.join("models");
        if models_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&models_dir) {
                log::error!("Failed to delete models directory: {}", e);
            } else {
                log::info!("Deleted models directory");
            }
        }
        
        // Delete recordings directory
        let recordings_dir = app_data_dir.join("recordings");
        if recordings_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&recordings_dir) {
                log::error!("Failed to delete recordings directory: {}", e);
            } else {
                log::info!("Deleted recordings directory");
            }
        }
    }
    
    // 3. Clear license data from keychain
    log::info!("Clearing license data");
    if let Err(e) = keyring::Entry::new("com.ideaplexa.voicetypr", "license")
        .and_then(|entry| entry.delete_password())
    {
        log::warn!("Failed to clear license from keychain: {}", e);
        // Don't fail the whole reset if keychain clear fails
    }
    
    // 4. Clear cache data (license validation cache)
    if let Ok(cache_dir) = app.path().cache_dir() {
        if cache_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&cache_dir) {
                log::warn!("Failed to clear cache directory: {}", e);
            } else {
                log::info!("Cleared cache directory");
            }
        }
    }
    
    // 5. Clear app preferences (macOS defaults system)
    log::info!("Clearing app preferences...");
    match std::process::Command::new("defaults")
        .args(&["delete", "com.ideaplexa.voicetypr"])
        .output()
    {
        Ok(_) => log::info!("Cleared defaults preferences"),
        Err(e) => log::debug!("No defaults to clear: {}", e),
    }
    
    // Also remove the preferences plist file
    if let Ok(home_dir) = app.path().home_dir() {
        let prefs_path = home_dir
            .join("Library")
            .join("Preferences")
            .join("com.ideaplexa.voicetypr.plist");
        if prefs_path.exists() {
            if let Err(e) = fs::remove_file(&prefs_path) {
                log::warn!("Failed to remove preferences plist: {}", e);
            } else {
                log::info!("Removed preferences plist file");
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
                log::warn!("Failed to clear saved state: {}", e);
            } else {
                log::info!("Cleared saved application state");
            }
        }
        
        // Clear any logs
        let logs_path = home_dir
            .join("Library")
            .join("Logs")
            .join("com.ideaplexa.voicetypr");
        if logs_path.exists() {
            if let Err(e) = fs::remove_dir_all(&logs_path) {
                log::warn!("Failed to clear logs: {}", e);
            } else {
                log::info!("Cleared application logs");
            }
        }
        
        // Clear WebKit data if any
        let webkit_path = home_dir
            .join("Library")
            .join("WebKit")
            .join("com.ideaplexa.voicetypr");
        if webkit_path.exists() {
            if let Err(e) = fs::remove_dir_all(&webkit_path) {
                log::warn!("Failed to clear WebKit data: {}", e);
            } else {
                log::info!("Cleared WebKit data");
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
                log::warn!("Failed to clear NSURLSession cache: {}", e);
            } else {
                log::info!("Cleared NSURLSession downloads cache");
            }
        }
    }
    
    // 7. Reset system permissions using osascript with admin privileges
    log::info!("Attempting to reset system permissions...");
    
    // Create AppleScript that will prompt for admin password
    // Using 'tccutil reset All' to reset all permissions at once
    let reset_script = r#"do shell script "tccutil reset All com.ideaplexa.voicetypr" with administrator privileges"#;
    
    // Execute the script and wait for completion
    match tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(reset_script)
        .output()
        .await
    {
        Ok(output) => {
            if output.status.success() {
                log::info!("Successfully reset system permissions");
                // Log the output for debugging
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.is_empty() {
                    log::info!("tccutil output: {}", stdout);
                }
            } else {
                let error = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                log::warn!("Failed to reset permissions - stderr: {}, stdout: {}", error, stdout);
                // User might have cancelled the password prompt - continue with reset
            }
        }
        Err(e) => {
            log::warn!("Could not execute permission reset: {}", e);
            // Continue with reset even if this fails
        }
    }
    
    // 8. Clear any runtime state
    // Reset the whisper manager state
    use tauri::async_runtime::RwLock as AsyncRwLock;
    let whisper_state = app.state::<AsyncRwLock<crate::whisper::manager::WhisperManager>>();
    let mut whisper_manager = whisper_state.write().await;
    whisper_manager.clear_all();
    drop(whisper_manager);
    
    // 9. Transcriber cache will be cleared when app restarts
    // The cache is in-memory only and doesn't persist between app launches
    
    // 10. Kill cfprefsd to ensure preference changes take effect
    log::info!("Refreshing preferences daemon...");
    match std::process::Command::new("killall")
        .arg("cfprefsd")
        .output()
    {
        Ok(_) => log::info!("Refreshed cfprefsd"),
        Err(e) => log::debug!("Could not refresh cfprefsd: {}", e),
    }
    
    // 11. Emit reset event to frontend
    app.emit("app-reset", ()).map_err(|e| e.to_string())?;
    
    log::info!("App data reset completed - app is now in fresh install state");
    Ok(())
}