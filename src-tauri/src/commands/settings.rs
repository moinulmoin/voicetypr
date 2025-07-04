use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    pub hotkey: String,
    pub current_model: String,
    pub language: String,
    pub auto_insert: bool,
    pub show_window_on_record: bool,
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "CommandOrControl+Shift+Space".to_string(),
            current_model: "".to_string(), // Empty means auto-select
            language: "en".to_string(),
            auto_insert: true,
            show_window_on_record: false,
            theme: "system".to_string(),
        }
    }
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Result<Settings, String> {
    let store = app.store("settings")
        .map_err(|e| e.to_string())?;

    let settings = Settings {
        hotkey: store.get("hotkey")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().hotkey),
        current_model: store.get("current_model")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().current_model),
        language: store.get("language")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().language),
        auto_insert: store.get("auto_insert")
            .and_then(|v| v.as_bool())
            .unwrap_or(Settings::default().auto_insert),
        show_window_on_record: store.get("show_window_on_record")
            .and_then(|v| v.as_bool())
            .unwrap_or(Settings::default().show_window_on_record),
        theme: store.get("theme")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| Settings::default().theme),
    };

    Ok(settings)
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    settings: Settings,
) -> Result<(), String> {
    let store = app.store("settings")
        .map_err(|e| e.to_string())?;

    store.set("hotkey", json!(settings.hotkey));
    store.set("current_model", json!(settings.current_model));
    store.set("language", json!(settings.language));
    store.set("auto_insert", json!(settings.auto_insert));
    store.set("show_window_on_record", json!(settings.show_window_on_record));
    store.set("theme", json!(settings.theme));

    store.save().map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn set_global_shortcut(
    app: AppHandle,
    shortcut: String,
) -> Result<(), String> {
    // Validate shortcut format
    if shortcut.is_empty() || shortcut.len() > 100 {
        return Err("Invalid shortcut format".to_string());
    }
    let shortcuts = app.global_shortcut();

    // Unregister all existing shortcuts
    shortcuts.unregister_all().map_err(|e| e.to_string())?;

    // Register new shortcut
    let shortcut_obj: Shortcut = shortcut.parse()
        .map_err(|_| "Invalid shortcut format".to_string())?;
    shortcuts.register(shortcut_obj.clone()).map_err(|e| e.to_string())?;
    
    // Update the global recording shortcut state
    if let Some(recording_shortcut) = crate::RECORDING_SHORTCUT.get() {
        if let Ok(mut guard) = recording_shortcut.lock() {
            *guard = shortcut_obj;
        }
    }

    // Save to settings
    let store = app.store("settings").map_err(|e| e.to_string())?;
    store.set("hotkey", json!(shortcut));
    store.save().map_err(|e| e.to_string())?;

    Ok(())
}