use tokio::time::{sleep, Duration};

#[tauri::command]
pub async fn check_accessibility_permission() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_macos_permissions::check_accessibility_permission;

        log::info!("Checking accessibility permissions for keyboard simulation");

        // Try checking with a small delay to handle macOS timing issues
        let mut attempts = 0;
        const MAX_ATTEMPTS: u8 = 3;

        loop {
            attempts += 1;

            // Check permission
            let has_permission = check_accessibility_permission().await;

            // If we got a definitive result or reached max attempts, return
            if has_permission || attempts >= MAX_ATTEMPTS {
                if has_permission {
                    log::info!("Accessibility permission is authorized");
                } else {
                    log::warn!(
                        "Accessibility permission is not authorized after {} attempts",
                        attempts
                    );
                }
                return Ok(has_permission);
            }

            // Wait a bit before retry
            log::debug!(
                "Permission check returned false, retrying... (attempt {}/{})",
                attempts,
                MAX_ATTEMPTS
            );
            sleep(Duration::from_millis(200)).await;
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // On non-macOS platforms, we don't need special permissions
        Ok(true)
    }
}

#[tauri::command]
pub async fn request_accessibility_permission(app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_macos_permissions::{
            check_accessibility_permission, request_accessibility_permission,
        };

        // First check if permission is already granted
        let already_granted = check_accessibility_permission().await;
        if already_granted {
            log::info!("Accessibility permission already granted");

            // Emit accessibility-granted event for UI update
            log::info!("Emitting accessibility-granted event");
            use tauri::Emitter;
            let _ = app.emit("accessibility-granted", ());

            // Return true to indicate permission is already granted
            return Ok(true);
        }

        log::info!("Requesting accessibility permissions");
        request_accessibility_permission().await;

        // Wait a bit for macOS to process the request
        sleep(Duration::from_millis(500)).await;

        // Check the permission status after request and update readiness
        let has_permission = check_accessibility_permission().await;

        log::info!(
            "Accessibility permission check after request: {}",
            has_permission
        );

        // Emit appropriate event based on permission status
        use tauri::Emitter;
        if has_permission {
            let _ = app.emit("accessibility-granted", ());
        } else {
            let _ = app.emit("accessibility-denied", ());
        }

        Ok(has_permission)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}

#[tauri::command]
pub async fn check_microphone_permission() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_macos_permissions::check_microphone_permission;

        log::info!("Checking microphone permissions");

        // Try checking with a small delay to handle macOS timing issues
        let mut attempts = 0;
        const MAX_ATTEMPTS: u8 = 3;

        loop {
            attempts += 1;

            // Check permission
            let has_permission = check_microphone_permission().await;

            // If we got a definitive result or reached max attempts, return
            if has_permission || attempts >= MAX_ATTEMPTS {
                if has_permission {
                    log::info!("Microphone permission is authorized");
                } else {
                    log::warn!(
                        "Microphone permission is not authorized after {} attempts",
                        attempts
                    );
                }
                return Ok(has_permission);
            }

            // Wait a bit before retry
            log::debug!(
                "Permission check returned false, retrying... (attempt {}/{})",
                attempts,
                MAX_ATTEMPTS
            );
            sleep(Duration::from_millis(200)).await;
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // On non-macOS platforms, we assume permission is granted
        Ok(true)
    }
}

#[tauri::command]
pub async fn request_microphone_permission(app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_macos_permissions::{
            check_microphone_permission, request_microphone_permission,
        };

        // First check if permission is already granted
        let already_granted = check_microphone_permission().await;
        if already_granted {
            log::info!("Microphone permission already granted");

            // Emit microphone-granted event for UI update
            log::info!("Emitting microphone-granted event");
            use tauri::Emitter;
            let _ = app.emit("microphone-granted", ());

            return Ok(true);
        }

        log::info!("Requesting microphone permissions");

        // Request permission - this will show the system dialog
        let _ = request_microphone_permission().await;

        // Wait a bit for macOS to process
        sleep(Duration::from_millis(500)).await;

        // After requesting, check the actual permission status
        let has_permission = check_microphone_permission().await;

        if has_permission {
            log::info!("Microphone permission granted");
        } else {
            log::warn!("Microphone permission denied");
        }

        // Emit appropriate event based on permission status
        use tauri::Emitter;
        if has_permission {
            let _ = app.emit("microphone-granted", ());
        } else {
            let _ = app.emit("microphone-denied", ());
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

                Ok(false)
            } else {
                log::error!("AppleScript failed with unexpected error: {}", error);

                Err(format!("AppleScript error: {}", error))
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}
