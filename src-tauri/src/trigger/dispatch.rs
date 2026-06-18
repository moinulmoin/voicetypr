//! Route `keytrigger::TriggerEvent`s to the shared shortcut dispatch path.

use keytrigger::{KeyPhase, TriggerEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::ShortcutState;

use crate::state::app_state::AppState;

/// Invoked on the engine dispatcher thread for every emitted trigger event.
pub fn on_engine_event(app: &AppHandle, ev: TriggerEvent) {
    let app_state = app.state::<AppState>();

    let binding = match app_state.engine_bindings.lock() {
        Ok(guard) => guard.iter().find(|b| b.id == ev.id).cloned(),
        Err(error) => {
            log::error!("keytrigger: engine_bindings lock poisoned: {}", error);
            return;
        }
    };

    // Transient window: the matcher may emit a synthetic Released for a binding
    // the app map no longer holds (it was just removed). The removal path
    // already released it, so this is safe to ignore.
    let Some(binding) = binding else {
        log::debug!("keytrigger: no engine binding for id {}", ev.id);
        return;
    };

    let state = match ev.phase {
        KeyPhase::Pressed => ShortcutState::Pressed,
        KeyPhase::Released => ShortcutState::Released,
    };

    crate::recording::hotkeys::dispatch_action(
        app,
        app_state.inner(),
        &binding.id,
        binding.action,
        binding.trigger,
        state,
    );
}
