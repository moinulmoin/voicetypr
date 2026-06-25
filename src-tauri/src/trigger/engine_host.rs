//! Engine lifecycle + binding application for the host app.

use std::collections::HashSet;

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::ShortcutState;

use crate::commands::shortcuts::ShortcutBinding;
use crate::state::app_state::AppState;

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
            ShortcutState::Released,
        );
    }

    match app_state.engine_bindings.lock() {
        Ok(mut guard) => *guard = new_bindings,
        Err(error) => log::error!("keytrigger: engine_bindings lock poisoned: {}", error),
    }

    app_state.trigger_engine.set_bindings(triggers);
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
