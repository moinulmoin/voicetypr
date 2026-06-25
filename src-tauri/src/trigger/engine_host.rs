//! Engine lifecycle + binding application for the host app.

use std::collections::HashSet;

use keytrigger::KeyPhase;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

use crate::commands::shortcuts::{ShortcutAction, ShortcutBinding, ShortcutTrigger, TriggerKind};
use crate::state::app_state::AppState;
use crate::{RecordingMode, RecordingState};

use super::{mapping, EngineBinding};

/// Recompute the engine's binding set from the full binding list and apply it.
///
/// Ordering matters for the core recording path: a removed binding that is
/// mid-hold (e.g. HoldToRecord) must be Released BEFORE we swap the lookup map,
/// otherwise the matcher's own synthetic Released would miss the (now absent)
/// id and leave recording stuck on. `dispatch_action(Released)` is idempotent,
/// so releasing every removed binding unconditionally is safe.
pub fn apply_engine_bindings(app: &AppHandle, bindings: &[ShortcutBinding]) {
    let app_state = app.state::<AppState>();

    let mut new_bindings: Vec<EngineBinding> = Vec::new();
    let mut triggers = Vec::new();
    for binding in bindings
        .iter()
        .filter(|b| b.enabled && mapping::is_engine_kind(b))
    {
        if let Some(trigger) = mapping::to_trigger(binding) {
            new_bindings.push(EngineBinding {
                id: binding.id.clone(),
                action: binding.action,
                trigger: binding.trigger,
            });
            triggers.push((binding.id.clone(), trigger));
        }
    }

    let new_ids: HashSet<String> = new_bindings.iter().map(|b| b.id.clone()).collect();
    let removed: Vec<EngineBinding> = match app_state.engine_bindings.lock() {
        Ok(guard) => guard
            .iter()
            .filter(|b| !new_ids.contains(&b.id))
            .cloned()
            .collect(),
        Err(error) => {
            log::error!("keytrigger: engine_bindings lock poisoned: {}", error);
            Vec::new()
        }
    };
    for binding in &removed {
        crate::recording::hotkeys::dispatch_action(
            app,
            app_state.inner(),
            &binding.id,
            binding.action,
            binding.trigger,
            KeyPhase::Released,
        );
    }

    match app_state.engine_bindings.lock() {
        Ok(mut guard) => *guard = new_bindings,
        Err(error) => log::error!("keytrigger: engine_bindings lock poisoned: {}", error),
    }

    app_state.trigger_engine.set_bindings(triggers);
}

/// Rebuild the complete native-engine binding list from persisted settings and runtime state.
pub fn rebuild_engine_bindings(app: &AppHandle) {
    let settings = match crate::commands::shortcuts::load_shortcut_settings(app) {
        Ok(settings) => settings,
        Err(error) => {
            log::error!("keytrigger: failed to load shortcut settings: {}", error);
            return;
        }
    };

    let mut bindings: Vec<ShortcutBinding> = settings
        .bindings
        .iter()
        .filter(|binding| binding.enabled)
        .cloned()
        .collect();

    let store = app.store("settings").ok();
    let hotkey = store
        .as_ref()
        .and_then(|store| store.get("hotkey"))
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "CommandOrControl+Shift+Space".to_string());
    let recording_mode_str = store
        .as_ref()
        .and_then(|store| store.get("recording_mode"))
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "toggle".to_string());
    let recording_mode = if recording_mode_str == "push_to_talk" {
        RecordingMode::PushToTalk
    } else {
        RecordingMode::Toggle
    };
    let use_different_ptt_key = store
        .as_ref()
        .and_then(|store| store.get("use_different_ptt_key"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let ptt_hotkey = store
        .as_ref()
        .and_then(|store| store.get("ptt_hotkey"))
        .and_then(|value| value.as_str().map(str::to_string));

    let has_bare_modifier_primary = bindings
        .iter()
        .any(|binding| binding.enabled && binding.id == "onboarding-primary-hold");

    if !has_bare_modifier_primary && !hotkey.trim().is_empty() {
        let hold_to_record = recording_mode == RecordingMode::PushToTalk;
        bindings.push(ShortcutBinding {
            id: "primary".to_string(),
            action: if hold_to_record {
                ShortcutAction::HoldToRecord
            } else {
                ShortcutAction::ToggleRecording
            },
            shortcut: hotkey,
            trigger: if hold_to_record {
                ShortcutTrigger::Hold
            } else {
                ShortcutTrigger::Pressed
            },
            enabled: true,
            allow_risky_combo: false,
            trigger_kind: TriggerKind::Combo,
            modifier: None,
        });
    }

    if recording_mode == RecordingMode::PushToTalk && use_different_ptt_key {
        if let Some(ptt_hotkey) = ptt_hotkey.filter(|value| !value.trim().is_empty()) {
            bindings.push(ShortcutBinding {
                id: "ptt".to_string(),
                action: ShortcutAction::HoldToRecord,
                shortcut: ptt_hotkey,
                trigger: ShortcutTrigger::Hold,
                enabled: true,
                allow_risky_combo: false,
                trigger_kind: TriggerKind::Combo,
                modifier: None,
            });
        }
    }

    if matches!(crate::get_recording_state(app), RecordingState::Recording) {
        bindings.push(ShortcutBinding {
            id: "escape-cancel".to_string(),
            action: ShortcutAction::CancelRecording,
            shortcut: "Escape".to_string(),
            trigger: ShortcutTrigger::Pressed,
            enabled: true,
            allow_risky_combo: true,
            trigger_kind: TriggerKind::Combo,
            modifier: None,
        });
    }

    apply_engine_bindings(app, &bindings);
}

/// Attempt to start the native trigger engine. On macOS without Accessibility
/// the tap fails to create and `start` returns an error; we log and rely on a
/// retry when `accessibility-granted` fires. Idempotent.
pub fn start_engine(app: &AppHandle) {
    let app_state = app.state::<AppState>();
    if app_state.trigger_engine.is_running() {
        return;
    }
    let handle = app.clone();
    match app_state
        .trigger_engine
        .start(move |ev| super::dispatch::on_engine_event(&handle, ev))
    {
        Ok(()) => log::info!("keytrigger: native trigger engine started"),
        Err(error) => log::warn!(
            "keytrigger: trigger engine not started ({}); will retry on permission grant",
            error
        ),
    }
}
