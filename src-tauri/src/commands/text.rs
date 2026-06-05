use arboard::Clipboard;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use tauri_plugin_store::StoreExt;

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

const CLOSING_PUNCTUATION: [char; 8] = ['"', '\'', '“', '”', '’', ')', ']', '}'];

/// Last non-closing character, allowing quotes/brackets after sentence punctuation.
fn last_meaningful_char(text: &str) -> Option<char> {
    text.chars()
        .rev()
        .find(|c| !CLOSING_PUNCTUATION.contains(c))
}

/// Detect contexts where a trailing insertion space would be harmful.
fn should_skip_trailing_insertion_space(text: &str) -> bool {
    let semantic_end = text.trim_end_matches(|c| CLOSING_PUNCTUATION.contains(&c));
    let before_period = semantic_end.strip_suffix('.').unwrap_or(semantic_end);
    let looks_like_url = semantic_end.contains("://")
        || before_period.ends_with(".com")
        || before_period.ends_with(".org")
        || before_period.ends_with(".net")
        || before_period.ends_with(".io")
        || before_period.ends_with(".dev")
        || before_period.ends_with(".app");
    // Email-like: contains @ with no spaces (even if followed by period)
    let looks_like_email = before_period.contains('@') && !before_period.contains(' ');
    let looks_like_code = semantic_end.contains('=')
        || semantic_end.contains("->")
        || semantic_end.contains("::")
        || semantic_end.contains('{')
        || semantic_end.contains('}')
        || semantic_end.contains(';');

    looks_like_url || looks_like_email || looks_like_code
}

/// Whether to append exactly one trailing space before the next insertion.
fn should_add_trailing_insertion_space(text: &str) -> bool {
    !should_skip_trailing_insertion_space(text) && last_meaningful_char(text).is_some()
}

/// Ensure insertable prose has exactly one trailing space at the insertion boundary.
///
/// This is applied at the insertion boundary only, so stored transcription
/// history remains clean. Rules:
/// - `Hello world.` → `Hello world. `
/// - `Hello world. ` → `Hello world. ` (normalize to one)
/// - `Hello world` → `Hello world ` (fragment boundary for continuous dictation)
/// - `Hello world,` → `Hello world, ` (fragment boundary for continuous dictation)
/// - `https://example.com.` → `https://example.com.` (URL-like, skip)
/// - `foo@bar.com.` → `foo@bar.com.` (email-like, skip)
/// - `x = y.` → `x = y.` (code-like, skip)
fn ensure_trailing_sentence_space(text: &str) -> String {
    let without_trailing_spaces = text.trim_end_matches(' ');
    if without_trailing_spaces.is_empty() {
        return text.to_string();
    }

    // Preserve explicit structural whitespace. The helper is for ergonomic
    // sentence spacing, not for rewriting multiline text or caller-provided
    // newlines/tabs at the insertion boundary.
    if without_trailing_spaces.ends_with('\n')
        || without_trailing_spaces.ends_with('\r')
        || without_trailing_spaces.ends_with('\t')
        || without_trailing_spaces.contains('\n')
        || without_trailing_spaces.contains('\r')
    {
        return text.to_string();
    }

    if should_add_trailing_insertion_space(without_trailing_spaces) {
        format!("{} ", without_trailing_spaces)
    } else {
        text.to_string()
    }
}

#[tauri::command]
pub async fn insert_text(app: tauri::AppHandle, text: String) -> Result<(), String> {
    // Check if already inserting text
    if IS_INSERTING.swap(true, Ordering::SeqCst) {
        log::warn!("Text insertion already in progress, skipping duplicate request");
        return Err("Text insertion already in progress".to_string());
    }

    // Ensure we reset the flag on exit
    let _guard = InsertionGuard;

    // Small delay to ensure the app doesn't interfere with text insertion
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Check accessibility permission
    #[cfg(target_os = "macos")]
    let has_accessibility_permission = {
        use crate::commands::permissions::check_accessibility_permission;
        check_accessibility_permission().await?
    };

    #[cfg(not(target_os = "macos"))]
    let has_accessibility_permission = true;

    // Move to a blocking task since clipboard operations are synchronous
    let keep_transcription_in_clipboard = {
        let store = app
            .store("settings")
            .map_err(|e| format!("Failed to access settings: {}", e))?;
        store
            .get("keep_transcription_in_clipboard")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    };

    tokio::task::spawn_blocking(move || {
        // Apply trailing sentence space only at the insertion boundary,
        // so stored transcription history remains clean.
        let insertable_text = ensure_trailing_sentence_space(&text);
        // Always use clipboard method for reliability and to prevent duplicate insertion
        // This function handles both copying to clipboard and pasting at cursor
        insert_via_clipboard(
            insertable_text,
            has_accessibility_permission,
            Some(app),
            keep_transcription_in_clipboard,
        )
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

/// Copy plain text to the system clipboard without attempting to paste
#[tauri::command]
pub async fn copy_text_to_clipboard(text: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let mut clipboard =
            Clipboard::new().map_err(|e| format!("Failed to initialize clipboard: {}", e))?;
        clipboard
            .set_text(&text)
            .map_err(|e| format!("Failed to set clipboard: {}", e))?;
        Ok(())
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

fn insert_via_clipboard(
    text: String,
    has_accessibility_permission: bool,
    app_handle: Option<tauri::AppHandle>,
    keep_transcription_in_clipboard: bool,
) -> Result<(), String> {
    // This function handles both copying text to clipboard AND pasting it at cursor
    // Initialize clipboard
    let mut clipboard =
        Clipboard::new().map_err(|e| format!("Failed to initialize clipboard: {}", e))?;

    let previous_clipboard_text = if keep_transcription_in_clipboard {
        None
    } else {
        match clipboard.get_text() {
            Ok(value) => Some(value),
            Err(err) => {
                log::debug!(
                    "Could not capture previous clipboard text (likely non-text content): {}",
                    err
                );
                None
            }
        }
    };

    let insertion_result: Result<(), String> = (|| {
        // Set transcribed text as clipboard content
        clipboard
            .set_text(&text)
            .map_err(|e| format!("Failed to set clipboard: {}", e))?;

        log::info!("Set clipboard content: {}", text);

        // Small delay to ensure clipboard is ready
        thread::sleep(Duration::from_millis(50));

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
                    panic::catch_unwind(AssertUnwindSafe(try_paste_with_applescript));

                match paste_result {
                    Ok(Ok(_)) => {
                        log::info!("Successfully pasted with AppleScript");
                    }
                    Ok(Err(e)) => {
                        log::warn!("AppleScript paste failed: {}, text remains in clipboard", e);
                        // Notify user through pill toast that paste failed but text is in clipboard
                        if let Some(app) = &app_handle {
                            crate::commands::audio::pill_toast(
                                app,
                                "Paste failed - copied to clipboard",
                                1500,
                            );
                        }
                        // Don't fail - text is still in clipboard for manual paste
                    }
                    Err(panic_err) => {
                        log::error!(
                            "PANIC during paste: {:?}, text remains in clipboard",
                            panic_err
                        );
                        // Notify user through pill toast about the failure
                        if let Some(app) = &app_handle {
                            crate::commands::audio::pill_toast(
                                app,
                                "Paste failed - copied to clipboard",
                                1500,
                            );
                        }
                        // Don't fail - text is still in clipboard for manual paste
                    }
                }
            }
        }

        Ok(())
    })();

    if !keep_transcription_in_clipboard {
        if insertion_result.is_ok() {
            if let Some(previous_text) = previous_clipboard_text {
                if let Err(e) = clipboard.set_text(&previous_text) {
                    log::error!("Failed to restore original clipboard text: {}", e);
                } else {
                    log::debug!("Restored original clipboard text after paste");
                }
            } else {
                log::debug!(
                    "No plain-text clipboard content to restore; leaving clipboard unchanged"
                );
            }
        } else {
            log::debug!(
                "Skipping clipboard restoration after paste failure; transcript remains available for manual paste"
            );
        }
    }

    insertion_result
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

    #[cfg(target_os = "linux")]
    {
        // Keep Enigo as fallback for Linux due to X11/Wayland differences
        log::debug!("Using Enigo fallback for Linux keyboard simulation");

        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialize Enigo: {:?}", e))?;

        // Simulate Ctrl+V on Linux
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

    #[cfg(target_os = "windows")]
    {
        // Windows doesn't need enigo fallback - rdev is sufficient
        log::debug!("No Enigo fallback for Windows - rdev should have worked");
        Err("Windows paste failed with rdev (no fallback needed)".to_string())
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
    let paste_start = std::time::Instant::now();
    log::info!("=== PASTE CHAIN START ===");
    log::debug!("Platform: {}", std::env::consts::OS);
    log::debug!(
        "Method: rdev primary, fallback available: {}",
        cfg!(any(target_os = "macos", target_os = "linux"))
    );

    // Add a small delay to ensure the app is not in focus
    thread::sleep(Duration::from_millis(50));

    let result = {
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
    };

    let paste_duration = paste_start.elapsed();
    match &result {
        Ok(_) => {
            log::info!("=== PASTE SUCCESS in {}ms ===", paste_duration.as_millis());
        }
        Err(e) => {
            log::warn!(
                "=== PASTE FAILED in {}ms: {} ===",
                paste_duration.as_millis(),
                e
            );
        }
    }

    result
}

#[cfg(target_os = "macos")]
fn paste_mac() -> Result<(), SimulateError> {
    log::debug!("Starting macOS paste simulation with rdev");

    // Add initial delay to match Windows timing for better reliability
    thread::sleep(Duration::from_millis(50));

    // Try paste with retry logic
    for attempt in 1..=2 {
        log::debug!("macOS paste attempt {}/2", attempt);

        let result = (|| {
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

            Ok::<(), SimulateError>(())
        })();

        match result {
            Ok(_) => {
                log::debug!("macOS paste simulation completed on attempt {}", attempt);
                return Ok(());
            }
            Err(e) if attempt < 2 => {
                log::warn!(
                    "macOS paste attempt {} failed: {:?}, retrying...",
                    attempt,
                    e
                );
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                log::error!("macOS paste failed after 2 attempts: {:?}", e);
                return Err(e);
            }
        }
    }

    unreachable!()
}

#[cfg(target_os = "windows")]
fn paste_windows() -> Result<(), SimulateError> {
    log::debug!("Starting Windows paste simulation with rdev");

    // Add initial delay to match macOS timing for better reliability
    thread::sleep(Duration::from_millis(50));

    // Try paste with retry logic
    for attempt in 1..=2 {
        log::debug!("Windows paste attempt {}/2", attempt);

        let result = (|| {
            send_key_event(&EventType::KeyPress(RdevKey::ControlLeft))?;
            send_key_event(&EventType::KeyPress(RdevKey::KeyV))?;
            send_key_event(&EventType::KeyRelease(RdevKey::KeyV))?;
            send_key_event(&EventType::KeyRelease(RdevKey::ControlLeft))?;
            Ok::<(), SimulateError>(())
        })();

        match result {
            Ok(_) => {
                log::debug!("Windows paste simulation completed on attempt {}", attempt);
                return Ok(());
            }
            Err(e) if attempt < 2 => {
                log::warn!(
                    "Windows paste attempt {} failed: {:?}, retrying...",
                    attempt,
                    e
                );
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                log::error!("Windows paste failed after 2 attempts: {:?}", e);
                return Err(e);
            }
        }
    }

    unreachable!()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentence_end_gets_trailing_space() {
        assert_eq!(
            ensure_trailing_sentence_space("Hello world."),
            "Hello world. "
        );
    }

    #[test]
    fn exclamation_gets_trailing_space() {
        assert_eq!(
            ensure_trailing_sentence_space("That's great!"),
            "That's great! "
        );
    }

    #[test]
    fn question_mark_gets_trailing_space() {
        assert_eq!(
            ensure_trailing_sentence_space("How are you?"),
            "How are you? "
        );
    }

    #[test]
    fn existing_trailing_spaces_normalize_to_one() {
        assert_eq!(
            ensure_trailing_sentence_space("Hello world.   "),
            "Hello world. "
        );
    }

    #[test]
    fn prose_fragment_gets_trailing_space() {
        assert_eq!(
            ensure_trailing_sentence_space("Hello world"),
            "Hello world "
        );
    }

    #[test]
    fn repeated_prose_fragments_keep_single_space_boundary() {
        let combined = format!(
            "{}{}",
            ensure_trailing_sentence_space("hello world"),
            ensure_trailing_sentence_space("next phrase"),
        );

        assert_eq!(combined, "hello world next phrase ");
    }

    #[test]
    fn repeated_fragments_do_not_duplicate_spaces() {
        assert_eq!(
            ensure_trailing_sentence_space("hello world   "),
            "hello world "
        );
        assert_eq!(
            ensure_trailing_sentence_space("already spaced "),
            "already spaced "
        );
    }

    #[test]
    fn comma_gets_trailing_space() {
        assert_eq!(
            ensure_trailing_sentence_space("Hello world,"),
            "Hello world, "
        );
    }

    #[test]
    fn colon_gets_trailing_space() {
        assert_eq!(ensure_trailing_sentence_space("Note:"), "Note: ");
    }

    #[test]
    fn url_no_space() {
        assert_eq!(
            ensure_trailing_sentence_space("Visit https://example.com."),
            "Visit https://example.com."
        );
    }

    #[test]
    fn email_no_space() {
        assert_eq!(
            ensure_trailing_sentence_space("user@example.com."),
            "user@example.com."
        );
    }

    #[test]
    fn code_like_no_space() {
        assert_eq!(ensure_trailing_sentence_space("x = y."), "x = y.");
    }

    #[test]
    fn multiline_no_space() {
        assert_eq!(
            ensure_trailing_sentence_space("Hello.\nWorld."),
            "Hello.\nWorld."
        );
    }

    #[test]
    fn trailing_newline_is_preserved() {
        assert_eq!(ensure_trailing_sentence_space("Hello.\n"), "Hello.\n");
    }

    #[test]
    fn closing_quote_gets_trailing_space() {
        assert_eq!(
            ensure_trailing_sentence_space("He said \"yes.\""),
            "He said \"yes.\" "
        );
    }

    #[test]
    fn curly_closing_quote_gets_trailing_space() {
        assert_eq!(
            ensure_trailing_sentence_space("He said “yes.”"),
            "He said “yes.” "
        );
    }

    #[test]
    fn closing_paren_gets_trailing_space() {
        assert_eq!(
            ensure_trailing_sentence_space("That works (really!)"),
            "That works (really!) "
        );
    }

    #[test]
    fn empty_string_unchanged() {
        assert_eq!(ensure_trailing_sentence_space(""), "");
    }

    #[test]
    fn only_whitespace_unchanged() {
        assert_eq!(ensure_trailing_sentence_space("   "), "   ");
    }

    #[test]
    fn tld_like_ending_no_space() {
        // .com ending should not get a space (ambiguous with domain)
        assert_eq!(
            ensure_trailing_sentence_space("example.com."),
            "example.com."
        );
    }

    #[test]
    fn normal_prose_with_period() {
        // Normal dictation: sentence with a period
        assert_eq!(
            ensure_trailing_sentence_space("The quick brown fox jumps over the lazy dog."),
            "The quick brown fox jumps over the lazy dog. "
        );
    }

    #[test]
    fn repeated_sentence_insertions_keep_boundaries() {
        let combined = format!(
            "{}{}{}",
            ensure_trailing_sentence_space("This is the testing bro one two three."),
            ensure_trailing_sentence_space("This is a testing blow one to three."),
            ensure_trailing_sentence_space("This is a testing row on to three."),
        );

        assert_eq!(
            combined,
            "This is the testing bro one two three. This is a testing blow one to three. This is a testing row on to three. "
        );
    }

    #[test]
    fn code_assignment_sentence_like_no_space() {
        assert_eq!(
            ensure_trailing_sentence_space("result = ok."),
            "result = ok."
        );
    }
}
