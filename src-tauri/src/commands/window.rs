use tauri::{AppHandle, Manager};
use crate::AppState;

#[tauri::command]
pub async fn show_pill_widget(app: AppHandle) -> Result<(), String> {
    // Get the window manager from app state
    let app_state = app.state::<AppState>();
    let window_manager = app_state.get_window_manager()
        .ok_or("Window manager not initialized")?;
    
    // Use window manager to show pill window
    window_manager.show_pill_window().await?;
    
    log::info!("Pill widget shown via WindowManager");
    Ok(())
}

#[tauri::command]
pub async fn hide_pill_widget(app: AppHandle) -> Result<(), String> {
    // Get the window manager from app state
    let app_state = app.state::<AppState>();
    let window_manager = app_state.get_window_manager()
        .ok_or("Window manager not initialized")?;
    
    // Use window manager to hide pill window
    window_manager.hide_pill_window().await?;
    
    log::info!("Pill widget hidden via WindowManager");
    Ok(())
}

#[tauri::command]
pub async fn close_pill_widget(app: AppHandle) -> Result<(), String> {
    // Get the window manager from app state
    let app_state = app.state::<AppState>();
    let window_manager = app_state.get_window_manager()
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
    let window_manager = app_state.get_window_manager()
        .ok_or("Window manager not initialized")?;
    
    if let Some(main_window) = window_manager.get_main_window() {
        main_window.show().map_err(|e| e.to_string())?;
        main_window.set_focus().map_err(|e| e.to_string())?;
    }
    
    Ok(())
}