use tauri::{AppHandle, LogicalPosition, Manager, WebviewUrl, WebviewWindowBuilder};

#[tauri::command]
pub async fn show_pill_widget(app: AppHandle) -> Result<(), String> {
    // Check if pill window already exists
    if app.get_webview_window("pill").is_some() {
        return Ok(());
    }

    // Get screen dimensions to position the pill
    if let Some(main_window) = app.get_webview_window("main") {
        let monitor = main_window
            .current_monitor()
            .map_err(|e| e.to_string())?
            .ok_or("No monitor found")?;
            
        let screen_size = monitor.size();
        let scale_factor = monitor.scale_factor();
        
        // Position in top-right corner
        let pill_width = 60.0;
        let pill_height = 36.0;
        let margin = 20.0;
        
        let x = (screen_size.width as f64 / scale_factor) - pill_width - margin;
        let y = margin;

        // Create the pill window
        WebviewWindowBuilder::new(&app, "pill", WebviewUrl::App("pill".into()))
            .title("Recording")
            .resizable(false)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .transparent(true)
            .inner_size(pill_width, pill_height)
            .position(x, y)
            .build()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn hide_pill_widget(app: AppHandle) -> Result<(), String> {
    if let Some(pill_window) = app.get_webview_window("pill") {
        pill_window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn update_pill_position(app: AppHandle, x: f64, y: f64) -> Result<(), String> {
    if let Some(pill_window) = app.get_webview_window("pill") {
        pill_window
            .set_position(LogicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}