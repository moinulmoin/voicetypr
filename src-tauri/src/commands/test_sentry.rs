/// Test commands to verify Sentry error tracking

#[tauri::command]
pub async fn test_sentry_errors() -> Result<String, String> {
    use crate::capture_sentry_message;
    
    // Test 1: Audio recording error
    capture_sentry_message!(
        "TEST: Audio recording initialization failure",
        tauri_plugin_sentry::sentry::Level::Error,
        tags: {
            "test" => "true",
            "error.type" => "permission_denied",
            "component" => "audio_recorder"
        }
    );
    
    // Test 2: Transcription error
    capture_sentry_message!(
        "TEST: Whisper inference failed",
        tauri_plugin_sentry::sentry::Level::Error,
        tags: {
            "test" => "true",
            "error.type" => "inference_failure",
            "component" => "transcriber"
        }
    );
    
    // Test 3: Permission error
    capture_sentry_message!(
        "TEST: Microphone permission denied",
        tauri_plugin_sentry::sentry::Level::Warning,
        tags: {
            "test" => "true",
            "permission.type" => "microphone",
            "permission.status" => "denied"
        }
    );
    
    // Test 4: Model download error
    capture_sentry_message!(
        "TEST: Model download failed after retries",
        tauri_plugin_sentry::sentry::Level::Error,
        tags: {
            "test" => "true",
            "error.type" => "download_failure",
            "component" => "model_manager"
        }
    );
    
    // Test 5: Text insertion error
    capture_sentry_message!(
        "TEST: Clipboard initialization failed",
        tauri_plugin_sentry::sentry::Level::Error,
        tags: {
            "test" => "true",
            "error.type" => "clipboard_init_failure",
            "component" => "text_insertion"
        }
    );
    
    Ok("Test Sentry errors sent successfully! Check your Sentry dashboard.".to_string())
}