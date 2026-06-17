use crate::ai::prompts::EnhancementPreset;
use crate::commands::shortcuts::{
    action_preset, hold_shortcut_transition, next_ai_enabled, normalized_custom_shortcut_conflict,
    pressed_shortcut_should_run, validate_shortcut_settings, CustomHoldTransition,
    ExistingShortcutStrings, ShortcutAction, ShortcutBinding, ShortcutSettings, ShortcutTrigger,
};
use std::collections::HashSet;
use tauri_plugin_global_shortcut::ShortcutState;

fn binding(action: ShortcutAction, shortcut: &str) -> ShortcutBinding {
    ShortcutBinding {
        id: format!("{:?}-{}", action, shortcut),
        action,
        shortcut: shortcut.to_string(),
        trigger: ShortcutTrigger::Pressed,
        enabled: true,
        allow_risky_combo: false,
    }
}

#[test]
fn shortcut_settings_default_empty() {
    assert!(ShortcutSettings::default().bindings.is_empty());
}

#[test]
fn single_key_hold_to_record_requires_risky_flag() {
    let mut single_key = binding(ShortcutAction::HoldToRecord, "F1");
    single_key.trigger = ShortcutTrigger::Hold;
    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![single_key.clone()],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_err());

    single_key.allow_risky_combo = true;
    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![single_key],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_ok());

    let mut modifier_only = binding(ShortcutAction::HoldToRecord, "Alt");
    modifier_only.trigger = ShortcutTrigger::Hold;
    modifier_only.allow_risky_combo = true;
    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![modifier_only],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_err());
}

#[test]
fn hold_to_record_requires_hold_trigger() {
    let mut hold = binding(ShortcutAction::HoldToRecord, "CommandOrControl+Alt+H");
    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![hold.clone()],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_err());

    hold.trigger = ShortcutTrigger::Hold;
    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![hold],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_ok());
}

#[test]
fn non_hold_actions_reject_hold_trigger() {
    let mut toggle = binding(ShortcutAction::ToggleRecording, "CommandOrControl+Alt+T");
    toggle.trigger = ShortcutTrigger::Hold;

    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![toggle],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_err());
}

#[test]
fn duplicate_enabled_custom_binding_rejected() {
    let first = binding(
        ShortcutAction::CopyLastTranscription,
        "CommandOrControl+Alt+C",
    );
    let second = binding(ShortcutAction::PasteLastTranscription, "Cmd+Alt+C");

    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![first, second],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_err());
}

#[test]
fn duplicate_existing_hotkey_rejected() {
    let existing = ExistingShortcutStrings {
        primary_hotkey: Some("CommandOrControl+Shift+Space".to_string()),
        ptt_hotkey: Some("Alt+Space".to_string()),
    };

    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![binding(
                ShortcutAction::OpenDashboard,
                "CommandOrControl+Shift+Space",
            )],
        },
        &existing,
    )
    .is_err());

    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![binding(ShortcutAction::OpenDashboard, "Alt+Space")],
        },
        &existing,
    )
    .is_err());
}

#[test]
fn escape_is_reserved_even_for_risky_hold_binding() {
    let mut escape = binding(ShortcutAction::HoldToRecord, "Escape");
    escape.trigger = ShortcutTrigger::Hold;
    escape.allow_risky_combo = true;

    assert!(validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![escape],
        },
        &ExistingShortcutStrings::default(),
    )
    .is_err());
}

#[test]
fn normalized_custom_shortcut_conflict_ignores_disabled_bindings() {
    let enabled = binding(ShortcutAction::OpenDashboard, "CommandOrControl+Alt+D");
    let mut disabled = binding(
        ShortcutAction::CopyLastTranscription,
        "CommandOrControl+Alt+C",
    );
    disabled.enabled = false;

    let settings = ShortcutSettings {
        bindings: vec![enabled.clone(), disabled],
    };

    assert_eq!(
        normalized_custom_shortcut_conflict("CommandOrControl+Alt+D", &settings),
        Some(enabled.id)
    );
    assert_eq!(
        normalized_custom_shortcut_conflict("CommandOrControl+Alt+C", &settings),
        None
    );
}

#[test]
fn pressed_trigger_dedupes_until_release() {
    let mut active = HashSet::new();

    assert!(pressed_shortcut_should_run(
        &mut active,
        "copy",
        ShortcutState::Pressed,
    ));
    assert!(!pressed_shortcut_should_run(
        &mut active,
        "copy",
        ShortcutState::Pressed,
    ));
    assert!(!pressed_shortcut_should_run(
        &mut active,
        "copy",
        ShortcutState::Released,
    ));
    assert!(pressed_shortcut_should_run(
        &mut active,
        "copy",
        ShortcutState::Pressed,
    ));
}

#[test]
fn multiple_hold_bindings_stop_only_after_last_release() {
    let mut active = HashSet::new();

    assert_eq!(
        hold_shortcut_transition(&mut active, "hold-a", ShortcutState::Pressed),
        CustomHoldTransition::Start
    );
    assert_eq!(
        hold_shortcut_transition(&mut active, "hold-b", ShortcutState::Pressed),
        CustomHoldTransition::Noop
    );
    assert_eq!(
        hold_shortcut_transition(&mut active, "hold-a", ShortcutState::Released),
        CustomHoldTransition::Noop
    );
    assert_eq!(
        hold_shortcut_transition(&mut active, "hold-b", ShortcutState::Released),
        CustomHoldTransition::Stop
    );
}

#[test]
fn disabled_invalid_binding_does_not_fail_validation() {
    let mut disabled = binding(ShortcutAction::OpenDashboard, "");
    disabled.enabled = false;

    let validated = validate_shortcut_settings(
        ShortcutSettings {
            bindings: vec![disabled],
        },
        &ExistingShortcutStrings::default(),
    )
    .expect("disabled invalid shortcut should not be registered or validated");

    assert_eq!(validated.bindings.len(), 1);
    assert!(!validated.bindings[0].enabled);
}

#[test]
fn mode_actions_map_to_expected_presets() {
    assert_eq!(
        action_preset(ShortcutAction::SetPersonalDictation),
        Some(EnhancementPreset::PersonalDictation)
    );
    assert_eq!(
        action_preset(ShortcutAction::SetCleanDictation),
        Some(EnhancementPreset::CleanDictation)
    );
    assert_eq!(
        action_preset(ShortcutAction::SetWriting),
        Some(EnhancementPreset::Writing)
    );
    assert_eq!(
        action_preset(ShortcutAction::SetNotes),
        Some(EnhancementPreset::Notes)
    );
    assert_eq!(
        action_preset(ShortcutAction::SetMessage),
        Some(EnhancementPreset::Message)
    );
    assert_eq!(
        action_preset(ShortcutAction::SetCode),
        Some(EnhancementPreset::Code)
    );
    assert_eq!(action_preset(ShortcutAction::OpenDashboard), None);
}

#[test]
fn next_ai_enabled_toggles_correctly() {
    // Disabling always works regardless of can_enable
    assert_eq!(next_ai_enabled(true, true), Some(false));
    assert_eq!(next_ai_enabled(true, false), Some(false));
    // Enabling only works when can_enable is true
    assert_eq!(next_ai_enabled(false, true), Some(true));
    // Enabling is refused without a usable AI setup
    assert_eq!(next_ai_enabled(false, false), None);
}
