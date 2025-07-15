use arboard::Clipboard;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use std::panic::{self, AssertUnwindSafe};

// Only import Enigo on non-macOS platforms
#[cfg(not(target_os = "macos"))]
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};

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
    
    // Small delay to ensure the app doesn't interfere with text insertion
    thread::sleep(Duration::from_millis(100));

    // Move to a blocking task since clipboard operations are synchronous
    tokio::task::spawn_blocking(move || {
        // Always use clipboard method for reliability and to prevent duplicate insertion
        // This function handles both copying to clipboard and pasting at cursor
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

    // Small delay to ensure clipboard is ready
    thread::sleep(Duration::from_millis(100));

    // Check if we have accessibility permissions before attempting to paste
    #[cfg(target_os = "macos")]
    {
        use crate::commands::permissions::check_accessibility_permission;
        
        // Check permission synchronously from sync context
        let has_permission = match std::thread::spawn(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                check_accessibility_permission().await
            })
        }).join() {
            Ok(Ok(has_perm)) => has_perm,
            Ok(Err(_)) => false,
            Err(_) => false
        };
        
        if !has_permission {
            log::warn!("No accessibility permission - text copied to clipboard but cannot paste automatically");
            // Text is in clipboard, user can paste manually
            return Ok(());
        }
    }

    // Try to paste using Cmd+V (macOS) with panic protection
    // Add delay since pill was just hidden
    
    let paste_result = panic::catch_unwind(AssertUnwindSafe(|| {
        try_paste_with_enigo()
    }));
    
    match paste_result {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            log::warn!("Enigo paste failed: {}, text remains in clipboard", e);
            // Don't fail - text is still in clipboard for manual paste
        }
        Err(panic_err) => {
            log::error!("PANIC during paste: {:?}, text remains in clipboard", panic_err);
            // Don't fail - text is still in clipboard for manual paste
        }
    }

    // Restore original clipboard content after a delay
    if let Some(original) = original_clipboard {
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(200)); // Delay before restoring clipboard
            if let Ok(mut clipboard) = Clipboard::new() {
                let _ = clipboard.set_text(&original);
            }
        });
    }

    Ok(())
}

fn try_paste_with_enigo() -> Result<(), String> {
    // Use AppleScript instead of Enigo on macOS
    #[cfg(target_os = "macos")]
    {
        log::debug!("Using AppleScript for keyboard simulation");
        
        // AppleScript to simulate Cmd+V
        let script = r#"
            tell application "System Events"
                keystroke "v" using {command down}
            end tell
        "#;
        
        match std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    // AppleScript paste successful
                    Ok(())
                } else {
                    let error = String::from_utf8_lossy(&output.stderr);
                    log::error!("AppleScript paste failed: {}", error);
                    Err(format!("AppleScript failed: {}", error))
                }
            }
            Err(e) => {
                log::error!("Failed to run AppleScript: {}", e);
                Err(format!("Failed to run AppleScript: {}", e))
            }
        }
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        // Use Enigo on other platforms
        log::debug!("Using Enigo for keyboard simulation");
        
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialize Enigo: {:?}", e))?;
        
        // Simulate Ctrl+V on Windows/Linux
        enigo.key(Key::Control, Press)
            .map_err(|e| format!("Failed to press Control: {:?}", e))?;
        thread::sleep(Duration::from_millis(20));
        enigo.key(Key::Unicode('v'), Click)
            .map_err(|e| format!("Failed to click V: {:?}", e))?;
        thread::sleep(Duration::from_millis(20));
        enigo.key(Key::Control, Release)
            .map_err(|e| format!("Failed to release Control: {:?}", e))?;
        
        Ok(())
    }
}

// Guard to ensure IS_INSERTING is reset even if the function returns early
struct InsertionGuard;

impl Drop for InsertionGuard {
    fn drop(&mut self) {
        IS_INSERTING.store(false, Ordering::SeqCst);
    }
}
