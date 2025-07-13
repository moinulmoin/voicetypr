use arboard::Clipboard;
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

// Global flag to prevent concurrent text insertions
static IS_INSERTING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub async fn insert_text(text: String) -> Result<(), String> {
    // Check if already inserting text
    if IS_INSERTING.swap(true, Ordering::SeqCst) {
        log::warn!("Text insertion already in progress, skipping duplicate request");
        return Err("Text insertion already in progress".to_string());
    }
    
    // Ensure we reset the flag on exit
    let _guard = InsertionGuard;
    
    log::debug!("Starting text insertion with {} characters", text.len());
    
    // Small delay to ensure the app doesn't interfere with text insertion
    thread::sleep(Duration::from_millis(100));

    // Move to a blocking task since clipboard operations are synchronous
    tokio::task::spawn_blocking(move || {
        // Always use clipboard method for reliability and to prevent duplicate insertion
        // This function handles both copying to clipboard and pasting at cursor
        log::debug!("Using clipboard method for text insertion");
        insert_via_clipboard(text)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

fn insert_via_clipboard(text: String) -> Result<(), String> {
    // This function handles both copying text to clipboard AND pasting it at cursor
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

// Guard to ensure IS_INSERTING is reset even if the function returns early
struct InsertionGuard;

impl Drop for InsertionGuard {
    fn drop(&mut self) {
        IS_INSERTING.store(false, Ordering::SeqCst);
        log::debug!("Text insertion completed, guard released");
    }
}
