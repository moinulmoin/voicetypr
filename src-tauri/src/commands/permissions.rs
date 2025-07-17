#[tauri::command]
pub async fn check_accessibility_permission() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_macos_permissions::check_accessibility_permission;
        
        log::info!("Checking accessibility permissions for keyboard simulation");
        
        let has_permission = check_accessibility_permission().await;
        
        if has_permission {
            log::info!("Accessibility permission is authorized");
        } else {
            log::warn!("Accessibility permission is not authorized");
        }
        
        Ok(has_permission)
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        // On non-macOS platforms, we don't need special permissions
        Ok(true)
    }
}

#[tauri::command]
pub async fn request_accessibility_permission() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_macos_permissions::request_accessibility_permission;
        
        log::info!("Requesting accessibility permissions");
        request_accessibility_permission().await;
        Ok(())
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

#[tauri::command]
pub async fn check_microphone_permission() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_macos_permissions::check_microphone_permission;
        
        log::info!("Checking microphone permissions");
        
        let has_permission = check_microphone_permission().await;
        
        if has_permission {
            log::info!("Microphone permission is authorized");
        } else {
            log::warn!("Microphone permission is not authorized");
        }
        
        Ok(has_permission)
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        // On non-macOS platforms, we assume permission is granted
        Ok(true)
    }
}

#[tauri::command]
pub async fn request_microphone_permission() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_macos_permissions::request_microphone_permission;
        
        log::info!("Requesting microphone permissions");
        
        // Request permission - this will show the system dialog
        let _ = request_microphone_permission().await;
        
        // After requesting, check the actual permission status
        use tauri_plugin_macos_permissions::check_microphone_permission;
        let has_permission = check_microphone_permission().await;
        
        if has_permission {
            log::info!("Microphone permission granted");
        } else {
            log::warn!("Microphone permission denied");
        }
        
        Ok(has_permission)
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}