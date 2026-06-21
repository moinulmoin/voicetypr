use arboard::Clipboard;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
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
#[derive(Debug, PartialEq, Eq)]
enum PasteOutcome {
    Pasted,
    LeftInClipboard,
    NoPermission,
}

const CLIPBOARD_RESTORE_DELAY: Duration = Duration::from_millis(500);

// Platform-specific clipboard settle delay (after set_text, before paste).
#[cfg(target_os = "macos")]
const CLIPBOARD_SETTLE_DELAY: Duration = Duration::from_millis(15);
#[cfg(target_os = "windows")]
const CLIPBOARD_SETTLE_DELAY: Duration = Duration::from_millis(25);
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const CLIPBOARD_SETTLE_DELAY: Duration = Duration::from_millis(50);

// Platform-specific pre-paste delay in try_paste_with_rdev (0 = skip entirely).
#[cfg(target_os = "macos")]
const RDEV_PRE_PASTE_DELAY: Duration = Duration::from_millis(0);
#[cfg(target_os = "windows")]
const RDEV_PRE_PASTE_DELAY: Duration = Duration::from_millis(20);
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const RDEV_PRE_PASTE_DELAY: Duration = Duration::from_millis(50);

/// In-flight clipboard restoration state, guarded by [`CLIPBOARD_GUARD`].
///
/// `transcript` is what we last placed on the clipboard; `original` is the
/// user's clipboard content to restore once the transcript is no longer needed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingClipboardRestore {
    /// Monotonic id of the insertion that scheduled this restore; the restore
    /// only acts on its own generation, so a newer insertion cancels it.
    generation: u64,
    transcript: String,
    original: String,
}

/// Serializes the insertion (capture -> set -> paste -> record) sequence against
/// the deferred restore, and carries the pending restore between them. A single
/// global lock is correct here because `IS_INSERTING` already prevents
/// concurrent insertions; only the background restore contends.
static CLIPBOARD_GUARD: Mutex<Option<PendingClipboardRestore>> = Mutex::new(None);

/// Monotonic insertion id. Each insertion takes the next value so its deferred
/// restore can detect when a newer insertion has superseded it.
static CLIPBOARD_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Decide which ORIGINAL clipboard text an insertion must eventually restore.
///
/// If the clipboard currently holds a transcript from a prior, not-yet-restored
/// dictation, carry that dictation's original forward (so back-to-back
/// dictations restore the user's real clipboard, never an intermediate
/// transcript). Returns `None` when there is nothing safe to restore (a
/// non-text or unreadable clipboard).
fn resolve_original_to_restore(
    pending: &Option<PendingClipboardRestore>,
    current_clipboard: Option<&str>,
) -> Option<String> {
    let current = current_clipboard?;
    match pending {
        Some(p) if p.transcript == current => Some(p.original.clone()),
        _ => Some(current.to_string()),
    }
}

/// What a deferred restore timer should do with the pending state.
#[derive(Debug, PartialEq, Eq)]
enum DeferredRestore {
    /// Clipboard still holds our transcript: restore this original, then clear.
    Restore(String),
    /// Our entry, but the clipboard changed (or is non-text): clear it, no restore.
    ClearPending,
    /// Not our entry (a newer insertion owns it) or nothing pending: leave it.
    Skip,
}

/// Decide what the deferred restore timer for `generation` should do. A timer
/// always resolves its OWN generation's entry (restore-then-clear, or
/// clear-only), so a skipped restore never leaves a stale entry that a later
/// identical clipboard value could misclassify.
fn deferred_restore_action(
    pending: &Option<PendingClipboardRestore>,
    current_clipboard: Option<&str>,
    generation: u64,
) -> DeferredRestore {
    let Some(pending) = pending.as_ref() else {
        return DeferredRestore::Skip;
    };
    // A newer insertion owns the pending entry; this stale timer must not touch it.
    if pending.generation != generation {
        return DeferredRestore::Skip;
    }
    match current_clipboard {
        Some(current) if current == pending.transcript => {
            DeferredRestore::Restore(pending.original.clone())
        }
        _ => DeferredRestore::ClearPending,
    }
}

/// Spawn the background, lock-serialized clipboard restore. After the settle
/// delay it restores the user's original clipboard iff our transcript is still
/// present, then clears the pending state. Fails closed on any clipboard error,
/// so it never clobbers a user copy or a newer dictation's transcript.
fn spawn_deferred_clipboard_restore(generation: u64) {
    if let Err(e) = thread::Builder::new()
        .name("clipboard-restore".into())
        .spawn(move || {
            thread::sleep(CLIPBOARD_RESTORE_DELAY);

            let mut pending = CLIPBOARD_GUARD
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());

            // If the clipboard cannot be opened or read, treat current as unknown
            // (None). For our own entry that maps to `ClearPending` below, so we
            // still clear it and never leave a stale entry behind.
            let mut clipboard = match Clipboard::new() {
                Ok(c) => Some(c),
                Err(e) => {
                    log::debug!("clipboard-restore: failed to open clipboard: {}", e);
                    None
                }
            };
            let current = clipboard.as_mut().and_then(|c| c.get_text().ok());

            match deferred_restore_action(&pending, current.as_deref(), generation) {
                DeferredRestore::Restore(original) => {
                    if let Some(clipboard) = clipboard.as_mut() {
                        match clipboard.set_text(&original) {
                            Ok(()) => {
                                log::debug!("clipboard-restore: restored original clipboard text")
                            }
                            Err(e) => log::debug!("clipboard-restore: set_text error: {}", e),
                        }
                    }
                    // Resolve our own entry either way so it never lingers.
                    *pending = None;
                }
                DeferredRestore::ClearPending => {
                    *pending = None;
                    log::debug!(
                        "clipboard-restore: clipboard unavailable or changed, cleared stale pending"
                    )
                }
                DeferredRestore::Skip => {
                    log::debug!("clipboard-restore: superseded by a newer insertion, skipping")
                }
            }
        })
    {
        log::debug!("Failed to spawn clipboard-restore thread: {}", e);
    }
}

/// Set the clipboard to `text`, let it settle, then paste, returning the paste
/// outcome. Capturing the previous clipboard and scheduling a restore is the
/// caller's responsibility, so it can serialize that against deferred restores.
fn run_clipboard_insertion(
    clipboard: &mut dyn ClipboardOps,
    paste: &mut dyn FnMut() -> PasteOutcome,
    sleep: &mut dyn FnMut(Duration),
    text: &str,
) -> Result<PasteOutcome, String> {
    clipboard
        .set_text(text)
        .map_err(|e| format!("Failed to set clipboard: {}", e))?;

    log::info!("Set clipboard content ({} chars)", text.chars().count());

    sleep(CLIPBOARD_SETTLE_DELAY);

    if let Ok(clipboard_check) = clipboard.get_text() {
        log::info!(
            "Clipboard content verified ({} chars)",
            clipboard_check.chars().count()
        );
    }

    Ok(paste())
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
                            crate::commands::audio::pill_toast_with_suggestion(
                                app,
                                "Text copied",
                                "Grant Accessibility permission to enable auto-paste",
                                1500,
                                None,
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
                            crate::commands::audio::pill_toast_with_suggestion(
                                app,
                                "Text copied",
                                "Grant Accessibility permission to enable auto-paste",
                                1500,
                                None,
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

    // Serialize capture -> set -> paste -> record against the deferred restore so
    // a stale restore can never clobber this (or a newer) insertion's clipboard.
    let mut guard = CLIPBOARD_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());

    // This insertion's generation; the deferred restore only acts on its own
    // generation, so a newer insertion (which bumps this) cancels older timers.
    let generation = CLIPBOARD_GENERATION
        .fetch_add(1, Ordering::SeqCst)
        .wrapping_add(1);

    // Decide which ORIGINAL we must eventually restore, carrying it forward when
    // the clipboard still holds a transcript from a prior, not-yet-restored
    // dictation (so rapid dictations restore the user's real clipboard).
    let original_to_restore = if keep_transcription_in_clipboard {
        None
    } else {
        let current = clipboard.get_text().ok();
        resolve_original_to_restore(&guard, current.as_deref())
    };

    let outcome = run_clipboard_insertion(&mut clipboard, &mut paste, &mut sleep, &text)?;

    log::debug!("clipboard insertion outcome: {:?}", outcome);

    // Every outcome supersedes any prior pending restore: we either record our
    // own (Pasted) or clear it (no-restore outcomes), so a stale restore from a
    // previous dictation can never fire over what we just placed or left.
    match outcome {
        PasteOutcome::Pasted => {
            match original_to_restore {
                Some(original) => {
                    *guard = Some(PendingClipboardRestore {
                        generation,
                        transcript: text.clone(),
                        original,
                    });
                    drop(guard);
                    spawn_deferred_clipboard_restore(generation);
                }
                None => *guard = None,
            }
            Ok(())
        }
        // Paste failed but the transcript stays on the clipboard for manual
        // paste; schedule no restore AND invalidate any stale prior restore so
        // it cannot remove the transcript we just left.
        PasteOutcome::LeftInClipboard => {
            *guard = None;
            Ok(())
        }
        PasteOutcome::NoPermission => {
            *guard = None;
            Err(
                "No accessibility permission - text copied to clipboard. Please paste manually or grant accessibility permission."
                    .to_string(),
            )
        }
    }
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

    // Pre-paste delay; 0 on macOS (skip entirely), conservative on Windows/Linux.
    if !RDEV_PRE_PASTE_DELAY.is_zero() {
        thread::sleep(RDEV_PRE_PASTE_DELAY);
    }

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
    thread::sleep(Duration::from_millis(15));

    // Try paste with retry logic
    for attempt in 1..=2 {
        log::debug!("macOS paste attempt {}/2", attempt);

        let result = (|| {
            // Press Cmd (Meta) key first and hold it
            log::debug!("Pressing MetaLeft (Cmd)");
            simulate(&EventType::KeyPress(RdevKey::MetaLeft))?;
            thread::sleep(Duration::from_millis(15)); // Give OS time to register modifier

            // While Cmd is held, press V
            log::debug!("Pressing KeyV while Cmd is held");
            simulate(&EventType::KeyPress(RdevKey::KeyV))?;
            thread::sleep(Duration::from_millis(15));

            // Release V first
            log::debug!("Releasing KeyV");
            simulate(&EventType::KeyRelease(RdevKey::KeyV))?;
            thread::sleep(Duration::from_millis(15));

            // Then release Cmd
            log::debug!("Releasing MetaLeft (Cmd)");
            simulate(&EventType::KeyRelease(RdevKey::MetaLeft))?;
            thread::sleep(Duration::from_millis(15));

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
        events: Rc<RefCell<Vec<String>>>,
    }

    impl MockClipboard {
        fn new(current: &str, events: Rc<RefCell<Vec<String>>>) -> Self {
            Self {
                current: current.to_string(),
                history: Vec::new(),
                events,
            }
        }
    }

    impl ClipboardOps for MockClipboard {
        fn get_text(&mut self) -> Result<String, String> {
            self.events.borrow_mut().push("get".to_string());
            Ok(self.current.clone())
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

    // --- run_clipboard_insertion tests (set -> settle -> paste; no restore here) ---

    #[test]
    fn run_insertion_sets_settles_and_returns_pasted() {
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

        let outcome =
            run_clipboard_insertion(&mut clipboard, &mut paste, &mut sleep, "transcription")
                .unwrap();

        assert_eq!(outcome, PasteOutcome::Pasted);
        assert_eq!(clipboard.current, "transcription");
        assert_eq!(clipboard.history, vec!["transcription"]);
        // Settles once, never blocks on the 500 ms restore (that is deferred).
        assert!(sleeps.borrow().contains(&CLIPBOARD_SETTLE_DELAY));
        assert!(!sleeps.borrow().contains(&CLIPBOARD_RESTORE_DELAY));
        // Ordering: set before settle before paste.
        let events = events.borrow();
        let set_idx = events
            .iter()
            .position(|e| e == "set:transcription")
            .unwrap();
        let paste_idx = events.iter().position(|e| e == "paste").unwrap();
        assert!(set_idx < paste_idx);
    }

    #[test]
    fn run_insertion_returns_left_in_clipboard() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let sleeps = Rc::new(RefCell::new(Vec::new()));
        let mut clipboard = MockClipboard::new("previous", events.clone());
        let mut paste = || PasteOutcome::LeftInClipboard;
        let mut sleep = sleep_recorder(events, sleeps);

        let outcome =
            run_clipboard_insertion(&mut clipboard, &mut paste, &mut sleep, "transcription")
                .unwrap();

        assert_eq!(outcome, PasteOutcome::LeftInClipboard);
        assert_eq!(clipboard.current, "transcription");
    }

    #[test]
    fn run_insertion_returns_no_permission() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let sleeps = Rc::new(RefCell::new(Vec::new()));
        let mut clipboard = MockClipboard::new("previous", events.clone());
        let mut paste = || PasteOutcome::NoPermission;
        let mut sleep = sleep_recorder(events, sleeps);

        // The Err mapping now lives in the caller; the core just reports outcome.
        let outcome =
            run_clipboard_insertion(&mut clipboard, &mut paste, &mut sleep, "transcription")
                .unwrap();

        assert_eq!(outcome, PasteOutcome::NoPermission);
        assert_eq!(clipboard.current, "transcription");
    }

    // --- resolve_original_to_restore (which ORIGINAL to eventually restore) ---

    #[test]
    fn resolve_uses_current_clipboard_when_nothing_pending() {
        assert_eq!(
            resolve_original_to_restore(&None, Some("user content")),
            Some("user content".to_string())
        );
    }

    #[test]
    fn resolve_carries_original_when_clipboard_holds_prior_transcript() {
        let pending = Some(PendingClipboardRestore {
            generation: 1,
            transcript: "prior transcript".into(),
            original: "real original".into(),
        });
        // Clipboard still holds our prior transcript → carry the real original.
        assert_eq!(
            resolve_original_to_restore(&pending, Some("prior transcript")),
            Some("real original".to_string())
        );
    }

    #[test]
    fn resolve_uses_user_copy_when_clipboard_changed() {
        let pending = Some(PendingClipboardRestore {
            generation: 1,
            transcript: "prior transcript".into(),
            original: "real original".into(),
        });
        // User copied something since our paste → that becomes the original.
        assert_eq!(
            resolve_original_to_restore(&pending, Some("user copy")),
            Some("user copy".to_string())
        );
    }

    #[test]
    fn resolve_skips_for_non_text_clipboard() {
        let pending = Some(PendingClipboardRestore {
            generation: 1,
            transcript: "prior".into(),
            original: "orig".into(),
        });
        assert_eq!(resolve_original_to_restore(&pending, None), None);
    }

    // --- deferred_restore_action (what the background restore does) ---

    #[test]
    fn deferred_restores_when_transcript_still_present() {
        let pending = Some(PendingClipboardRestore {
            generation: 7,
            transcript: "transcript".into(),
            original: "original".into(),
        });
        assert_eq!(
            deferred_restore_action(&pending, Some("transcript"), 7),
            DeferredRestore::Restore("original".to_string())
        );
    }

    #[test]
    fn deferred_clears_own_entry_when_user_copied_something_else() {
        let pending = Some(PendingClipboardRestore {
            generation: 7,
            transcript: "transcript".into(),
            original: "original".into(),
        });
        // Our entry, but the clipboard changed -> clear it (no restore, no stale entry).
        assert_eq!(
            deferred_restore_action(&pending, Some("user copy"), 7),
            DeferredRestore::ClearPending
        );
    }

    #[test]
    fn deferred_skips_when_nothing_pending() {
        assert_eq!(
            deferred_restore_action(&None, Some("transcript"), 7),
            DeferredRestore::Skip
        );
    }

    #[test]
    fn deferred_clears_own_entry_for_non_text_clipboard() {
        let pending = Some(PendingClipboardRestore {
            generation: 7,
            transcript: "transcript".into(),
            original: "original".into(),
        });
        assert_eq!(
            deferred_restore_action(&pending, None, 7),
            DeferredRestore::ClearPending
        );
    }

    /// Blocker 1 guard: a stale timer must not act once a newer insertion took
    /// over, even if the clipboard still holds (an identical) transcript.
    #[test]
    fn deferred_skips_when_generation_superseded() {
        let pending = Some(PendingClipboardRestore {
            generation: 8, // newer insertion now owns the pending state
            transcript: "transcript".into(),
            original: "original".into(),
        });
        // This timer was scheduled for generation 7 — it must skip and not touch it.
        assert_eq!(
            deferred_restore_action(&pending, Some("transcript"), 7),
            DeferredRestore::Skip
        );
    }

    // --- rapid-dictation carry-forward regression guards (review blocker 2) ---

    #[test]
    fn rapid_dictation_preserves_user_original_not_intermediate_transcript() {
        // Dictation A: clipboard holds the user's ORIGINAL.
        let original_a = resolve_original_to_restore(&None, Some("ORIGINAL")).unwrap();
        let pending_a = PendingClipboardRestore {
            generation: 1,
            transcript: "A_text".into(),
            original: original_a,
        };
        // Dictation B starts before A's restore fires; clipboard now holds A_text.
        let original_b = resolve_original_to_restore(&Some(pending_a), Some("A_text")).unwrap();
        let pending_b = PendingClipboardRestore {
            generation: 2,
            transcript: "B_text".into(),
            original: original_b,
        };
        // B's deferred restore must restore ORIGINAL, never the intermediate A_text.
        assert_eq!(
            deferred_restore_action(&Some(pending_b), Some("B_text"), 2),
            DeferredRestore::Restore("ORIGINAL".to_string())
        );
    }

    #[test]
    fn rapid_identical_text_dictation_preserves_user_original() {
        // Same transcript twice — the case the epoch-only guard got wrong.
        let original_a = resolve_original_to_restore(&None, Some("ORIGINAL")).unwrap();
        let pending_a = PendingClipboardRestore {
            generation: 1,
            transcript: "SAME".into(),
            original: original_a,
        };
        // Clipboard holds "SAME" (from A) when B captures.
        let original_b = resolve_original_to_restore(&Some(pending_a), Some("SAME")).unwrap();
        assert_eq!(original_b, "ORIGINAL");
        let pending_b = PendingClipboardRestore {
            generation: 2,
            transcript: "SAME".into(),
            original: original_b,
        };
        assert_eq!(
            deferred_restore_action(&Some(pending_b), Some("SAME"), 2),
            DeferredRestore::Restore("ORIGINAL".to_string())
        );
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
