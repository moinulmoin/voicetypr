use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

#[derive(Debug, Clone)]
pub struct WindowManager {
    app_handle: AppHandle,
    main_window: Arc<Mutex<Option<WebviewWindow>>>,
    pill_window: Arc<Mutex<Option<WebviewWindow>>>,
}

impl WindowManager {
    pub fn new(app_handle: AppHandle) -> Self {
        // Get reference to main window on creation
        let main_window = app_handle.get_webview_window("main");
        
        Self {
            app_handle,
            main_window: Arc::new(Mutex::new(main_window)),
            pill_window: Arc::new(Mutex::new(None)),
        }
    }

    /// Get the main window reference
    pub fn get_main_window(&self) -> Option<WebviewWindow> {
        self.main_window.lock().unwrap().clone()
    }

    /// Get the pill window reference
    pub fn get_pill_window(&self) -> Option<WebviewWindow> {
        self.pill_window.lock().unwrap().clone()
    }

    /// Show the pill window, creating it if necessary
    pub async fn show_pill_window(&self) -> Result<(), String> {
        let mut pill_guard = self.pill_window.lock().unwrap();
        
        // Check if window exists and is still valid
        if let Some(window) = &*pill_guard {
            // Try to show the window - if it fails, we'll create a new one
            if window.show().is_ok() {
                log::info!("Showing existing pill window");
                return Ok(());
            }
        }

        // Create new pill window
        log::info!("Creating new pill window");
        let pill_window = WebviewWindowBuilder::new(
            &self.app_handle,
            "pill",
            WebviewUrl::App("pill".into())
        )
        .title("Recording")
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .content_protected(true)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .inner_size(200.0, 60.0)
        .build()
        .map_err(|e| e.to_string())?;

        // Convert to NSPanel on macOS
        #[cfg(target_os = "macos")]
        {
            use tauri_nspanel::WebviewWindowExt;
            
            pill_window.to_panel()
                .map_err(|e| format!("Failed to convert to NSPanel: {:?}", e))?;
            
            log::info!("Converted pill window to NSPanel");
        }

        // Show the window
        pill_window.show().map_err(|e| e.to_string())?;
        
        // Store the window reference
        *pill_guard = Some(pill_window);
        
        log::info!("Pill window created and shown successfully");
        Ok(())
    }

    /// Hide the pill window
    pub async fn hide_pill_window(&self) -> Result<(), String> {
        if let Some(window) = self.get_pill_window() {
            window.hide().map_err(|e| e.to_string())?;
            log::info!("Pill window hidden");
        }
        Ok(())
    }

    /// Close the pill window
    pub async fn close_pill_window(&self) -> Result<(), String> {
        // Take the window out of the mutex to avoid holding lock across await
        let window = {
            let mut pill_guard = self.pill_window.lock().unwrap();
            pill_guard.take()
        };
        
        if let Some(window) = window {
            // First hide to prevent focus issues
            window.hide().map_err(|e| e.to_string())?;
            
            // Small delay before closing
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            
            // Close the window
            window.close().map_err(|e| e.to_string())?;
            log::info!("Pill window closed");
        }
        
        Ok(())
    }

    /// Emit event to specific window
    pub fn emit_to_window(&self, window_id: &str, event: &str, payload: serde_json::Value) -> Result<(), String> {
        let window = match window_id {
            "main" => self.get_main_window(),
            "pill" => self.get_pill_window(),
            _ => None,
        };

        if let Some(window) = window {
            window.emit(event, payload).map_err(|e| e.to_string())?;
            log::debug!("Emitted '{}' event to {} window", event, window_id);
        } else {
            log::warn!("Cannot emit '{}' event - {} window not found", event, window_id);
        }

        Ok(())
    }

    /// Emit event to pill window only
    pub fn emit_to_pill(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
        self.emit_to_window("pill", event, payload)
    }

    /// Emit event to main window only
    pub fn emit_to_main(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
        self.emit_to_window("main", event, payload)
    }

    /// Check if pill window is visible
    pub fn is_pill_visible(&self) -> bool {
        if let Some(window) = self.get_pill_window() {
            window.is_visible().unwrap_or(false)
        } else {
            false
        }
    }

    /// Update pill window position
    pub async fn update_pill_position(&self, x: f64, y: f64) -> Result<(), String> {
        if let Some(window) = self.get_pill_window() {
            use tauri::LogicalPosition;
            window.set_position(LogicalPosition::new(x, y))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}