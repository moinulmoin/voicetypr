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
            
            // Capture to Sentry - permission denial blocks core functionality
            use crate::capture_sentry_message;
            capture_sentry_message!(
                "Microphone permission denied by user",
                tauri_plugin_sentry::sentry::Level::Warning,
                tags: {
                    "permission.type" => "microphone",
                    "permission.status" => "denied",
                    "operation" => "request"
                }
            );
        }

        Ok(has_permission)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}

#[tauri::command]
pub async fn test_automation_permission() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        
        log::info!("Testing automation permission by simulating Cmd+V");
        
        // Try to simulate Cmd+V which will trigger the System Events permission dialog
        // This is exactly what happens during actual paste operation
        let script = r#"
            tell application "System Events"
                -- Simulate Cmd+V (paste)
                keystroke "v" using command down
                return "success"
            end tell
        "#;
        
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| format!("Failed to run AppleScript: {}", e))?;
        
        if output.status.success() {
            log::info!("Automation permission granted - Cmd+V simulation succeeded");
            Ok(true)
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            if error.contains("not allowed assistive access") || error.contains("1743") {
                log::warn!("Automation permission denied by user: {}", error);
                
                // Capture to Sentry - accessibility permission needed for paste
                use crate::capture_sentry_message;
                capture_sentry_message!(
                    "Accessibility permission denied for automation",
                    tauri_plugin_sentry::sentry::Level::Warning,
                    tags: {
                        "permission.type" => "accessibility",
                        "permission.status" => "denied",
                        "operation" => "test_automation"
                    }
                );
                
                Ok(false)
            } else {
                log::error!("AppleScript failed with unexpected error: {}", error);
                
                // Capture unexpected AppleScript errors
                use crate::capture_sentry_message;
                capture_sentry_message!(
                    &format!("AppleScript automation test failed: {}", error),
                    tauri_plugin_sentry::sentry::Level::Error,
                    tags: {
                        "error.type" => "applescript_error",
                        "component" => "permissions"
                    }
                );
                
                Err(format!("AppleScript error: {}", error))
            }
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}
