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
            
            // Restore saved position if available
            use tauri_plugin_store::StoreExt;
            use tauri::LogicalPosition;
            
            let position_restored = if let Ok(store) = self.app_handle.store("settings") {
                if let Some(pos_value) = store.get("pill_position") {
                    if let Some(arr) = pos_value.as_array() {
                        if arr.len() == 2 {
                            if let (Some(x), Some(y)) = (arr[0].as_f64(), arr[1].as_f64()) {
                                let _ = existing_window.set_position(LogicalPosition::new(x, y));
                                true
                            } else { false }
                        } else { false }
                    } else { false }
                } else { false }
            } else { false };
            
            if !position_restored {
                existing_window.center().map_err(|e| e.to_string())?;
            }
            
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
        log::info!("Creating new pill window (lazy-loaded on recording start)");
        
        // Get position - use saved position if available, otherwise center
        use tauri_plugin_store::StoreExt;
        
        let (position_x, position_y) = if let Ok(store) = self.app_handle.store("settings") {
            if let Some(pos_value) = store.get("pill_position") {
                if let Some(arr) = pos_value.as_array() {
                    if arr.len() == 2 {
                        if let (Some(x), Some(y)) = (arr[0].as_f64(), arr[1].as_f64()) {
                            log::info!("Using saved pill position: ({}, {})", x, y);
                            (x, y)
                        } else {
                            self.calculate_center_position()
                        }
                    } else {
                        self.calculate_center_position()
                    }
                } else {
                    self.calculate_center_position()
                }
            } else {
                self.calculate_center_position()
            }
        } else {
            self.calculate_center_position()
        };
        
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
        log::info!("[TRANSCRIPTION_DEBUG] emit_to_window called: window='{}', event='{}'", window_id, event);
        
        let window = match window_id {
            "main" => self.get_main_window(),
            "pill" => self.get_pill_window(),
            _ => None,
        };

        if let Some(window) = window {
            // Log window state
            let is_visible = window.is_visible().unwrap_or(false);
            log::info!("[TRANSCRIPTION_DEBUG] {} window found, visible: {}", window_id, is_visible);
            
            // Check if window is visible or if it's a critical event
            let is_critical = matches!(event, "recording-state-changed" | "transcription-complete" | "transcription-error");
            
            // Debug payload for transcription-complete
            if event == "transcription-complete" {
                log::info!("[TRANSCRIPTION_DEBUG] Payload: {}", payload);
            }
            
            match window.emit(event, payload.clone()) {
                Ok(_) => {
                    log::info!("[FLOW] Successfully emitted '{}' event to {} window", event, window_id);
                }
                Err(e) => {
                    if is_critical {
                        log::error!("[FLOW] Failed to emit critical event '{}' to {} window: {}", event, window_id, e);
                        // For critical events, retry with app-wide emission
                        log::info!("[TRANSCRIPTION_DEBUG] Attempting app-wide emission for critical event");
                        if let Err(e2) = self.app_handle.emit(event, payload) {
                            log::error!("[FLOW] Also failed app-wide emission: {}", e2);
                        } else {
                            log::info!("[TRANSCRIPTION_DEBUG] App-wide emission succeeded");
                        }
                    } else {
                        log::warn!("[FLOW] Failed to emit '{}' event to {} window: {}", event, window_id, e);
                    }
                    return Err(e.to_string());
                }
            }
        } else {
            log::warn!("[FLOW] Cannot emit '{}' event - {} window not found", event, window_id);
            // For critical events when window not found, try app-wide emission
            if matches!(event, "recording-state-changed" | "transcription-complete" | "transcription-error") {
                log::info!("[FLOW] Attempting app-wide emission for critical event '{}'", event);
                if let Err(e) = self.app_handle.emit(event, payload) {
                    log::error!("[FLOW] App-wide emission also failed: {}", e);
                } else {
                    log::info!("[TRANSCRIPTION_DEBUG] App-wide emission succeeded for missing window");
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

    /// Update pill window position with retry logic
    pub async fn update_pill_position(&self, x: f64, y: f64) -> Result<(), String> {
        if let Some(window) = self.get_pill_window() {
            use tauri::LogicalPosition;
            
            const MAX_RETRIES: u32 = 3;
            const RETRY_DELAY_MS: u64 = 50;
            
            for attempt in 1..=MAX_RETRIES {
                match window.set_position(LogicalPosition::new(x, y)) {
                    Ok(_) => return Ok(()),
                    Err(e) => {
                        if attempt < MAX_RETRIES {
                            log::warn!("Failed to update pill position (attempt {}): {}. Retrying...", 
                                      attempt, e);
                            tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
                        } else {
                            return Err(format!("Failed to update pill position after {} attempts: {}", 
                                             MAX_RETRIES, e));
                        }
                    }
                }
            }
        }
        Ok(())
    }
    
    /// Calculate center position for pill window
    fn calculate_center_position(&self) -> (f64, f64) {
        if let Some(main_window) = self.get_main_window() {
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
        }
    }
}