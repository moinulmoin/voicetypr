use crate::AppState;
use crate::utils::logger::*;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub async fn show_pill_widget(app: AppHandle) -> Result<(), String> {
    log_start("WINDOW_LIFECYCLE");
    log_with_context(log::Level::Debug, "Window lifecycle operation", &[
        ("operation", "show_pill_widget"),
        ("window_type", "pill"),
        ("action", "show")
    ]);

    // Get the window manager from app state
    let app_state = app.state::<AppState>();
    let window_manager = app_state
        .get_window_manager()
        .ok_or_else(|| {
            let error = "Window manager not initialized";
            log_failed("WINDOW_LIFECYCLE", error);
            log_with_context(log::Level::Error, "Window manager error", &[
                ("operation", "show_pill_widget"),
                ("error", error)
            ]);
            error.to_string()
        })?;

    // Use window manager to show pill window
    match window_manager.show_pill_window().await {
        Ok(_) => {
            log_complete("WINDOW_LIFECYCLE", 0); // No timing needed for this operation
            log_with_context(log::Level::Info, "Window shown successfully", &[
                ("operation", "show_pill_widget"),
                ("window_type", "pill"),
                ("result", "success")
            ]);
            log::info!("Pill widget shown via WindowManager");
            Ok(())
        }
        Err(e) => {
            log_failed("WINDOW_LIFECYCLE", &e);
            log_with_context(log::Level::Error, "Window show failed", &[
                ("operation", "show_pill_widget"),
                ("window_type", "pill"),
                ("error", &e)
            ]);
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn hide_pill_widget(app: AppHandle) -> Result<(), String> {
    log_start("WINDOW_LIFECYCLE");
    log_with_context(log::Level::Debug, "Window lifecycle operation", &[
        ("operation", "hide_pill_widget"),
        ("window_type", "pill"),
        ("action", "hide")
    ]);

    // Get the window manager from app state
    let app_state = app.state::<AppState>();
    let window_manager = app_state
        .get_window_manager()
        .ok_or_else(|| {
            let error = "Window manager not initialized";
            log_failed("WINDOW_LIFECYCLE", error);
            log_with_context(log::Level::Error, "Window manager error", &[
                ("operation", "hide_pill_widget"),
                ("error", error)
            ]);
            error.to_string()
        })?;

    // Use window manager to hide pill window
    match window_manager.hide_pill_window().await {
        Ok(_) => {
            log_complete("WINDOW_LIFECYCLE", 0);
            log_with_context(log::Level::Info, "Window hidden successfully", &[
                ("operation", "hide_pill_widget"),
                ("window_type", "pill"),
                ("result", "success")
            ]);
            log::info!("Pill widget hidden via WindowManager");
            Ok(())
        }
        Err(e) => {
            log_failed("WINDOW_LIFECYCLE", &e);
            log_with_context(log::Level::Error, "Window hide failed", &[
                ("operation", "hide_pill_widget"),
                ("window_type", "pill"),
                ("error", &e)
            ]);
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn close_pill_widget(app: AppHandle) -> Result<(), String> {
    // Get the window manager from app state
    let app_state = app.state::<AppState>();
    let window_manager = app_state
        .get_window_manager()
        .ok_or("Window manager not initialized")?;

    // Use window manager to close pill window
    window_manager.close_pill_window().await?;

    // IMPORTANT: Ensure main window stays hidden
    // macOS may try to activate the main window when the pill closes
    if let Some(main_window) = window_manager.get_main_window() {
        // Check if main window is hidden before pill closes
        if !main_window.is_visible().unwrap_or(true) {
            // Keep it hidden
            let _ = main_window.hide();
            log::info!("Ensured main window stays hidden after pill close");
        }
    }

    log::info!("Pill widget closed via WindowManager");
    Ok(())
}

// Note: update_pill_position has been removed since pill position is now fixed at center-bottom
// This was a design decision made during security review to simplify the codebase

#[tauri::command]
pub async fn focus_main_window(app: AppHandle) -> Result<(), String> {
    // Get the window manager from app state
    let app_state = app.state::<AppState>();
    let window_manager = app_state
        .get_window_manager()
        .ok_or("Window manager not initialized")?;

    if let Some(main_window) = window_manager.get_main_window() {
        // Check if window is already visible to avoid duplicate animations
        let is_visible = main_window.is_visible().unwrap_or(false);

        if !is_visible {
            main_window.show().map_err(|e| e.to_string())?;
        }

        // Always set focus, even if already visible
        main_window.set_focus().map_err(|e| e.to_string())?;
    }

    Ok(())
}
