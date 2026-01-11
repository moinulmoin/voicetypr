use crate::utils::logger::*;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

#[derive(Debug, Clone)]
pub struct WindowManager {
    app_handle: AppHandle,
    main_window: Arc<Mutex<Option<WebviewWindow>>>,
    pill_window: Arc<Mutex<Option<WebviewWindow>>>,
}

fn calculate_pill_position(position: &str, screen_width: f64, screen_height: f64) -> (f64, f64) {
    let pill_width = 80.0;
    let pill_height = 40.0;
    let edge_offset = 60.0; // Distance from top/bottom edge

    let x = (screen_width - pill_width) / 2.0;
    let y = match position {
        "top" => edge_offset,
        "center" => (screen_height - pill_height) / 2.0,
        "bottom" | _ => screen_height - pill_height - edge_offset,
    };

    (x, y)
}

impl WindowManager {
    pub fn new(app_handle: AppHandle) -> Self {
        log_with_context(
            log::Level::Info,
            "Window manager initialization",
            &[("operation", "WINDOW_MANAGER_INIT"), ("stage", "startup")],
        );

        // Get reference to main window on creation
        let main_window = app_handle.get_webview_window("main");

        let main_available = main_window.is_some();
        log_with_context(
            log::Level::Debug,
            "Window manager setup",
            &[
                (
                    "main_window_available",
                    &main_available.to_string().as_str(),
                ),
                ("pill_window_created", "false"),
            ],
        );

        let window_manager = Self {
            app_handle,
            main_window: Arc::new(Mutex::new(main_window)),
            pill_window: Arc::new(Mutex::new(None)),
        };

        log_with_context(
            log::Level::Info,
            "Window manager ready",
            &[("operation", "WINDOW_MANAGER_INIT"), ("result", "success")],
        );

        window_manager
    }

    /// Get the main window reference
    pub fn get_main_window(&self) -> Option<WebviewWindow> {
        match self.main_window.lock() {
            Ok(guard) => guard.clone(),
            Err(e) => {
                log::error!("Main window mutex is poisoned: {}", e);
                None
            }
        }
    }

    /// Get the pill window reference (validates window is still alive)
    pub fn get_pill_window(&self) -> Option<WebviewWindow> {
        let mut pill_guard = match self.pill_window.lock() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Pill window mutex is poisoned: {}", e);
                return None;
            }
        };

        // Check if the window reference is still valid
        if let Some(ref window) = *pill_guard {
            // Verify the window still exists
            if window.is_closable().is_ok() {
                return Some(window.clone());
            } else {
                // Window is no longer valid, clear the reference
                log::debug!("Pill window reference is stale, clearing");
                *pill_guard = None;
            }
        }

        // Fallback: Check if pill window exists in Tauri but not in our cache
        if let Some(window) = self.app_handle.get_webview_window("pill") {
            if window.is_closable().is_ok() {
                log::debug!("Found pill window via app_handle fallback, caching it");
                *pill_guard = Some(window.clone());
                return Some(window);
            }
        }

        None
    }

    /// Check if pill window exists and is valid
    pub fn has_pill_window(&self) -> bool {
        self.get_pill_window().is_some()
    }

    /// Store the pill window reference
    pub fn set_pill_window(&self, window: WebviewWindow) {
        let mut pill_guard = match self.pill_window.lock() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to lock pill window mutex for storing window: {}", e);
                return;
            }
        };
        *pill_guard = Some(window);
        log::debug!("Stored pill window reference");
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
                        log::warn!(
                            "Failed to show pill window (attempt {}): {}. Retrying...",
                            attempt,
                            e
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
                    } else {
                        return Err(format!(
                            "Failed to show pill window after {} attempts: {}",
                            MAX_RETRIES, e
                        ));
                    }
                }
            }
        }

        unreachable!()
    }

    /// Internal implementation of show_pill_window
    async fn show_pill_window_internal(&self) -> Result<(), String> {
        // First, check if we have a valid existing window (hold lock briefly)
        {
            let mut pill_guard = match self.pill_window.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    let msg = format!("Pill window mutex is poisoned: {}", e);
                    log::error!("{}", msg);
                    return Err(msg);
                }
            };

            // Check if we have an existing valid pill window
            let existing_valid = if let Some(ref existing_window) = *pill_guard {
                existing_window.is_closable().is_ok()
            } else if let Some(existing_window) = self.app_handle.get_webview_window("pill") {
                if existing_window.is_closable().is_ok() {
                    *pill_guard = Some(existing_window);
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if existing_valid {
                if let Some(ref existing_window) = *pill_guard {
                    // Window exists and is valid - just show it and reposition
                    existing_window.show().map_err(|e| e.to_string())?;

                    // Always position at center-bottom
                    use tauri::LogicalPosition;
                    let (x, y) = self.calculate_center_position();
                    let _ = existing_window.set_position(LogicalPosition::new(x, y));

                    log::debug!("Showing existing pill window");
                    return Ok(());
                }
            }

            // Clear stale reference if any
            *pill_guard = None;
            // Guard is dropped here at end of block
        }

        // Close any orphaned pill window that might exist in Tauri but not in our cache
        // (This is outside the lock so the await is safe)
        if let Some(orphan) = self.app_handle.get_webview_window("pill") {
            log::debug!("Closing orphaned pill window before creating new one");
            let _ = orphan.close();
            // Small delay to ensure window is fully closed
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        // Create new window
        log::info!("Creating new pill window (lazy-loaded on recording start)");

        // Always use fixed center-bottom position
        let (position_x, position_y) = self.calculate_center_position();
        log::info!(
            "Positioning pill at center-bottom: ({}, {})",
            position_x,
            position_y
        );

        let pill_builder = WebviewWindowBuilder::new(
            &self.app_handle,
            "pill",
            WebviewUrl::App("pill.html".into()),
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
        .shadow(false) // Disabled to fix Windows transparency issue
        .skip_taskbar(true)
        .inner_size(80.0, 40.0)
        .position(position_x, position_y)
        .visible(true) // Start visible
        .focused(false); // Don't steal focus

        // Disable context menu only in production builds
        #[cfg(not(debug_assertions))]
        let pill_builder = pill_builder.initialization_script(
            "document.addEventListener('contextmenu', e => e.preventDefault());",
        );

        #[cfg(debug_assertions)]
        let pill_builder = pill_builder;

        let pill_window = pill_builder.build().map_err(|e| e.to_string())?;

        // Convert to NSPanel on macOS
        #[cfg(target_os = "macos")]
        {
            use tauri_nspanel::WebviewWindowExt;

            pill_window
                .to_panel()
                .map_err(|e| format!("Failed to convert to NSPanel: {:?}", e))?;

            log::info!("Converted pill window to NSPanel");
        }

        // Apply Windows-specific window flags to prevent focus stealing
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::*;

            if let Ok(hwnd) = pill_window.hwnd() {
                unsafe {
                    let hwnd = HWND(hwnd.0 as isize);

                    // Validate HWND before using it
                    if IsWindow(hwnd).as_bool() {
                        // Get current window style
                        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);

                        // Add tool window and no-activate flags, remove from Alt-Tab
                        let new_style =
                            (style | WS_EX_TOOLWINDOW.0 as isize | WS_EX_NOACTIVATE.0 as isize)
                                & !(WS_EX_APPWINDOW.0 as isize);

                        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style);

                        // Force window to update with new styles
                        SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED,
                        );

                        log::info!("Applied Windows-specific window flags for pill");
                    } else {
                        log::warn!("Invalid HWND received from Tauri window");
                    }
                }
            }
        }

        // Show the window after NSPanel conversion
        pill_window.show().map_err(|e| e.to_string())?;

        // Set always on top again to ensure it's visible
        pill_window
            .set_always_on_top(true)
            .map_err(|e| e.to_string())?;

        // Emit current recording state to the pill window
        let app_state = pill_window.app_handle().state::<crate::AppState>();
        let current_state = app_state.get_current_state();
        let _ = pill_window.emit(
            "recording-state-changed",
            serde_json::json!({
                "state": match current_state {
                    crate::RecordingState::Idle => "idle",
                    crate::RecordingState::Starting => "starting",
                    crate::RecordingState::Recording => "recording",
                    crate::RecordingState::Stopping => "stopping",
                    crate::RecordingState::Transcribing => "transcribing",
                    crate::RecordingState::Error => "error",
                },
                "error": null
            }),
        );

        // Store the window reference (re-acquire lock)
        {
            match self.pill_window.lock() {
                Ok(mut pill_guard) => {
                    *pill_guard = Some(pill_window);
                }
                Err(e) => {
                    log::error!("Pill window mutex poisoned while storing window: {}", e);
                }
            }
        }

        log::info!(
            "Pill window created and shown at ({}, {})",
            position_x,
            position_y
        );

        // After creating the pill window, flush any queued critical events in the background
        let app_for_flush = self.app_handle.clone();
        tauri::async_runtime::spawn(async move {
            crate::flush_pill_event_queue(&app_for_flush).await;
        });

        Ok(())
    }

    /// Hide the pill window (don't close it) with retry logic
    pub async fn hide_pill_window(&self) -> Result<(), String> {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY_MS: u64 = 50;

        for attempt in 1..=MAX_RETRIES {
            // Get the window reference inside the retry loop to handle stale references
            let window = {
                match self.pill_window.lock() {
                    Ok(pill_guard) => pill_guard.clone(),
                    Err(e) => {
                        log::error!("Pill window mutex is poisoned during hide: {}", e);
                        break;
                    }
                }
            };

            if let Some(window) = window {
                // Verify window is still valid before trying to hide
                if window.is_closable().is_err() {
                    // Window is no longer valid, clear the reference
                    if let Ok(mut pill_guard) = self.pill_window.lock() {
                        *pill_guard = None;
                        log::debug!("Pill window reference is stale during hide, cleared");
                    }
                    return Ok(());
                }

                match window.hide() {
                    Ok(_) => {
                        log::info!("Pill window hidden");
                        return Ok(());
                    }
                    Err(e) => {
                        // Check if error is because window was closed
                        if !window.is_closable().unwrap_or(false) {
                            // Window was closed, clear the reference
                            if let Ok(mut pill_guard) = self.pill_window.lock() {
                                *pill_guard = None;
                                log::debug!("Pill window was closed during hide attempt");
                            }
                            return Ok(());
                        }

                        if attempt < MAX_RETRIES {
                            log::warn!(
                                "Failed to hide pill window (attempt {}): {}. Retrying...",
                                attempt,
                                e
                            );
                            tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS))
                                .await;
                        } else {
                            return Err(format!(
                                "Failed to hide pill window after {} attempts: {}",
                                MAX_RETRIES, e
                            ));
                        }
                    }
                }
            } else {
                // No window to hide
                return Ok(());
            }
        }

        Ok(())
    }

    /// Close the pill window (actually destroy it)
    pub async fn close_pill_window(&self) -> Result<(), String> {
        // Take the window out of the mutex
        let window = {
            match self.pill_window.lock() {
                Ok(mut pill_guard) => pill_guard.take(),
                Err(e) => {
                    log::error!("Pill window mutex is poisoned during close: {}", e);
                    return Ok(());
                }
            }
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
    pub fn emit_to_window(
        &self,
        window_id: &str,
        event: &str,
        payload: serde_json::Value,
    ) -> Result<(), String> {
        // Only log critical events
        if matches!(event, "recording-state-changed" | "transcription-complete") {
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
            let is_critical =
                matches!(event, "recording-state-changed" | "transcription-complete");

            // Check if window is visible or if it's a critical event

            match window.emit(event, payload.clone()) {
                Ok(_) => {}
                Err(e) => {
                    if is_critical {
                        log::error!(
                            "[FLOW] Failed to emit critical event '{}' to {} window: {}",
                            event,
                            window_id,
                            e
                        );
                        // For critical events, retry with app-wide emission
                        if let Err(e2) = self.app_handle.emit(event, payload) {
                            log::error!("Also failed app-wide emission: {}", e2);
                        }
                    } else {
                        log::debug!(
                            "Failed to emit '{}' event to {} window: {}",
                            event,
                            window_id,
                            e
                        );
                    }
                    return Err(e.to_string());
                }
            }
        } else {
            log::debug!(
                "Cannot emit '{}' event - {} window not found",
                event,
                window_id
            );
            // For critical events when window not found, try app-wide emission
            let is_critical =
                matches!(event, "recording-state-changed" | "transcription-complete");

            // Queue critical pill events so they can be delivered when the pill window is created
            if is_critical && window_id == "pill" {
                let app_state = self.app_handle.state::<crate::AppState>();
                app_state.queue_pill_event(event, payload.clone());
            }

            if is_critical {
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
        let pill_guard = match self.pill_window.lock() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!(
                    "Pill window mutex is poisoned during visibility check: {}",
                    e
                );
                return false;
            }
        };

        if let Some(ref window) = *pill_guard {
            // Check both that window is valid and visible
            if window.is_closable().is_ok() {
                window.is_visible().unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Get the current pill indicator position from settings
    fn get_pill_position_setting(&self) -> String {
        use tauri_plugin_store::StoreExt;
        if let Ok(store) = self.app_handle.store("settings") {
            store
                .get("pill_indicator_position")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "bottom".to_string())
        } else {
            "bottom".to_string()
        }
    }

    /// Calculate position for pill window based on position setting
    /// position: "top", "center", or "bottom"
    fn calculate_position_for(&self, position: &str) -> (f64, f64) {
        // Get screen dimensions
        let (screen_width, screen_height) = self.get_screen_dimensions();
        let (x, y) = calculate_pill_position(position, screen_width, screen_height);

        log::info!(
            "Calculated pill position: ({}, {}) for '{}' on {}x{} screen",
            x,
            y,
            position,
            screen_width,
            screen_height
        );
        (x, y)
    }

    /// Get screen dimensions from available monitors
    fn get_screen_dimensions(&self) -> (f64, f64) {
        // Try to get monitor from main window
        if let Some(main_window) = self.get_main_window() {
            if let Ok(Some(monitor)) = main_window.current_monitor() {
                let size = monitor.size();
                let scale = monitor.scale_factor();
                return (size.width as f64 / scale, size.height as f64 / scale);
            }
        }

        // Fallback to primary monitor
        if let Ok(Some(monitor)) = self.app_handle.primary_monitor() {
            let size = monitor.size();
            let scale = monitor.scale_factor();
            return (size.width as f64 / scale, size.height as f64 / scale);
        }

        // Safe default for common screen sizes
        log::error!("Could not get any monitor info, using safe defaults");
        (1920.0, 1080.0)
    }

    /// Calculate center position for pill window using current settings
    fn calculate_center_position(&self) -> (f64, f64) {
        let position = self.get_pill_position_setting();
        self.calculate_position_for(&position)
    }

    /// Reposition pill and toast windows to current monitor center-bottom.
    /// Called when monitor configuration changes (display connect/disconnect, resolution change).
    pub fn reposition_floating_windows(&self) {
        use tauri::LogicalPosition;

        let (pill_x, pill_y) = self.calculate_center_position();

        // Reposition pill window
        if let Some(pill) = self.get_pill_window() {
            if let Err(e) = pill.set_position(LogicalPosition::new(pill_x, pill_y)) {
                log::warn!("Failed to reposition pill window: {}", e);
            } else {
                log::info!("Repositioned pill window to ({}, {})", pill_x, pill_y);
            }
        }

        // Reposition toast window (above pill)
        if let Some(toast) = self.app_handle.get_webview_window("toast") {
            let toast_width = 400.0;
            let toast_height = 80.0;
            let pill_width = 80.0;
            let gap = 8.0;
            let toast_x = pill_x + (pill_width - toast_width) / 2.0;
            let toast_y = pill_y - toast_height - gap;

            if let Err(e) = toast.set_position(LogicalPosition::new(toast_x, toast_y)) {
                log::warn!("Failed to reposition toast window: {}", e);
            } else {
                log::info!("Repositioned toast window to ({}, {})", toast_x, toast_y);
            }
        }
    }

    /// Reposition pill and toast windows to a specific position.
    /// Called when the pill_indicator_position setting changes.
    pub fn reposition_floating_windows_with_position(&self, position: &str) {
        use tauri::LogicalPosition;

        let (pill_x, pill_y) = self.calculate_position_for(position);

        // Reposition pill window
        if let Some(pill) = self.get_pill_window() {
            if let Err(e) = pill.set_position(LogicalPosition::new(pill_x, pill_y)) {
                log::warn!("Failed to reposition pill window: {}", e);
            } else {
                log::info!("Repositioned pill window to ({}, {}) for position '{}'", pill_x, pill_y, position);
            }
        }

        // Reposition toast window (above or below pill depending on position)
        if let Some(toast) = self.app_handle.get_webview_window("toast") {
            let toast_width = 400.0;
            let toast_height = 80.0;
            let pill_width = 80.0;
            let gap = 8.0;
            let toast_x = pill_x + (pill_width - toast_width) / 2.0;
            // If pill is at top, put toast below; otherwise put toast above
            let toast_y = if position == "top" {
                pill_y + 40.0 + gap // Below pill
            } else {
                pill_y - toast_height - gap // Above pill
            };

            if let Err(e) = toast.set_position(LogicalPosition::new(toast_x, toast_y)) {
                log::warn!("Failed to reposition toast window: {}", e);
            } else {
                log::info!("Repositioned toast window to ({}, {})", toast_x, toast_y);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::calculate_pill_position;

    #[test]
    fn calculate_pill_position_top() {
        let (x, y) = calculate_pill_position("top", 1920.0, 1080.0);
        assert_eq!(x, 920.0);
        assert_eq!(y, 60.0);
    }

    #[test]
    fn calculate_pill_position_center() {
        let (x, y) = calculate_pill_position("center", 1920.0, 1080.0);
        assert_eq!(x, 920.0);
        assert_eq!(y, 520.0);
    }

    #[test]
    fn calculate_pill_position_bottom() {
        let (x, y) = calculate_pill_position("bottom", 1920.0, 1080.0);
        assert_eq!(x, 920.0);
        assert_eq!(y, 980.0);
    }

    #[test]
    fn calculate_pill_position_defaults_to_bottom() {
        let (x, y) = calculate_pill_position("unknown", 1920.0, 1080.0);
        assert_eq!(x, 920.0);
        assert_eq!(y, 980.0);
    }
}
