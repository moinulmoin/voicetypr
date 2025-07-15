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
    
    /// Check if pill window exists
    pub fn has_pill_window(&self) -> bool {
        self.pill_window.lock().unwrap().is_some()
    }

    /// Show the pill window, creating it if necessary (with retry logic)
    pub async fn show_pill_window(&self) -> Result<(), String> {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY_MS: u64 = 100;
        
        for attempt in 1..=MAX_RETRIES {
            match self.show_pill_window_internal().await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempt < MAX_RETRIES {
                        log::warn!("Failed to show pill window (attempt {}): {}. Retrying...", 
                                  attempt, e);
                        tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
                    } else {
                        return Err(format!("Failed to show pill window after {} attempts: {}", 
                                         MAX_RETRIES, e));
                    }
                }
            }
        }
        
        unreachable!()
    }
    
    /// Internal implementation of show_pill_window
    async fn show_pill_window_internal(&self) -> Result<(), String> {
        // First check if Tauri has a window with the "pill" label
        if let Some(existing_window) = self.app_handle.get_webview_window("pill") {
            // Window exists in Tauri, update our reference and show it
            {
                let mut pill_guard = self.pill_window.lock().unwrap();
                *pill_guard = Some(existing_window.clone());
            }
            
            existing_window.show().map_err(|e| e.to_string())?;
            
            // Always position at center-bottom
            use tauri::LogicalPosition;
            let (x, y) = self.calculate_center_position();
            let _ = existing_window.set_position(LogicalPosition::new(x, y));
            
            log::debug!("Found and showing existing pill window from Tauri");
            
            return Ok(());
        }

        // No window exists, create new one
        log::info!("Creating new pill window (lazy-loaded on recording start)");
        
        // Always use fixed center-bottom position
        let (position_x, position_y) = self.calculate_center_position();
        log::info!("Positioning pill at center-bottom: ({}, {})", position_x, position_y);
        
        let pill_window = WebviewWindowBuilder::new(
            &self.app_handle,
            "pill",
            WebviewUrl::App("pill.html".into())
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
        .position(position_x, position_y)
        .visible(true)  // Start visible
        .focused(false)  // Don't steal focus
        .build()
        .map_err(|e| e.to_string())?;

        // Show the window first (before NSPanel conversion)
        pill_window.show().map_err(|e| e.to_string())?;
        
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
        
        log::info!("Pill window created and shown at ({}, {})", position_x, position_y);
        Ok(())
    }

    /// Hide the pill window (don't close it) with retry logic
    pub async fn hide_pill_window(&self) -> Result<(), String> {
        if let Some(window) = self.get_pill_window() {
            const MAX_RETRIES: u32 = 3;
            const RETRY_DELAY_MS: u64 = 50;
            
            for attempt in 1..=MAX_RETRIES {
                match window.hide() {
                    Ok(_) => {
                        log::info!("Pill window hidden");
                        return Ok(());
                    }
                    Err(e) => {
                        if attempt < MAX_RETRIES {
                            log::warn!("Failed to hide pill window (attempt {}): {}. Retrying...", 
                                      attempt, e);
                            tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
                        } else {
                            return Err(format!("Failed to hide pill window after {} attempts: {}", 
                                             MAX_RETRIES, e));
                        }
                    }
                }
            }
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
        // Only log critical events
        if matches!(event, "recording-state-changed" | "transcription-complete" | "transcription-error") {
            log::debug!("emit_to_window: window='{}', event='{}'", window_id, event);
        }
        
        let window = match window_id {
            "main" => self.get_main_window(),
            "pill" => self.get_pill_window(),
            _ => None,
        };

        if let Some(window) = window {
            // Skip visibility check for performance
            
            // Check if window is visible or if it's a critical event
            let is_critical = matches!(event, "recording-state-changed" | "transcription-complete" | "transcription-error");
            
            // Check if window is visible or if it's a critical event
            
            match window.emit(event, payload.clone()) {
                Ok(_) => {}
                Err(e) => {
                    if is_critical {
                        log::error!("[FLOW] Failed to emit critical event '{}' to {} window: {}", event, window_id, e);
                        // For critical events, retry with app-wide emission
                        if let Err(e2) = self.app_handle.emit(event, payload) {
                            log::error!("Also failed app-wide emission: {}", e2);
                        }
                    } else {
                        log::debug!("Failed to emit '{}' event to {} window: {}", event, window_id, e);
                    }
                    return Err(e.to_string());
                }
            }
        } else {
            log::debug!("Cannot emit '{}' event - {} window not found", event, window_id);
            // For critical events when window not found, try app-wide emission
            if matches!(event, "recording-state-changed" | "transcription-complete" | "transcription-error") {
                if let Err(e) = self.app_handle.emit(event, payload) {
                    log::error!("App-wide emission also failed: {}", e);
                }
            }
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

    
    /// Calculate center bottom position for pill window
    fn calculate_center_position(&self) -> (f64, f64) {
        if let Some(main_window) = self.get_main_window() {
            if let Ok(Some(monitor)) = main_window.current_monitor() {
                let size = monitor.size();
                let scale = monitor.scale_factor();
                let width = size.width as f64 / scale;
                let height = size.height as f64 / scale;
                
                // Center bottom position with offset
                let pill_width = 200.0;
                let pill_height = 60.0;
                let bottom_offset = 100.0; // Distance from bottom of screen
                
                let x = (width - pill_width) / 2.0;
                let y = height - pill_height - bottom_offset;
                
                (x, y)
            } else {
                // Fallback for 1080p screen
                (860.0, 920.0)
            }
        } else {
            // Fallback for 1080p screen
            (860.0, 920.0)
        }
    }
}