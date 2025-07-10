use arboard::Clipboard;
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};
use std::thread;
use std::time::Duration;

#[tauri::command]
pub async fn insert_text(text: String) -> Result<(), String> {
    // Small delay to ensure the app doesn't interfere with text insertion
    thread::sleep(Duration::from_millis(100));

    // Move to a blocking task since enigo operations are synchronous
    tokio::task::spawn_blocking(move || {
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialize Enigo: {:?}", e))?;

        // Try direct text typing first
        match enigo.text(&text) {
            Ok(_) => {
                log::info!("Successfully inserted text using direct typing");
                Ok(())
            },
            Err(e) => {
                log::warn!("Direct text typing failed: {:?}, falling back to clipboard method", e);
                // Fallback to clipboard method if direct typing fails
                insert_via_clipboard(text).map_err(|_| format!("Failed to insert text: {:?}", e))
            }
        }
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

fn insert_via_clipboard(text: String) -> Result<(), String> {
    // Initialize clipboard
    let mut clipboard =
        Clipboard::new().map_err(|e| format!("Failed to initialize clipboard: {}", e))?;

    // Save current clipboard content
    let original_clipboard = clipboard.get_text().ok();

    // Set new clipboard content
    clipboard
        .set_text(&text)
        .map_err(|e| format!("Failed to set clipboard: {}", e))?;

    // Small delay
    thread::sleep(Duration::from_millis(50));

    // Paste using Cmd+V (macOS)
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("Failed to initialize Enigo: {:?}", e))?;

    // Use Command key (Meta) on macOS
    enigo
        .key(Key::Meta, Press)
        .map_err(|e| format!("Failed to press Meta key: {:?}", e))?;
    enigo
        .key(Key::Unicode('v'), Click)
        .map_err(|e| format!("Failed to press V key: {:?}", e))?;
    enigo
        .key(Key::Meta, Release)
        .map_err(|e| format!("Failed to release Meta key: {:?}", e))?;

    // Restore original clipboard content after a delay
    if let Some(original) = original_clipboard {
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(500));
            if let Ok(mut clipboard) = Clipboard::new() {
                let _ = clipboard.set_text(&original);
            }
        });
    }

    Ok(())
}
