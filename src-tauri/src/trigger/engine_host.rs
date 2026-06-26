//! Engine lifecycle + binding application for the host app.

use keytrigger::KeyPhase;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

use crate::commands::shortcuts::{ShortcutAction, ShortcutBinding, ShortcutTrigger, TriggerKind};
use crate::state::app_state::AppState;
use crate::{RecordingMode, RecordingState};

use super::{mapping, EngineBinding};

/// Recompute the engine's binding set from the full binding list and apply it.
///
/// Ordering matters for the core recording path. Before swapping the lookup
/// map we synthesize a `Released` for every OLD binding that is gone, OR that
/// persists with a changed action/trigger (e.g. `primary` flipping between
/// HoldToRecord/PushToTalk and ToggleRecording when recording mode changes) —
/// otherwise a mid-hold binding is never released and recording stays stuck.
/// `dispatch_action(Released)` is idempotent, so releasing unconditionally is safe.
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

    // Old bindings needing a synthetic Released BEFORE the swap: ids that are
    // gone, OR ids that persist with a changed action/trigger (e.g. `primary`
    // flipping recording mode). Released with the OLD binding so it routes
    // through the correct old hold/PTT handler; `dispatch_action(Released)`
    // is idempotent.
    let needs_release: Vec<EngineBinding> = match app_state.engine_bindings.lock() {
        Ok(guard) => bindings_needing_release(&guard, &new_bindings),
        Err(error) => {
            log::error!("keytrigger: engine_bindings lock poisoned: {}", error);
            Vec::new()
        }
    };
    for binding in &needs_release {
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

/// Old bindings that need a synthetic `Released` dispatched before the app-map
/// swap: ids that disappear entirely, OR ids that persist with a changed
/// action/trigger (e.g. `primary` flipping between HoldToRecord/Hold and
/// ToggleRecording/Pressed when recording mode changes). Returns the OLD
/// `EngineBinding`s so the release routes through the correct old hold/PTT
/// handler. Pure (no app state) so it can be unit-tested directly.
fn bindings_needing_release(old: &[EngineBinding], new: &[EngineBinding]) -> Vec<EngineBinding> {
    old.iter()
        .filter(|old_b| match new.iter().find(|n| n.id == old_b.id) {
            None => true,
            Some(new_b) => new_b.action != old_b.action || new_b.trigger != old_b.trigger,
        })
        .cloned()
        .collect()
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

    // Persisted bindings may predate current validation rules; the update/save
    // path rejects them, but at startup we read straight from the store. Skip
    // (and warn) any enabled binding that fails per-binding validation so it is
    // not installed into the engine. At minimum this excludes a custom combo
    // that resolves to bare `SingleKey(Escape)` — Escape is reserved for the
    // synthesized `escape-cancel`. The synthesized escape-cancel/primary/ptt
    // ids are appended below and bypass this gate.
    let mut bindings: Vec<ShortcutBinding> = settings
        .bindings
        .iter()
        .filter(|binding| {
            if !binding.enabled {
                return false;
            }
            if let Err(error) = mapping::validate(binding) {
                log::warn!(
                    "keytrigger: skipping invalid persisted binding '{}' at rebuild: {}",
                    binding.id,
                    error
                );
                return false;
            }
            if binding.trigger_kind == TriggerKind::Combo
                && crate::commands::key_normalizer::normalize_shortcut_keys(&binding.shortcut)
                    == "Escape"
            {
                log::warn!(
                    "keytrigger: skipping persisted binding '{}' at rebuild: Escape is reserved",
                    binding.id
                );
                return false;
            }
            true
        })
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

    // A bare-modifier primary is any enabled recording-action binding triggered
    // by a modifier hold or an isolated tap — the frontend's notion of "primary"
    // (GeneralSettings.tsx / shortcut-display.ts), not just the literal
    // "onboarding-primary-hold" id. When one exists we must NOT also synthesize
    // the combo `primary`, or a phantom consuming combo coexists with the
    // pass-through bare-modifier primary and breaks it.
    let has_bare_modifier_primary = bindings.iter().any(|binding| {
        binding.enabled
            && matches!(
                binding.action,
                ShortcutAction::HoldToRecord | ShortcutAction::ToggleRecording
            )
            && matches!(
                binding.trigger_kind,
                TriggerKind::ModifierHold | TriggerKind::IsolatedTap
            )
    });

    if !has_bare_modifier_primary {
        // Default-combo fallback: if the stored hotkey is empty/blank and there is
        // no bare-modifier primary either, the user would otherwise be left with NO
        // way to start recording by hotkey. The retired global-shortcut startup
        // fell back to this same default for that inconsistent state.
        let hotkey = if hotkey.trim().is_empty() {
            "CommandOrControl+Shift+Space".to_string()
        } else {
            hotkey
        };
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

#[cfg(test)]
mod tests {
    use super::bindings_needing_release;
    use crate::commands::shortcuts::{ShortcutAction, ShortcutTrigger};
    use crate::trigger::EngineBinding;

    fn binding(id: &str, action: ShortcutAction, trigger: ShortcutTrigger) -> EngineBinding {
        EngineBinding {
            id: id.to_string(),
            action,
            trigger,
        }
    }

    /// Fix E: a stable id (`primary`) that persists but flips its action/trigger
    /// when recording mode changes must be treated as needing a synthetic
    /// `Released`, carried out with the OLD binding so it routes through the old
    /// hold/PTT handler and recording is not left stuck.
    #[test]
    fn fix_e_changed_action_or_trigger_marks_for_release() {
        let old = vec![binding(
            "primary",
            ShortcutAction::HoldToRecord,
            ShortcutTrigger::Hold,
        )];
        let new = vec![binding(
            "primary",
            ShortcutAction::ToggleRecording,
            ShortcutTrigger::Pressed,
        )];
        let needs = bindings_needing_release(&old, &new);
        assert_eq!(needs.len(), 1, "a changed persistent id must be released");
        assert_eq!(needs[0].id, "primary");
        // The release must carry the OLD action/trigger to hit the old handler.
        assert_eq!(needs[0].action, ShortcutAction::HoldToRecord);
        assert_eq!(needs[0].trigger, ShortcutTrigger::Hold);
    }

    #[test]
    fn fix_e_unchanged_persistent_id_is_not_released() {
        let old = vec![binding(
            "primary",
            ShortcutAction::ToggleRecording,
            ShortcutTrigger::Pressed,
        )];
        let new = vec![binding(
            "primary",
            ShortcutAction::ToggleRecording,
            ShortcutTrigger::Pressed,
        )];
        assert!(bindings_needing_release(&old, &new).is_empty());
    }

    /// A genuinely removed id (e.g. escape-cancel when recording stops) must
    /// still be released — Fix E broadens the release path, it does not replace it.
    #[test]
    fn fix_e_removed_id_is_still_released() {
        let old = vec![binding(
            "escape-cancel",
            ShortcutAction::CancelRecording,
            ShortcutTrigger::Pressed,
        )];
        let new: Vec<EngineBinding> = vec![];
        let needs = bindings_needing_release(&old, &new);
        assert_eq!(needs.len(), 1);
        assert_eq!(needs[0].id, "escape-cancel");
    }

    #[test]
    fn fix_e_action_only_change_marks_for_release() {
        // Same trigger, different action — still a changed binding.
        let old = vec![binding(
            "x",
            ShortcutAction::HoldToRecord,
            ShortcutTrigger::Hold,
        )];
        let new = vec![binding(
            "x",
            ShortcutAction::ToggleRecording,
            ShortcutTrigger::Hold,
        )];
        assert_eq!(bindings_needing_release(&old, &new).len(), 1);
    }

    #[test]
    fn fix_e_trigger_only_change_marks_for_release() {
        // Same action, different trigger — still a changed binding.
        let old = vec![binding(
            "x",
            ShortcutAction::ToggleRecording,
            ShortcutTrigger::Pressed,
        )];
        let new = vec![binding(
            "x",
            ShortcutAction::ToggleRecording,
            ShortcutTrigger::Hold,
        )];
        assert_eq!(bindings_needing_release(&old, &new).len(), 1);
    }
}
