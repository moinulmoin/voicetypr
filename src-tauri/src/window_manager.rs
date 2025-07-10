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
        // First check if Tauri has a window with the "pill" label
        if let Some(existing_window) = self.app_handle.get_webview_window("pill") {
            // Window exists in Tauri, update our reference and show it
            {
                let mut pill_guard = self.pill_window.lock().unwrap();
                *pill_guard = Some(existing_window.clone());
            }
            
            existing_window.show().map_err(|e| e.to_string())?;
            existing_window.center().map_err(|e| e.to_string())?;
            
            // Debug: Check if window is actually visible
            let is_visible = existing_window.is_visible().unwrap_or(false);
            log::info!("Found and showing existing pill window from Tauri - visible: {}", is_visible);
            
            // If not visible after show, try set_focus as a workaround
            if !is_visible {
                log::warn!("Window not visible after show(), trying set_focus()");
                let _ = existing_window.set_focus();
            }
            
            return Ok(());
        }

        // No window exists, create new one
        log::info!("Creating new pill window");
        
        // Get screen dimensions to center the pill
        let (center_x, center_y) = if let Some(main_window) = self.get_main_window() {
            if let Ok(Some(monitor)) = main_window.current_monitor() {
                let size = monitor.size();
                let scale = monitor.scale_factor();
                let width = size.width as f64 / scale;
                let height = size.height as f64 / scale;
                
                // Center position (accounting for pill size)
                let pill_width = 200.0;
                let pill_height = 60.0;
                ((width - pill_width) / 2.0, (height - pill_height) / 2.0)
            } else {
                (640.0, 360.0)
            }
        } else {
            (640.0, 360.0)
        };
        
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
        .shadow(true)
        .skip_taskbar(true)
        .inner_size(200.0, 60.0)
        .position(center_x, center_y)
        .visible(true)  // Start visible
        .focused(false)  // Don't steal focus
        .build()
        .map_err(|e| e.to_string())?;

        // Show the window first (before NSPanel conversion)
        pill_window.show().map_err(|e| e.to_string())?;
        pill_window.center().map_err(|e| e.to_string())?;
        
        // Convert to NSPanel on macOS
        #[cfg(target_os = "macos")]
        {
            use tauri_nspanel::WebviewWindowExt;
            
            pill_window.to_panel()
                .map_err(|e| format!("Failed to convert to NSPanel: {:?}", e))?;
            
            log::info!("Converted pill window to NSPanel");
            
            // Show again after NSPanel conversion
            pill_window.show().map_err(|e| e.to_string())?;
        }
        
        // Store the window reference
        {
            let mut pill_guard = self.pill_window.lock().unwrap();
            *pill_guard = Some(pill_window);
        }
        
        log::info!("Pill window created and shown at ({}, {})", center_x, center_y);
        Ok(())
    }

    /// Hide the pill window (don't close it)
    pub async fn hide_pill_window(&self) -> Result<(), String> {
        if let Some(window) = self.get_pill_window() {
            window.hide().map_err(|e| e.to_string())?;
            log::info!("Pill window hidden");
        }
        Ok(())
    }

    /// Close the pill window (actually destroy it)
    pub async fn close_pill_window(&self) -> Result<(), String> {
        // Take the window out of the mutex
        let window = {
            let mut pill_guard = self.pill_window.lock().unwrap();
            pill_guard.take()
        };
        
        if let Some(window) = window {
            // Hide first
            let _ = window.hide();
            
            // Then close
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