use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Serialize)]
pub struct LogFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub date: String,
}

#[derive(Debug, Deserialize)]
pub struct LogFilters {
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub max_size_mb: Option<u64>,
}

#[tauri::command]
pub async fn get_log_files(
    app: tauri::AppHandle,
    filters: Option<LogFilters>,
) -> Result<Vec<LogFile>, String> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| format!("Failed to get log directory: {}", e))?;

    if !log_dir.exists() {
        return Ok(vec![]);
    }

    let mut log_files = Vec::new();

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

            // Only include voicetypr log files
            if file_name.starts_with("voicetypr-") && file_name.ends_with(".log") {
                let metadata = fs::metadata(&path)
                    .map_err(|e| format!("Failed to get file metadata: {}", e))?;

                // Extract date from filename (voicetypr-YYYY-MM-DD.log)
                let date_str = file_name
                    .strip_prefix("voicetypr-")
                    .and_then(|s| s.strip_suffix(".log"))
                    .unwrap_or("");

                // Apply filters if provided
                if let Some(ref filters) = filters {
                    // Date filtering
                    if let Ok(file_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                        if let Some(ref from_date) = filters.from_date {
                            if let Ok(from) = NaiveDate::parse_from_str(from_date, "%Y-%m-%d") {
                                if file_date < from {
                                    continue;
                                }
                            }
                        }

                        if let Some(ref to_date) = filters.to_date {
                            if let Ok(to) = NaiveDate::parse_from_str(to_date, "%Y-%m-%d") {
                                if file_date > to {
                                    continue;
                                }
                            }
                        }
                    }

                    // Size filtering
                    if let Some(max_size_mb) = filters.max_size_mb {
                        if metadata.len() > max_size_mb * 1024 * 1024 {
                            continue;
                        }
                    }
                }

                log_files.push(LogFile {
                    name: file_name.clone(),
                    path: path.to_string_lossy().to_string(),
                    size: metadata.len(),
                    date: date_str.to_string(),
                });
            }
        }
    }

    // Sort by date (newest first)
    log_files.sort_by(|a, b| b.date.cmp(&a.date));

    Ok(log_files)
}

#[tauri::command]
pub async fn read_log_file(path: String) -> Result<String, String> {
    let path = PathBuf::from(path);

    // Security check: ensure the path is a log file
    if !path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("voicetypr-") && n.ends_with(".log"))
        .unwrap_or(false)
    {
        return Err("Invalid log file path".to_string());
    }

    fs::read_to_string(&path).map_err(|e| format!("Failed to read log file: {}", e))
}

#[tauri::command]
pub async fn export_logs(
    app: tauri::AppHandle,
    from_date: Option<String>,
    to_date: Option<String>,
) -> Result<String, String> {
    let log_files = get_log_files(
        app.clone(),
        Some(LogFilters {
            from_date,
            to_date,
            max_size_mb: None,
        }),
    )
    .await?;

    if log_files.is_empty() {
        return Err("No log files found for the specified date range".to_string());
    }

    // Create a temporary directory for export
    let temp_dir = std::env::temp_dir().join(format!(
        "voicetypr-logs-{}",
        Local::now().format("%Y%m%d-%H%M%S")
    ));
    fs::create_dir_all(&temp_dir).map_err(|e| format!("Failed to create temp directory: {}", e))?;

    // Copy all log files to temp directory
    for log_file in &log_files {
        let source = PathBuf::from(&log_file.path);
        let dest = temp_dir.join(&log_file.name);
        fs::copy(&source, &dest).map_err(|e| format!("Failed to copy log file: {}", e))?;
    }

    // Create a summary file
    let summary_path = temp_dir.join("summary.txt");
    let summary_content = format!(
        "VoiceTypr Log Export\n\
        ===================\n\
        Export Date: {}\n\
        Total Files: {}\n\
        Date Range: {} to {}\n\n\
        Files:\n{}",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        log_files.len(),
        log_files
            .last()
            .map(|f| &f.date)
            .unwrap_or(&"N/A".to_string()),
        log_files
            .first()
            .map(|f| &f.date)
            .unwrap_or(&"N/A".to_string()),
        log_files
            .iter()
            .map(|f| format!("- {} ({:.2} MB)", f.name, f.size as f64 / 1024.0 / 1024.0))
            .collect::<Vec<_>>()
            .join("\n")
    );

    fs::write(&summary_path, summary_content)
        .map_err(|e| format!("Failed to write summary file: {}", e))?;

    Ok(temp_dir.to_string_lossy().to_string())
}

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
        std::process::Command::new("explorer")
            .arg(&log_dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    Ok(())
}
