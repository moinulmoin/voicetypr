//! Native key-trigger engine integration (plan 022, P2).
//!
//! Wires the standalone `keytrigger` crate into Voicetypr so trigger kinds that
//! `tauri-plugin-global-shortcut` cannot express — modifier-only holds (e.g.
//! hold Right-Option to talk) and double-taps — drive the same shortcut actions.
//! Observation-only and additive: Combo/SingleKey bindings still use
//! `global_shortcut`; only `ModifierHold`/`DoubleTap` bindings are routed here.

pub mod dispatch;
pub mod engine_host;
pub mod mapping;

use crate::commands::shortcuts::{ShortcutAction, ShortcutTrigger};

/// An engine-routed binding: the engine emits `TriggerEvent { id }` and we look
/// up the action/trigger here to run the shared dispatch path.
#[derive(Debug, Clone)]
pub struct EngineBinding {
    pub id: String,
    pub action: ShortcutAction,
    pub trigger: ShortcutTrigger,
}
