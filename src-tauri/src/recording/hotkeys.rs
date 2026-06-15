use crate::commands::audio::{
    start_recording, stop_recording, RecorderState, PTT_START_ABORTED_AFTER_RELEASE,
};
use crate::commands::shortcuts::{
    self, hold_shortcut_transition, pressed_shortcut_should_run, CustomHoldTransition,
    RegisteredShortcutBinding, ShortcutAction, ShortcutTrigger,
};
use crate::recording::escape_handler::handle_escape_key_press;
use crate::{get_recording_state, update_recording_state, AppState, RecordingMode, RecordingState};
use std::sync::atomic::{AtomicBool, Ordering};
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

    if let Some(binding) = shortcuts::matching_custom_binding(&app_state, shortcut) {
        dispatch_custom_shortcut(app, &app_state, binding, event_state);
        return;
    }

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

    let should_handle =
        should_handle_recording_shortcut(recording_mode, is_recording_shortcut, is_ptt_shortcut);

    if should_handle {
        let current_state = get_recording_state(app);
        match recording_mode {
            RecordingMode::Toggle => {
                handle_toggle_mode(app, &app_state, current_state, event_state);
            }
            RecordingMode::PushToTalk => {
                let source_id = if is_ptt_shortcut {
                    "__builtin_ptt_shortcut"
                } else {
                    "__builtin_recording_shortcut"
                };
                handle_hold_to_record_source(app, &app_state, source_id, event_state);
            }
        }
    } else if !is_recording_shortcut && !is_ptt_shortcut {
        handle_non_recording_shortcut(app, shortcut, event_state);
    }
}

fn handle_hold_to_record_source(
    app: &tauri::AppHandle,
    app_state: &AppState,
    source_id: &str,
    event_state: ShortcutState,
) {
    let transition = match app_state.active_custom_hold_bindings.lock() {
        Ok(mut active_bindings) => {
            hold_shortcut_transition(&mut active_bindings, source_id, event_state)
        }
        Err(error) => {
            log::error!("Failed to lock active hold bindings: {}", error);
            CustomHoldTransition::Noop
        }
    };

    match transition {
        CustomHoldTransition::Start => {
            let current_state = get_recording_state(app);
            handle_ptt_mode(app, app_state, current_state, ShortcutState::Pressed);
        }
        CustomHoldTransition::Stop => {
            let current_state = get_recording_state(app);
            handle_ptt_mode(app, app_state, current_state, ShortcutState::Released);
        }
        CustomHoldTransition::Noop => {}
    }
}

fn should_handle_recording_shortcut(
    recording_mode: RecordingMode,
    is_recording_shortcut: bool,
    is_ptt_shortcut: bool,
) -> bool {
    match recording_mode {
        // Toggle mode needs both Pressed and Released events so repeated
        // Windows key-repeat Pressed events map to one physical hold.
        RecordingMode::Toggle => is_recording_shortcut,
        RecordingMode::PushToTalk => is_recording_shortcut || is_ptt_shortcut,
    }
}

/// Handle toggle mode recording (click to start/stop)
fn handle_toggle_mode(
    app: &tauri::AppHandle,
    app_state: &AppState,
    current_state: RecordingState,
    event_state: ShortcutState,
) {
    match event_state {
        ShortcutState::Released => {
            app_state.toggle_key_held.store(false, Ordering::SeqCst);
            return;
        }
        ShortcutState::Pressed => {}
    }

    if !claim_toggle_press(&app_state.toggle_key_held) {
        log::debug!("Toggle: Ignoring duplicate key press while hotkey is held");
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

fn claim_toggle_press(toggle_key_held: &AtomicBool) -> bool {
    !toggle_key_held.swap(true, Ordering::SeqCst)
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
                        Err(e) if e == PTT_START_ABORTED_AFTER_RELEASE => {
                            log::info!("PTT: Recording start cancelled after key release");
                            update_recording_state(&app_handle, RecordingState::Idle, None);
                        }
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

            // Debounce: swap returns previous value, skip if already false (duplicate release)
            // This prevents race conditions when multiple release events fire rapidly
            if !app_state.ptt_key_held.swap(false, Ordering::SeqCst) {
                log::debug!("PTT: Ignoring duplicate key release");
                return;
            }

            match current_state {
                RecordingState::Recording => {
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
                RecordingState::Starting => {
                    // Key released while recording is still starting up.
                    // Set the pending flag so start_recording() can honor the stop
                    // as soon as it reaches the Recording state. This prevents
                    // recording from continuing after the user released PTT.
                    log::info!(
                        "PTT: Key released while Starting; setting pending_stop_after_start"
                    );
                    app_state
                        .pending_stop_after_start
                        .store(true, Ordering::SeqCst);
                }
                _ => {
                    log::debug!("PTT: Key released in state {:?}; no action", current_state);
                }
            }
        }
    }
}

fn dispatch_custom_shortcut(
    app: &tauri::AppHandle,
    app_state: &AppState,
    binding: RegisteredShortcutBinding,
    event_state: ShortcutState,
) {
    if binding.trigger == ShortcutTrigger::Pressed {
        let should_run = match app_state.active_custom_pressed_bindings.lock() {
            Ok(mut active_bindings) => {
                pressed_shortcut_should_run(&mut active_bindings, &binding.id, event_state)
            }
            Err(error) => {
                log::error!("Failed to lock active custom pressed bindings: {}", error);
                false
            }
        };

        if !should_run {
            return;
        }
    } else if binding.action != ShortcutAction::HoldToRecord {
        return;
    }

    match binding.action {
        ShortcutAction::ToggleRecording => {
            let current_state = get_recording_state(app);
            handle_toggle_mode(app, app_state, current_state, event_state);
        }
        ShortcutAction::HoldToRecord => {
            if binding.trigger != ShortcutTrigger::Hold {
                return;
            }
            handle_hold_to_record_source(app, app_state, &binding.id, event_state);
        }
        ShortcutAction::CancelRecording => {
            if event_state == ShortcutState::Pressed {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = crate::commands::audio::cancel_recording(app_handle).await {
                        log::error!("Shortcut cancel_recording failed: {}", error);
                    }
                });
            }
        }
        ShortcutAction::CopyLastTranscription => {
            if event_state == ShortcutState::Pressed {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    match shortcuts::latest_copyable_transcription_text(&app_handle).await {
                        Ok(Some(text)) => {
                            if let Err(error) =
                                crate::commands::text::copy_text_to_clipboard(text).await
                            {
                                log::error!("Shortcut copy_last_transcription failed: {}", error);
                            }
                        }
                        Ok(None) => log::debug!(
                            "Shortcut copy_last_transcription found no copyable history item"
                        ),
                        Err(error) => {
                            log::error!("Shortcut copy_last_transcription failed: {}", error)
                        }
                    }
                });
            }
        }
        ShortcutAction::PasteLastTranscription => {
            if event_state == ShortcutState::Pressed {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    match shortcuts::latest_copyable_transcription_text(&app_handle).await {
                        Ok(Some(text)) => {
                            if let Err(error) =
                                crate::commands::text::insert_text(app_handle.clone(), text).await
                            {
                                log::error!("Shortcut paste_last_transcription failed: {}", error);
                            }
                        }
                        Ok(None) => log::debug!(
                            "Shortcut paste_last_transcription found no copyable history item"
                        ),
                        Err(error) => {
                            log::error!("Shortcut paste_last_transcription failed: {}", error)
                        }
                    }
                });
            }
        }
        ShortcutAction::CycleFormattingMode => {
            if event_state == ShortcutState::Pressed {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = shortcuts::cycle_formatting_preset(app_handle).await {
                        log::error!("Shortcut cycle_formatting_mode failed: {}", error);
                    }
                });
            }
        }
        ShortcutAction::SetPersonalDictation
        | ShortcutAction::SetCleanDictation
        | ShortcutAction::SetWriting
        | ShortcutAction::SetNotes
        | ShortcutAction::SetMessage
        | ShortcutAction::SetCode => {
            if event_state == ShortcutState::Pressed {
                if let Some(preset) = shortcuts::action_preset(binding.action) {
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(error) =
                            shortcuts::set_formatting_preset(app_handle, preset).await
                        {
                            log::error!("Shortcut set formatting mode failed: {}", error);
                        }
                    });
                }
            }
        }
        ShortcutAction::OpenDashboard => {
            if event_state == ShortcutState::Pressed {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = crate::commands::window::focus_main_window(app_handle).await
                    {
                        log::error!("Shortcut open_dashboard failed: {}", error);
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

#[cfg(test)]
mod tests {
    use super::{claim_toggle_press, should_handle_recording_shortcut};
    use crate::RecordingMode;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn toggle_shortcut_events_are_handled_for_press_and_release() {
        assert!(should_handle_recording_shortcut(
            RecordingMode::Toggle,
            true,
            false
        ));
        assert!(!should_handle_recording_shortcut(
            RecordingMode::Toggle,
            false,
            false
        ));
    }

    #[test]
    fn claim_toggle_press_blocks_repeats_until_release() {
        let held = AtomicBool::new(false);

        assert!(claim_toggle_press(&held));
        assert!(!claim_toggle_press(&held));

        held.store(false, Ordering::SeqCst);
        assert!(claim_toggle_press(&held));
    }
}
