use crate::commands::audio::{start_recording, stop_recording, RecorderState};
use crate::recording::escape_handler::handle_escape_key_press;
use crate::{get_recording_state, update_recording_state, AppState, RecordingMode, RecordingState};
use std::sync::atomic::Ordering;
use tauri::Manager;
use tauri_plugin_global_shortcut::{Shortcut, ShortcutState};

/// Handle global shortcut events for recording
///
/// This is the main entry point for all global shortcut handling.
/// It determines the recording mode and dispatches to the appropriate handler.
pub fn handle_global_shortcut(
    app: &tauri::AppHandle,
    shortcut: &Shortcut,
    event_state: ShortcutState,
) {
    log::debug!(
        "Global shortcut triggered: {:?} - State: {:?}",
        shortcut,
        event_state
    );

    let Some(app_state) = app.try_state::<AppState>() else {
        log::warn!("Global shortcut triggered before AppState initialized");
        return;
    };

    let recording_mode = {
        if let Ok(mode_guard) = app_state.recording_mode.lock() {
            *mode_guard
        } else {
            RecordingMode::Toggle
        }
    };

    let is_recording_shortcut = {
        if let Ok(shortcut_guard) = app_state.recording_shortcut.lock() {
            if let Some(ref recording_shortcut) = *shortcut_guard {
                shortcut == recording_shortcut
            } else {
                false
            }
        } else {
            false
        }
    };

    let is_ptt_shortcut = {
        if let Ok(ptt_guard) = app_state.ptt_shortcut.lock() {
            if let Some(ref ptt_shortcut) = *ptt_guard {
                shortcut == ptt_shortcut
            } else {
                false
            }
        } else {
            false
        }
    };

    let should_handle = match recording_mode {
        RecordingMode::Toggle => is_recording_shortcut && event_state == ShortcutState::Pressed,
        RecordingMode::PushToTalk => is_recording_shortcut || is_ptt_shortcut,
    };

    if should_handle {
        let current_state = get_recording_state(app);
        handle_recording_shortcut(app, &app_state, recording_mode, current_state, event_state);
    } else if !is_recording_shortcut && !is_ptt_shortcut {
        handle_non_recording_shortcut(app, shortcut, event_state);
    }
}

/// Handle recording-related shortcuts (toggle or PTT)
fn handle_recording_shortcut(
    app: &tauri::AppHandle,
    app_state: &AppState,
    recording_mode: RecordingMode,
    current_state: RecordingState,
    event_state: ShortcutState,
) {
    match recording_mode {
        RecordingMode::Toggle => {
            handle_toggle_mode(app, app_state, current_state, event_state);
        }
        RecordingMode::PushToTalk => {
            handle_ptt_mode(app, app_state, current_state, event_state);
        }
    }
}

/// Handle toggle mode recording (click to start/stop)
fn handle_toggle_mode(
    app: &tauri::AppHandle,
    app_state: &AppState,
    current_state: RecordingState,
    event_state: ShortcutState,
) {
    if event_state != ShortcutState::Pressed {
        return;
    }

    let should_throttle = {
        let now = std::time::Instant::now();
        match app_state.last_toggle_press.lock() {
            Ok(mut last_press) => {
                if let Some(last) = *last_press {
                    if now.duration_since(last).as_millis() < 300 {
                        log::debug!("Toggle: Throttling hotkey press (too fast)");
                        true
                    } else {
                        *last_press = Some(now);
                        false
                    }
                } else {
                    *last_press = Some(now);
                    false
                }
            }
            Err(e) => {
                log::error!("Failed to lock last_toggle_press: {}", e);
                false
            }
        }
    };

    if should_throttle {
        crate::commands::audio::pill_toast(app, "Hold on...", 1000);
        return;
    }

    match current_state {
        RecordingState::Idle | RecordingState::Error => {
            log::info!("Toggle: Starting recording via hotkey");
            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let recorder_state = app_handle.state::<RecorderState>();
                match start_recording(app_handle.clone(), recorder_state).await {
                    Ok(_) => log::info!("Toggle: Recording started successfully"),
                    Err(e) => {
                        log::error!("Toggle: Error starting recording: {}", e);
                        update_recording_state(&app_handle, RecordingState::Error, Some(e));
                    }
                }
            });
        }
        RecordingState::Starting => {
            log::info!("Toggle: stop requested while starting; will stop after start completes");
            app_state
                .pending_stop_after_start
                .store(true, Ordering::SeqCst);
        }
        RecordingState::Recording => {
            log::info!("Toggle: Stopping recording via hotkey");
            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let recorder_state = app_handle.state::<RecorderState>();
                match stop_recording(app_handle.clone(), recorder_state).await {
                    Ok(_) => log::info!("Toggle: Recording stopped successfully"),
                    Err(e) => log::error!("Toggle: Error stopping recording: {}", e),
                }
            });
        }
        _ => log::debug!("Toggle: Ignoring hotkey in state {:?}", current_state),
    }
}

/// Handle push-to-talk mode recording (hold to record, release to stop)
fn handle_ptt_mode(
    app: &tauri::AppHandle,
    app_state: &AppState,
    current_state: RecordingState,
    event_state: ShortcutState,
) {
    match event_state {
        ShortcutState::Pressed => {
            log::info!("PTT: Key pressed");
            app_state.ptt_key_held.store(true, Ordering::Relaxed);

            if matches!(current_state, RecordingState::Idle | RecordingState::Error) {
                log::info!("PTT: Starting recording");
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let recorder_state = app_handle.state::<RecorderState>();
                    match start_recording(app_handle.clone(), recorder_state).await {
                        Ok(_) => log::info!("PTT: Recording started successfully"),
                        Err(e) => {
                            log::error!("PTT: Error starting recording: {}", e);
                            update_recording_state(&app_handle, RecordingState::Error, Some(e));
                        }
                    }
                });
            }
        }
        ShortcutState::Released => {
            log::info!("PTT: Key released");
            app_state.ptt_key_held.store(false, Ordering::Relaxed);

            if matches!(
                current_state,
                RecordingState::Recording | RecordingState::Starting
            ) {
                log::info!("PTT: Stopping recording");
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let recorder_state = app_handle.state::<RecorderState>();
                    match stop_recording(app_handle.clone(), recorder_state).await {
                        Ok(_) => log::info!("PTT: Recording stopped successfully"),
                        Err(e) => log::error!("PTT: Error stopping recording: {}", e),
                    }
                });
            }
        }
    }
}

/// Handle non-recording shortcuts (e.g., ESC key)
fn handle_non_recording_shortcut(
    app: &tauri::AppHandle,
    shortcut: &Shortcut,
    event_state: ShortcutState,
) {
    log::debug!("Non-recording shortcut triggered: {:?}", shortcut);

    let escape_shortcut: Shortcut = match "Escape".parse() {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to parse Escape shortcut: {:?}", e);
            return;
        }
    };

    log::debug!(
        "Comparing shortcuts - received: {:?}, escape: {:?}",
        shortcut,
        escape_shortcut
    );

    if shortcut == &escape_shortcut {
        log::info!("ESC key detected in global handler");

        let app_handle = app.clone();

        tauri::async_runtime::spawn(async move {
            let app_state = app_handle.state::<AppState>();
            handle_escape_key_press(&app_state, &app_handle, event_state).await;
        });
    }
}
