use crate::utils::logger::*;
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
        // Hold the lock for the entire operation to prevent race conditions
        let mut pill_guard = match self.pill_window.lock() {
            Ok(guard) => guard,
            Err(e) => {
                let msg = format!("Pill window mutex is poisoned: {}", e);
                log::error!("{}", msg);
                return Err(msg);
            }
        };

        // First check if we have a cached reference and if it's still valid
        if let Some(ref existing_window) = *pill_guard {
            // Verify the window still exists and is not closed
            if existing_window.is_closable().is_ok() {
                // Window is still valid, show it
                existing_window.show().map_err(|e| e.to_string())?;

                // Always position at center-bottom
                use tauri::LogicalPosition;
                let (x, y) = self.calculate_center_position();
                let _ = existing_window.set_position(LogicalPosition::new(x, y));

                log::debug!("Showing existing pill window from cache");
                return Ok(());
            } else {
                // Window reference is stale, clear it
                log::debug!("Cached pill window reference is stale, clearing");
                *pill_guard = None;
            }
        }

        // Check if Tauri has a window with the "pill" label
        if let Some(existing_window) = self.app_handle.get_webview_window("pill") {
            // Verify this window is valid before using it
            if existing_window.is_closable().is_ok() {
                // Window exists in Tauri and is valid, update our reference
                *pill_guard = Some(existing_window.clone());

                existing_window.show().map_err(|e| e.to_string())?;

                // Always position at center-bottom
                use tauri::LogicalPosition;
                let (x, y) = self.calculate_center_position();
                let _ = existing_window.set_position(LogicalPosition::new(x, y));

                log::debug!("Found and showing existing pill window from Tauri");
                return Ok(());
            }
        }

        // No window exists, create new one
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

        // Store the window reference
        *pill_guard = Some(pill_window);

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
        if matches!(
            event,
            "recording-state-changed" | "transcription-complete" | "transcription-error"
        ) {
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
            let is_critical = matches!(
                event,
                "recording-state-changed" | "transcription-complete" | "transcription-error"
            );

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
            let is_critical = matches!(
                event,
                "recording-state-changed" | "transcription-complete" | "transcription-error"
            );

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

    /// Calculate center bottom position for pill window
    fn calculate_center_position(&self) -> (f64, f64) {
        // Try to get monitor from main window
        if let Some(main_window) = self.get_main_window() {
            if let Ok(Some(monitor)) = main_window.current_monitor() {
                let size = monitor.size();
                let scale = monitor.scale_factor();
                let width = size.width as f64 / scale;
                let height = size.height as f64 / scale;

                // Center bottom position with offset
                let pill_width = 80.0;
                let pill_height = 40.0;
                let bottom_offset = 25.0; // Closer to bottom of screen

                let x = (width - pill_width) / 2.0;
                let y = height - pill_height - bottom_offset;

                log::info!(
                    "Calculated pill position: ({}, {}) on {}x{} screen",
                    x,
                    y,
                    width,
                    height
                );
                (x, y)
            } else {
                log::warn!("Could not get monitor from main window, trying primary monitor");
                // Try to get primary monitor
                if let Ok(Some(monitor)) = self.app_handle.primary_monitor() {
                    let size = monitor.size();
                    let scale = monitor.scale_factor();
                    let width = size.width as f64 / scale;
                    let height = size.height as f64 / scale;

                    let pill_width = 80.0;
                    let pill_height = 40.0;
                    let bottom_offset = 10.0;

                    let x = (width - pill_width) / 2.0;
                    let y = height - pill_height - bottom_offset;

                    log::info!(
                        "Using primary monitor: ({}, {}) on {}x{} screen",
                        x,
                        y,
                        width,
                        height
                    );
                    (x, y)
                } else {
                    log::error!("Could not get any monitor, using safe defaults");
                    // Safe default: try to center on common screen sizes
                    (100.0, 400.0)
                }
            }
        } else {
            log::warn!("Main window not available, using app handle to get primary monitor");
            // Try to get primary monitor directly
            if let Ok(Some(monitor)) = self.app_handle.primary_monitor() {
                let size = monitor.size();
                let scale = monitor.scale_factor();
                let width = size.width as f64 / scale;
                let height = size.height as f64 / scale;

                let pill_width = 80.0;
                let pill_height = 40.0;
                let bottom_offset = 20.0;

                let x = (width - pill_width) / 2.0;
                let y = height - pill_height - bottom_offset;

                log::info!(
                    "Using primary monitor (no main window): ({}, {}) on {}x{} screen",
                    x,
                    y,
                    width,
                    height
                );
                (x, y)
            } else {
                log::error!("Could not get any monitor info, using safe defaults");
                (100.0, 400.0)
            }
        }
    }
}
