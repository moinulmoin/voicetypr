use arboard::Clipboard;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

// Import rdev for more reliable keyboard simulation
use rdev::{simulate, EventType, Key as RdevKey, SimulateError};

// Import Enigo only for non-macOS platforms where it's used
#[cfg(not(target_os = "macos"))]
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};

// Global flag to prevent concurrent text insertions
static IS_INSERTING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub async fn insert_text(_app: tauri::AppHandle, text: String) -> Result<(), String> {
    // Check if already inserting text
    if IS_INSERTING.swap(true, Ordering::SeqCst) {
        log::warn!("Text insertion already in progress, skipping duplicate request");
        return Err("Text insertion already in progress".to_string());
    }

    // Ensure we reset the flag on exit
    let _guard = InsertionGuard;

    // Small delay to ensure the app doesn't interfere with text insertion
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Check accessibility permission
    #[cfg(target_os = "macos")]
    let has_accessibility_permission = {
        use crate::commands::permissions::check_accessibility_permission;
        check_accessibility_permission().await?
    };

    #[cfg(not(target_os = "macos"))]
    let has_accessibility_permission = true;

    // Move to a blocking task since clipboard operations are synchronous
    tokio::task::spawn_blocking(move || {
        // Always use clipboard method for reliability and to prevent duplicate insertion
        // This function handles both copying to clipboard and pasting at cursor
        insert_via_clipboard(text, has_accessibility_permission)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

fn insert_via_clipboard(text: String, has_accessibility_permission: bool) -> Result<(), String> {
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

    log::info!("Set clipboard content: {}", text);

    // Small delay to ensure clipboard is ready
    thread::sleep(Duration::from_millis(100));

    // Verify clipboard content was set
    if let Ok(clipboard_check) = clipboard.get_text() {
        log::info!("Clipboard content verified: {}", clipboard_check);
    }

    // Check if we have accessibility permissions before attempting to paste
    if !has_accessibility_permission {
        log::warn!(
            "No accessibility permission - text copied to clipboard but cannot paste automatically"
        );
        // Return a specific error so the caller knows it's an accessibility issue
        return Err("No accessibility permission - text copied to clipboard. Please paste manually or grant accessibility permission.".to_string());
    }

    // Try to paste using Cmd+V (macOS) with panic protection
    // Add delay since pill was just hidden

    // First try with rdev, fallback to AppleScript if it fails
    let rdev_result = try_paste_with_rdev();

    match rdev_result {
        Ok(_) => {
            log::info!("Successfully pasted with rdev");
        }
        Err(e) => {
            log::warn!("rdev paste failed: {}, trying AppleScript fallback", e);

            // Fallback to AppleScript
            let paste_result =
                panic::catch_unwind(AssertUnwindSafe(|| try_paste_with_applescript()));

            match paste_result {
                Ok(Ok(_)) => {
                    log::info!("Successfully pasted with AppleScript");
                }
                Ok(Err(e)) => {
                    log::warn!("AppleScript paste failed: {}, text remains in clipboard", e);
                    // Don't fail - text is still in clipboard for manual paste
                }
                Err(panic_err) => {
                    log::error!(
                        "PANIC during paste: {:?}, text remains in clipboard",
                        panic_err
                    );
                    // Don't fail - text is still in clipboard for manual paste
                }
            }
        }
    }

    // Restore original clipboard content after a delay
    if let Some(original) = original_clipboard {
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(200)); // Delay before restoring clipboard
            if let Ok(mut clipboard) = Clipboard::new() {
                if let Err(e) = clipboard.set_text(&original) {
                    log::error!("Failed to restore original clipboard: {}", e);
                }
            }
        });
    }

    Ok(())
}

fn try_paste_with_applescript() -> Result<(), String> {
    // Use AppleScript on macOS
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
        enigo
            .key(Key::Control, Press)
            .map_err(|e| format!("Failed to press Control: {:?}", e))?;
        thread::sleep(Duration::from_millis(20));
        enigo
            .key(Key::Unicode('v'), Click)
            .map_err(|e| format!("Failed to click V: {:?}", e))?;
        thread::sleep(Duration::from_millis(20));
        enigo
            .key(Key::Control, Release)
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

// Helper function to send events with proper timing (only used on Windows/Linux)
#[cfg(not(target_os = "macos"))]
fn send_key_event(event_type: &EventType) -> Result<(), SimulateError> {
    match simulate(event_type) {
        Ok(()) => {
            // Let the OS catch up - critical for proper key recognition
            thread::sleep(Duration::from_millis(50));
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to send event {:?}: {:?}", event_type, e);
            Err(e)
        }
    }
}

// rdev implementation for more reliable paste
fn try_paste_with_rdev() -> Result<(), String> {
    log::debug!("Attempting paste with rdev");

    // Add a small delay to ensure the app is not in focus
    thread::sleep(Duration::from_millis(100));

    #[cfg(target_os = "macos")]
    {
        paste_mac().map_err(|e| format!("Failed to paste on macOS: {:?}", e))
    }

    #[cfg(target_os = "windows")]
    {
        paste_windows().map_err(|e| format!("Failed to paste on Windows: {:?}", e))
    }

    #[cfg(target_os = "linux")]
    {
        paste_linux().map_err(|e| format!("Failed to paste on Linux: {:?}", e))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err("Unsupported platform for rdev".to_string())
    }
}

#[cfg(target_os = "macos")]
fn paste_mac() -> Result<(), SimulateError> {
    log::debug!("Starting macOS paste simulation with rdev");

    // Clear any existing key states first
    thread::sleep(Duration::from_millis(100));

    // Press Cmd (Meta) key first and hold it
    log::debug!("Pressing MetaLeft (Cmd)");
    simulate(&EventType::KeyPress(RdevKey::MetaLeft))?;
    thread::sleep(Duration::from_millis(50)); // Give OS time to register modifier

    // While Cmd is held, press V
    log::debug!("Pressing KeyV while Cmd is held");
    simulate(&EventType::KeyPress(RdevKey::KeyV))?;
    thread::sleep(Duration::from_millis(50));

    // Release V first
    log::debug!("Releasing KeyV");
    simulate(&EventType::KeyRelease(RdevKey::KeyV))?;
    thread::sleep(Duration::from_millis(50));

    // Then release Cmd
    log::debug!("Releasing MetaLeft (Cmd)");
    simulate(&EventType::KeyRelease(RdevKey::MetaLeft))?;
    thread::sleep(Duration::from_millis(50));

    log::debug!("macOS paste simulation completed");
    Ok(())
}

#[cfg(target_os = "windows")]
fn paste_windows() -> Result<(), SimulateError> {
    log::debug!("Starting Windows paste simulation with rdev");
    send_key_event(&EventType::KeyPress(RdevKey::ControlLeft))?;
    send_key_event(&EventType::KeyPress(RdevKey::KeyV))?;
    send_key_event(&EventType::KeyRelease(RdevKey::KeyV))?;
    send_key_event(&EventType::KeyRelease(RdevKey::ControlLeft))?;
    log::debug!("Windows paste simulation completed");
    Ok(())
}

#[cfg(target_os = "linux")]
fn paste_linux() -> Result<(), SimulateError> {
    log::debug!("Starting Linux paste simulation with rdev");
    send_key_event(&EventType::KeyPress(RdevKey::ControlLeft))?;
    send_key_event(&EventType::KeyPress(RdevKey::KeyV))?;
    send_key_event(&EventType::KeyRelease(RdevKey::KeyV))?;
    send_key_event(&EventType::KeyRelease(RdevKey::ControlLeft))?;
    log::debug!("Linux paste simulation completed");
    Ok(())
}
