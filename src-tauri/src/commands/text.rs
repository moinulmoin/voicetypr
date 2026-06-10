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

/// Ensure prose ending in sentence punctuation has exactly one trailing space.
///
/// This is applied at the insertion boundary only, so stored transcription
/// history remains clean. Rules:
/// - `Hello world.` → `Hello world. `
/// - `Hello world. ` → `Hello world. ` (normalize to one)
/// - `Hello world` → `Hello world` (no sentence end, no space)
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

    let closing_punctuation = ['"', '\'', '“', '”', '’', ')', ']', '}'];
    let sentence_end = without_trailing_spaces
        .chars()
        .rev()
        .find(|c| !closing_punctuation.contains(c));

    // Only add space after sentence-ending punctuation, allowing closing
    // quotes/brackets after the punctuation.
    if !matches!(sentence_end, Some('.' | '!' | '?')) {
        return text.to_string();
    }

    // Detect contexts where a trailing space would be harmful.
    let semantic_end =
        without_trailing_spaces.trim_end_matches(|c| closing_punctuation.contains(&c));
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

    if looks_like_url || looks_like_email || looks_like_code {
        return text.to_string();
    }

    // Preserve exactly one trailing space after the original closing punctuation.
    format!("{} ", without_trailing_spaces)
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
/// Minimal clipboard seam so insertion sequencing is unit-testable.
trait ClipboardOps {
    fn get_text(&mut self) -> Result<String, String>;
    fn set_text(&mut self, text: &str) -> Result<(), String>;
}

impl ClipboardOps for Clipboard {
    fn get_text(&mut self) -> Result<String, String> {
        Clipboard::get_text(self).map_err(|e| e.to_string())
    }

    fn set_text(&mut self, text: &str) -> Result<(), String> {
        Clipboard::set_text(self, text).map_err(|e| e.to_string())
    }
}

/// What happened to the paste attempt — decides restore behavior.
#[derive(Debug, PartialEq)]
enum PasteOutcome {
    Pasted,
    LeftInClipboard,
    NoPermission,
}

const CLIPBOARD_RESTORE_DELAY: Duration = Duration::from_millis(500);

fn run_clipboard_insertion(
    clipboard: &mut dyn ClipboardOps,
    paste: &mut dyn FnMut() -> PasteOutcome,
    sleep: &mut dyn FnMut(Duration),
    text: &str,
    keep_transcription_in_clipboard: bool,
) -> Result<PasteOutcome, String> {
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

    clipboard
        .set_text(text)
        .map_err(|e| format!("Failed to set clipboard: {}", e))?;

    log::info!("Set clipboard content ({} chars)", text.chars().count());

    sleep(Duration::from_millis(50));

    if let Ok(clipboard_check) = clipboard.get_text() {
        log::info!(
            "Clipboard content verified ({} chars)",
            clipboard_check.chars().count()
        );
    }

    let outcome = paste();

    match outcome {
        PasteOutcome::NoPermission => {
            log::debug!(
                "Skipping clipboard restoration after paste failure; transcript remains available for manual paste"
            );
            Err("No accessibility permission - text copied to clipboard. Please paste manually or grant accessibility permission.".to_string())
        }
        PasteOutcome::LeftInClipboard => {
            log::debug!(
                "Skipping clipboard restoration after paste failure; transcript remains available for manual paste"
            );
            Ok(outcome)
        }
        PasteOutcome::Pasted => {
            if !keep_transcription_in_clipboard {
                if let Some(previous_text) = previous_clipboard_text {
                    sleep(CLIPBOARD_RESTORE_DELAY);
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
            }

            Ok(outcome)
        }
    }
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

    let mut paste = || {
        // Check if we have accessibility permissions before attempting to paste
        if !has_accessibility_permission {
            log::warn!(
                "No accessibility permission - text copied to clipboard but cannot paste automatically"
            );
            return PasteOutcome::NoPermission;
        }

        // Try to paste using Cmd+V (macOS) with panic protection
        // Add delay since pill was just hidden

        // First try with rdev, fallback to AppleScript if it fails
        let rdev_result = try_paste_with_rdev();

        match rdev_result {
            Ok(_) => {
                log::info!("Successfully pasted with rdev");
                PasteOutcome::Pasted
            }
            Err(e) => {
                log::warn!("rdev paste failed: {}, trying AppleScript fallback", e);

                // Fallback to AppleScript
                let paste_result =
                    panic::catch_unwind(AssertUnwindSafe(try_paste_with_applescript));

                match paste_result {
                    Ok(Ok(_)) => {
                        log::info!("Successfully pasted with AppleScript");
                        PasteOutcome::Pasted
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
                        PasteOutcome::LeftInClipboard
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
                        PasteOutcome::LeftInClipboard
                    }
                }
            }
        }
    };

    let mut sleep = thread::sleep;

    run_clipboard_insertion(
        &mut clipboard,
        &mut paste,
        &mut sleep,
        &text,
        keep_transcription_in_clipboard,
    )
    .map(|_| ())
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
mod clipboard_insertion {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct MockClipboard {
        current: String,
        history: Vec<String>,
        get_text_error: bool,
        events: Rc<RefCell<Vec<String>>>,
    }

    impl MockClipboard {
        fn new(current: &str, events: Rc<RefCell<Vec<String>>>) -> Self {
            Self {
                current: current.to_string(),
                history: Vec::new(),
                get_text_error: false,
                events,
            }
        }

        fn non_text(events: Rc<RefCell<Vec<String>>>) -> Self {
            Self {
                current: String::new(),
                history: Vec::new(),
                get_text_error: true,
                events,
            }
        }
    }

    impl ClipboardOps for MockClipboard {
        fn get_text(&mut self) -> Result<String, String> {
            self.events.borrow_mut().push("get".to_string());
            if self.get_text_error {
                Err("non-text clipboard".to_string())
            } else {
                Ok(self.current.clone())
            }
        }

        fn set_text(&mut self, text: &str) -> Result<(), String> {
            self.events.borrow_mut().push(format!("set:{text}"));
            self.current = text.to_string();
            self.history.push(self.current.clone());
            Ok(())
        }
    }

    fn sleep_recorder(
        events: Rc<RefCell<Vec<String>>>,
        sleeps: Rc<RefCell<Vec<Duration>>>,
    ) -> impl FnMut(Duration) {
        move |duration| {
            events
                .borrow_mut()
                .push(format!("sleep:{}", duration.as_millis()));
            sleeps.borrow_mut().push(duration);
        }
    }

    #[test]
    fn restores_after_delay_when_pasted() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let sleeps = Rc::new(RefCell::new(Vec::new()));
        let mut clipboard = MockClipboard::new("previous", events.clone());
        let mut paste = {
            let events = events.clone();
            move || {
                events.borrow_mut().push("paste".to_string());
                PasteOutcome::Pasted
            }
        };
        let mut sleep = sleep_recorder(events.clone(), sleeps.clone());

        let outcome = run_clipboard_insertion(
            &mut clipboard,
            &mut paste,
            &mut sleep,
            "transcription",
            false,
        )
        .unwrap();

        assert_eq!(outcome, PasteOutcome::Pasted);
        assert_eq!(clipboard.current, "previous");
        assert!(sleeps.borrow().contains(&CLIPBOARD_RESTORE_DELAY));

        let events = events.borrow();
        let paste_index = events.iter().position(|event| event == "paste").unwrap();
        let restore_delay_index = events
            .iter()
            .position(|event| event == "sleep:500")
            .unwrap();
        let restore_index = events
            .iter()
            .position(|event| event == "set:previous")
            .unwrap();
        assert!(paste_index < restore_delay_index);
        assert!(restore_delay_index < restore_index);
    }

    #[test]
    fn keeps_transcription_when_setting_enabled() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let sleeps = Rc::new(RefCell::new(Vec::new()));
        let mut clipboard = MockClipboard::new("previous", events.clone());
        let mut paste = || PasteOutcome::Pasted;
        let mut sleep = sleep_recorder(events, sleeps.clone());

        let outcome = run_clipboard_insertion(
            &mut clipboard,
            &mut paste,
            &mut sleep,
            "transcription",
            true,
        )
        .unwrap();

        assert_eq!(outcome, PasteOutcome::Pasted);
        assert_eq!(clipboard.current, "transcription");
        assert!(!sleeps.borrow().contains(&CLIPBOARD_RESTORE_DELAY));
    }

    #[test]
    fn no_restore_when_paste_left_in_clipboard() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let sleeps = Rc::new(RefCell::new(Vec::new()));
        let mut clipboard = MockClipboard::new("previous", events.clone());
        let mut paste = || PasteOutcome::LeftInClipboard;
        let mut sleep = sleep_recorder(events, sleeps.clone());

        let outcome = run_clipboard_insertion(
            &mut clipboard,
            &mut paste,
            &mut sleep,
            "transcription",
            false,
        )
        .unwrap();

        assert_eq!(outcome, PasteOutcome::LeftInClipboard);
        assert_eq!(clipboard.current, "transcription");
        assert!(!sleeps.borrow().contains(&CLIPBOARD_RESTORE_DELAY));
    }

    #[test]
    fn no_restore_without_permission() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let sleeps = Rc::new(RefCell::new(Vec::new()));
        let mut clipboard = MockClipboard::new("previous", events.clone());
        let mut paste = || PasteOutcome::NoPermission;
        let mut sleep = sleep_recorder(events, sleeps.clone());

        let result = run_clipboard_insertion(
            &mut clipboard,
            &mut paste,
            &mut sleep,
            "transcription",
            false,
        );

        assert_eq!(
            result.unwrap_err(),
            "No accessibility permission - text copied to clipboard. Please paste manually or grant accessibility permission."
        );
        assert_eq!(clipboard.current, "transcription");
        assert!(!sleeps.borrow().contains(&CLIPBOARD_RESTORE_DELAY));
    }

    #[test]
    fn no_previous_text_leaves_clipboard() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let sleeps = Rc::new(RefCell::new(Vec::new()));
        let mut clipboard = MockClipboard::non_text(events.clone());
        let mut paste = || PasteOutcome::Pasted;
        let mut sleep = sleep_recorder(events, sleeps.clone());

        let outcome = run_clipboard_insertion(
            &mut clipboard,
            &mut paste,
            &mut sleep,
            "transcription",
            false,
        )
        .unwrap();

        assert_eq!(outcome, PasteOutcome::Pasted);
        assert_eq!(clipboard.current, "transcription");
        assert!(!sleeps.borrow().contains(&CLIPBOARD_RESTORE_DELAY));
        assert_eq!(clipboard.history, vec!["transcription"]);
    }
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
    fn no_sentence_end_no_space() {
        assert_eq!(ensure_trailing_sentence_space("Hello world"), "Hello world");
    }

    #[test]
    fn comma_no_space() {
        assert_eq!(
            ensure_trailing_sentence_space("Hello world,"),
            "Hello world,"
        );
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
