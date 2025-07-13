// Debug helper for transcription issues
use std::path::Path;

pub fn debug_transcription_flow(step: &str, details: &str) {
    log::info!("[TRANSCRIPTION_DEBUG] {} - {}", step, details);
}

pub fn check_audio_file(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Err(format!("Audio file does not exist: {:?}", path));
    }
    
    let metadata = std::fs::metadata(path).map_err(|e| format!("Cannot read metadata: {}", e))?;
    let size = metadata.len();
    
    if size == 0 {
        return Err("Audio file is empty (0 bytes)".to_string());
    }
    
    if size < 44 {
        return Err(format!("Audio file too small ({} bytes) - likely corrupted", size));
    }
    
    Ok(format!("Audio file OK: {} bytes at {:?}", size, path))
}

pub fn check_model_file(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Err(format!("Model file does not exist: {:?}", path));
    }
    
    let metadata = std::fs::metadata(path).map_err(|e| format!("Cannot read metadata: {}", e))?;
    let size = metadata.len();
    
    if size < 1_000_000 {  // Models should be at least 1MB
        return Err(format!("Model file suspiciously small: {} bytes", size));
    }
    
    Ok(format!("Model file OK: {} bytes at {:?}", size, path))
}