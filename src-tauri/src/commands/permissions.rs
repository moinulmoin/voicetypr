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