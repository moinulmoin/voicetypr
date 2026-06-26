//! Native key-trigger engine integration.
//!
//! Wires the standalone `keytrigger` crate into Voicetypr so Combo/SingleKey
//! shortcuts and modifier-only bindings drive the same shortcut actions.

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
