use tauri::{AppHandle, Manager};
use crate::{emit_to_window, AppState};

#[tauri::command]
pub async fn debug_transcription_flow(app: AppHandle) -> Result<String, String> {
    let mut debug_info = String::new();
    
    // Check if we have the window manager
    let app_state = app.state::<AppState>();
    if let Some(wm) = app_state.get_window_manager() {
        debug_info.push_str("✓ Window manager is initialized\n");
        
        // Check if pill window exists
        if wm.has_pill_window() {
            debug_info.push_str("✓ Pill window exists\n");
            
            // Check if pill window is visible
            if wm.is_pill_visible() {
                debug_info.push_str("✓ Pill window is visible\n");
            } else {
                debug_info.push_str("✗ Pill window is NOT visible\n");
            }
        } else {
            debug_info.push_str("✗ Pill window does NOT exist\n");
        }
    } else {
        debug_info.push_str("✗ Window manager is NOT initialized\n");
    }
    
    // Test event emission
    debug_info.push_str("\nTesting event emission to pill window...\n");
    let test_result = emit_to_window(&app, "pill", "test-event", 
        serde_json::json!({
            "message": "This is a test event",
            "timestamp": chrono::Utc::now().to_rfc3339()
        })
    );
    
    match test_result {
        Ok(_) => debug_info.push_str("✓ Test event emitted successfully\n"),
        Err(e) => debug_info.push_str(&format!("✗ Test event failed: {}\n", e)),
    }
    
    // Check recording state
    let current_state = app_state.get_current_state();
    debug_info.push_str(&format!("\nCurrent recording state: {:?}\n", current_state));
    
    Ok(debug_info)
}

#[tauri::command]
pub async fn test_transcription_event(app: AppHandle, text: String) -> Result<(), String> {
    // Emit a test transcription-complete event
    log::info!("[TEST] Emitting test transcription-complete event with text: '{}'", text);
    
    let result = emit_to_window(
        &app,
        "pill",
        "transcription-complete",
        serde_json::json!({
            "text": text,
            "model": "test-model"
        }),
    );
    
    match result {
        Ok(_) => {
            log::info!("[TEST] Test transcription-complete event emitted successfully");
            Ok(())
        },
        Err(e) => {
            log::error!("[TEST] Failed to emit test transcription-complete event: {}", e);
            Err(e)
        }
    }
}