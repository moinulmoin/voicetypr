//! Map persisted `ShortcutBinding`s to `keytrigger::Trigger`s.

use std::time::Duration;

use keytrigger::{Modifier, Side, TapKey, Trigger};

use crate::commands::shortcuts::{
    ModifierKind, ShortcutAction, ShortcutBinding, ShortcutTrigger, SideKind, TriggerKind,
};

/// Default double-tap window; bounded so a stored value can't be absurd.
const DEFAULT_DOUBLE_TAP_MS: u64 = 350;
const MIN_DOUBLE_TAP_MS: u64 = 120;
const MAX_DOUBLE_TAP_MS: u64 = 1000;

/// True if this binding is handled by the native engine (not `global_shortcut`).
pub fn is_engine_kind(binding: &ShortcutBinding) -> bool {
    matches!(
        binding.trigger_kind,
        TriggerKind::ModifierHold | TriggerKind::DoubleTap
    )
}

/// Convert a binding to its `keytrigger::Trigger`, or `None` if it is not an
/// engine kind or is missing required fields.
pub fn to_trigger(binding: &ShortcutBinding) -> Option<Trigger> {
    match binding.trigger_kind {
        TriggerKind::Combo => None,
        TriggerKind::ModifierHold => {
            let spec = binding.modifier?;
            Some(Trigger::ModifierHold {
                modifier: modifier(spec.modifier),
                side: side(spec.side),
            })
        }
        TriggerKind::DoubleTap => {
            // v1: double-tap targets a modifier (e.g. double-tap Command).
            let spec = binding.modifier?;
            let within = Duration::from_millis(
                binding
                    .double_tap_ms
                    .unwrap_or(DEFAULT_DOUBLE_TAP_MS)
                    .clamp(MIN_DOUBLE_TAP_MS, MAX_DOUBLE_TAP_MS),
            );
            Some(Trigger::DoubleTap {
                key: TapKey::Mod(modifier(spec.modifier), side(spec.side)),
                within,
            })
        }
    }
}

/// Validate an enabled engine-kind binding (the `global_shortcut` validator does
/// not run for these). Returns a user-facing error string on failure.
pub fn validate(binding: &ShortcutBinding) -> Result<(), String> {
    if binding.id.trim().is_empty() {
        return Err("Shortcut binding id is required".to_string());
    }
    match binding.trigger_kind {
        TriggerKind::Combo => {}
        TriggerKind::ModifierHold => {
            if binding.modifier.is_none() {
                return Err(format!(
                    "Hold-a-modifier binding '{}' requires a modifier",
                    binding.id
                ));
            }
            // The engine emits Pressed on modifier-down / Released on up; only the
            // Hold trigger + HoldToRecord action consume that (dispatch_action
            // otherwise silently no-ops).
            if binding.trigger != ShortcutTrigger::Hold
                || binding.action != ShortcutAction::HoldToRecord
            {
                return Err(
                    "Hold-a-modifier shortcuts can only be used for Hold-to-record".to_string(),
                );
            }
        }
        TriggerKind::DoubleTap => {
            if binding.modifier.is_none() {
                return Err(format!(
                    "Double-tap binding '{}' requires a modifier",
                    binding.id
                ));
            }
            // Double-tap is a one-shot press; it cannot drive a hold.
            if binding.trigger != ShortcutTrigger::Pressed {
                return Err("Double-tap shortcuts must use the press trigger".to_string());
            }
            if binding.action == ShortcutAction::HoldToRecord {
                return Err("Double-tap cannot be used for Hold-to-record".to_string());
            }
        }
    }
    Ok(())
}

fn modifier(kind: ModifierKind) -> Modifier {
    match kind {
        ModifierKind::Alt => Modifier::Alt,
        ModifierKind::Control => Modifier::Control,
        ModifierKind::Meta => Modifier::Meta,
        ModifierKind::Shift => Modifier::Shift,
    }
}

fn side(kind: SideKind) -> Side {
    match kind {
        SideKind::Left => Side::Left,
        SideKind::Right => Side::Right,
        SideKind::Either => Side::Either,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_engine_kind, to_trigger, validate};
    use crate::commands::shortcuts::{
        ModifierKind, ModifierSpec, ShortcutAction, ShortcutBinding, ShortcutTrigger, SideKind,
        TriggerKind,
    };
    use keytrigger::{Modifier, Side, Trigger};

    fn hold_binding() -> ShortcutBinding {
        ShortcutBinding {
            id: "x".to_string(),
            action: ShortcutAction::HoldToRecord,
            shortcut: String::new(),
            trigger: ShortcutTrigger::Hold,
            enabled: true,
            allow_risky_combo: false,
            trigger_kind: TriggerKind::ModifierHold,
            modifier: Some(ModifierSpec {
                modifier: ModifierKind::Alt,
                side: SideKind::Right,
            }),
            double_tap_ms: None,
        }
    }

    #[test]
    fn combo_is_not_engine_kind() {
        let mut b = hold_binding();
        b.trigger_kind = TriggerKind::Combo;
        assert!(!is_engine_kind(&b));
        assert!(to_trigger(&b).is_none());
    }

    #[test]
    fn modifier_hold_maps_and_validates() {
        let b = hold_binding();
        assert!(is_engine_kind(&b));
        assert!(validate(&b).is_ok());
        assert!(matches!(
            to_trigger(&b),
            Some(Trigger::ModifierHold {
                modifier: Modifier::Alt,
                side: Side::Right
            })
        ));
    }

    #[test]
    fn modifier_hold_rejects_non_hold_record_action() {
        let mut b = hold_binding();
        b.action = ShortcutAction::ToggleRecording;
        assert!(validate(&b).is_err());
    }

    #[test]
    fn double_tap_maps_and_validates() {
        let mut b = hold_binding();
        b.trigger_kind = TriggerKind::DoubleTap;
        b.trigger = ShortcutTrigger::Pressed;
        b.action = ShortcutAction::ToggleRecording;
        assert!(validate(&b).is_ok());
        assert!(matches!(to_trigger(&b), Some(Trigger::DoubleTap { .. })));
    }

    #[test]
    fn double_tap_rejects_hold_to_record() {
        let mut b = hold_binding();
        b.trigger_kind = TriggerKind::DoubleTap;
        b.trigger = ShortcutTrigger::Pressed;
        b.action = ShortcutAction::HoldToRecord;
        assert!(validate(&b).is_err());
    }

    #[test]
    fn engine_kind_missing_modifier_rejected() {
        let mut b = hold_binding();
        b.modifier = None;
        assert!(validate(&b).is_err());
        assert!(to_trigger(&b).is_none());
    }
}
