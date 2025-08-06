use chrono::{Local, NaiveDate};
use std::fs;
use tauri::Manager;


#[tauri::command]
pub async fn clear_old_logs(app: tauri::AppHandle, days_to_keep: u32) -> Result<u32, String> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    if !log_dir.exists() {
        return Ok(0);
    }

    let cutoff_date = Local::now().date_naive() - chrono::Duration::days(days_to_keep as i64);
    let mut deleted_count = 0;

    let entries =
        fs::read_dir(&log_dir).map_err(|e| format!("Failed to read log directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.is_file() {
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            if file_name.starts_with("voicetypr-") && file_name.ends_with(".log") {
                let date_str = file_name
                    .strip_prefix("voicetypr-")
                    .and_then(|s| s.strip_suffix(".log"))
                    .unwrap_or("");

                if let Ok(file_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                    if file_date < cutoff_date {
                        fs::remove_file(&path)
                            .map_err(|e| format!("Failed to delete log file: {}", e))?;
                        deleted_count += 1;
                        log::info!("Deleted old log file: {}", file_name);
                    }
                }
            }
        }
    }

    Ok(deleted_count)
}

#[tauri::command]
pub async fn get_log_directory(app: tauri::AppHandle) -> Result<String, String> {
    app.path()
        .app_log_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| format!("Failed to get log directory: {}", e))
}

#[tauri::command]
pub async fn open_logs_folder(app: tauri::AppHandle) -> Result<(), String> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    // Create directory if it doesn't exist
    if !log_dir.exists() {
        std::fs::create_dir_all(&log_dir)
            .map_err(|e| format!("Failed to create log directory: {}", e))?;
    }

    // Open the directory using the system's file manager
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&log_dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        
        std::process::Command::new("explorer")
            .arg(&log_dir)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    Ok(())
}
