use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_store::StoreExt;

use crate::ai::prompts::{EnhancementOptions, EnhancementPreset};
pub use crate::commands::key_normalizer::is_single_key_shortcut;
use crate::commands::key_normalizer::{
    normalize_shortcut_keys, validate_key_combination,
    validate_key_combination_allowing_safe_single_key,
};
use crate::AppState;

const SHORTCUT_BINDINGS_KEY: &str = "shortcut_bindings";

pub const MAX_SINGLE_KEY_BINDINGS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutAction {
    ToggleRecording,
    HoldToRecord,
    CancelRecording,
    CopyLastTranscription,
    PasteLastTranscription,
    CycleFormattingMode,
    ToggleAiFormatting,
    SetPersonalDictation,
    SetCleanDictation,
    SetWriting,
    SetNotes,
    SetMessage,
    SetCode,
    OpenDashboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutTrigger {
    Pressed,
    Hold,
}

/// How a binding is triggered. `Combo` (default) uses the legacy
/// `global_shortcut` path; `ModifierHold`/`DoubleTap` use the native engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    #[default]
    Combo,
    ModifierHold,
    DoubleTap,
    IsolatedTap,
}

/// A side-specific modifier for native (`ModifierHold`/`DoubleTap`) bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierSpec {
    pub modifier: ModifierKind,
    pub side: SideKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierKind {
    Alt,
    Control,
    Meta,
    Shift,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideKind {
    Left,
    Right,
    Either,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutBinding {
    pub id: String,
    pub action: ShortcutAction,
    pub shortcut: String,
    pub trigger: ShortcutTrigger,
    pub enabled: bool,
    pub allow_risky_combo: bool,
    #[serde(default)]
    pub trigger_kind: TriggerKind,
    #[serde(default)]
    pub modifier: Option<ModifierSpec>,
    #[serde(default)]
    pub double_tap_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutSettings {
    pub bindings: Vec<ShortcutBinding>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShortcutActionDefinition {
    pub action: ShortcutAction,
    pub label: &'static str,
    pub description: &'static str,
    pub section: &'static str,
    pub recommended_trigger: ShortcutTrigger,
    pub allows_single_key: bool,
}

#[derive(Debug, Clone)]
pub struct RegisteredShortcutBinding {
    pub id: String,
    pub action: ShortcutAction,
    pub trigger: ShortcutTrigger,
    pub shortcut: Shortcut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CustomHoldTransition {
    Start,
    Stop,
    Noop,
}

pub(crate) fn pressed_shortcut_should_run(
    active_bindings: &mut HashSet<String>,
    binding_id: &str,
    event_state: ShortcutState,
) -> bool {
    match event_state {
        ShortcutState::Pressed => active_bindings.insert(binding_id.to_string()),
        ShortcutState::Released => {
            active_bindings.remove(binding_id);
            false
        }
    }
}

pub(crate) fn hold_shortcut_transition(
    active_bindings: &mut HashSet<String>,
    binding_id: &str,
    event_state: ShortcutState,
) -> CustomHoldTransition {
    match event_state {
        ShortcutState::Pressed => {
            if active_bindings.insert(binding_id.to_string()) && active_bindings.len() == 1 {
                CustomHoldTransition::Start
            } else {
                CustomHoldTransition::Noop
            }
        }
        ShortcutState::Released => {
            if active_bindings.remove(binding_id) && active_bindings.is_empty() {
                CustomHoldTransition::Stop
            } else {
                CustomHoldTransition::Noop
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExistingShortcutStrings {
    pub primary_hotkey: Option<String>,
    pub ptt_hotkey: Option<String>,
}

#[derive(Debug, Clone)]
struct PreparedShortcutBinding {
    binding: ShortcutBinding,
    shortcut: Option<Shortcut>,
}

#[tauri::command]
pub fn get_shortcut_settings(app: AppHandle) -> Result<ShortcutSettings, String> {
    load_shortcut_settings(&app)
}

#[tauri::command]
pub fn list_shortcut_actions() -> Vec<ShortcutActionDefinition> {
    shortcut_action_definitions()
}

#[tauri::command]
pub fn update_shortcut_settings(
    app: AppHandle,
    settings: ShortcutSettings,
) -> Result<ShortcutSettings, String> {
    let existing = current_existing_shortcuts(&app);
    let prepared = prepare_shortcut_settings(settings, &existing)?;
    let sanitized = ShortcutSettings {
        bindings: prepared.iter().map(|item| item.binding.clone()).collect(),
    };

    let app_state = app.state::<AppState>();
    let previous = app_state
        .custom_shortcut_bindings
        .lock()
        .map_err(|e| format!("Failed to lock custom shortcut state: {}", e))?
        .clone();

    unregister_registered_shortcuts(&app, &previous);

    let mut newly_registered = Vec::new();
    for item in prepared.iter().filter(|item| item.binding.enabled) {
        let Some(shortcut) = item.shortcut else {
            continue;
        };
        if let Err(error) = app.global_shortcut().register(shortcut) {
            unregister_registered_shortcuts(&app, &newly_registered);
            restore_registered_shortcuts(&app, &app_state, previous);
            return Err(format!(
                "Failed to register shortcut '{}' for {:?}: {}",
                item.binding.shortcut, item.binding.action, error
            ));
        }

        newly_registered.push(RegisteredShortcutBinding {
            id: item.binding.id.clone(),
            action: item.binding.action,
            trigger: item.binding.trigger,
            shortcut,
        });
    }

    if let Err(error) = save_shortcut_settings(&app, &sanitized) {
        unregister_registered_shortcuts(&app, &newly_registered);
        restore_registered_shortcuts(&app, &app_state, previous);
        return Err(error);
    }

    // Route native (ModifierHold/DoubleTap) bindings to the trigger engine
    // BEFORE clearing shared active state, so a removed/edited hold-to-record
    // that is mid-hold is released (Stop) while its id is still tracked.
    crate::trigger::engine_host::apply_engine_bindings(&app, &sanitized.bindings);

    {
        let mut guard = app_state
            .custom_shortcut_bindings
            .lock()
            .map_err(|e| format!("Failed to lock custom shortcut state: {}", e))?;
        *guard = newly_registered;
        clear_active_custom_shortcut_state(&app_state);
    }

    Ok(sanitized)
}

pub fn register_saved_shortcuts(app: &AppHandle) -> Result<(), String> {
    let settings = load_shortcut_settings(app)?;
    let existing = current_existing_shortcuts(app);
    let primary = existing
        .primary_hotkey
        .as_deref()
        .map(normalize_shortcut_keys)
        .filter(|value| !value.is_empty());
    let ptt = existing
        .ptt_hotkey
        .as_deref()
        .map(normalize_shortcut_keys)
        .filter(|value| !value.is_empty());
    let app_state = app.state::<AppState>();

    let mut registered = Vec::new();
    let mut seen_enabled = HashSet::new();
    let mut single_key_count: usize = 0;
    let mut engine_source: Vec<ShortcutBinding> = Vec::new();
    for binding in settings
        .bindings
        .into_iter()
        .filter(|binding| binding.enabled)
    {
        let sanitized = ShortcutBinding {
            id: binding.id.trim().to_string(),
            shortcut: binding.shortcut.trim().to_string(),
            ..binding
        };

        // Native engine kinds (modifier-hold / double-tap) bypass global_shortcut.
        if crate::trigger::mapping::is_engine_kind(&sanitized) {
            engine_source.push(sanitized);
            continue;
        }

        let shortcut = match prepare_enabled_shortcut(
            &sanitized,
            primary.as_deref(),
            ptt.as_deref(),
            &mut seen_enabled,
        ) {
            Ok(shortcut) => shortcut,
            Err(error) => {
                log::error!(
                    "Skipping saved shortcut '{}' for {:?}: {}",
                    sanitized.shortcut,
                    sanitized.action,
                    error
                );
                continue;
            }
        };

        let normalized = normalize_shortcut_keys(&sanitized.shortcut);
        if is_single_key_shortcut(&normalized) {
            if single_key_count >= MAX_SINGLE_KEY_BINDINGS {
                log::warn!(
                    "Skipping saved single-key shortcut '{}' for {:?}: exceeds the {}-binding cap",
                    sanitized.shortcut,
                    sanitized.action,
                    MAX_SINGLE_KEY_BINDINGS,
                );
                continue;
            }
            single_key_count += 1;
        }

        match app.global_shortcut().register(shortcut) {
            Ok(_) => registered.push(RegisteredShortcutBinding {
                id: sanitized.id,
                action: sanitized.action,
                trigger: sanitized.trigger,
                shortcut,
            }),
            Err(error) => {
                log::error!(
                    "Failed to register saved shortcut '{}' for {:?}: {}",
                    sanitized.shortcut,
                    sanitized.action,
                    error
                );
            }
        }
    }
    let mut guard = app_state
        .custom_shortcut_bindings
        .lock()
        .map_err(|e| format!("Failed to lock custom shortcut state: {}", e))?;
    *guard = registered;
    clear_active_custom_shortcut_state(&app_state);

    crate::trigger::engine_host::apply_engine_bindings(app, &engine_source);
    Ok(())
}

pub fn matching_custom_binding(
    app_state: &AppState,
    shortcut: &Shortcut,
) -> Option<RegisteredShortcutBinding> {
    app_state
        .custom_shortcut_bindings
        .lock()
        .ok()
        .and_then(|bindings| {
            bindings
                .iter()
                .find(|binding| &binding.shortcut == shortcut)
                .cloned()
        })
}

pub fn registered_custom_shortcut_conflict(
    app_state: &AppState,
    shortcut: &Shortcut,
) -> Option<RegisteredShortcutBinding> {
    app_state
        .custom_shortcut_bindings
        .lock()
        .ok()
        .and_then(|bindings| {
            bindings
                .iter()
                .find(|binding| &binding.shortcut == shortcut)
                .cloned()
        })
}

#[cfg(test)]
pub fn normalized_custom_shortcut_conflict(
    normalized_shortcut: &str,
    settings: &ShortcutSettings,
) -> Option<String> {
    settings
        .bindings
        .iter()
        .filter(|binding| binding.enabled)
        .find(|binding| normalize_shortcut_keys(&binding.shortcut) == normalized_shortcut)
        .map(|binding| binding.id.clone())
}

pub fn action_preset(action: ShortcutAction) -> Option<EnhancementPreset> {
    match action {
        ShortcutAction::SetPersonalDictation => Some(EnhancementPreset::PersonalDictation),
        ShortcutAction::SetCleanDictation => Some(EnhancementPreset::CleanDictation),
        ShortcutAction::SetWriting => Some(EnhancementPreset::Writing),
        ShortcutAction::SetNotes => Some(EnhancementPreset::Notes),
        ShortcutAction::SetMessage => Some(EnhancementPreset::Message),
        ShortcutAction::SetCode => Some(EnhancementPreset::Code),
        _ => None,
    }
}

pub async fn latest_copyable_transcription_text(app: &AppHandle) -> Result<Option<String>, String> {
    let store = app
        .store("transcriptions")
        .map_err(|e| format!("Failed to open transcriptions store: {}", e))?;
    let mut entries: Vec<(String, serde_json::Value)> = Vec::new();
    for key in store.keys() {
        if let Some(value) = store.get(&key) {
            entries.push((key.to_string(), value));
        }
    }

    Ok(
        crate::menu::latest_copyable_transcription_id(&entries).and_then(|timestamp| {
            store.get(&timestamp).and_then(|value| {
                value
                    .get("text")
                    .and_then(|text| text.as_str().map(str::to_string))
            })
        }),
    )
}

pub async fn set_formatting_preset(
    app: AppHandle,
    preset: EnhancementPreset,
) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    store.set(
        "enhancement_options",
        serde_json::to_value(EnhancementOptions { preset })
            .map_err(|e| format!("Failed to serialize enhancement options: {}", e))?,
    );
    store.save().map_err(|e| e.to_string())?;
    drop(store);

    let app_state = app.state::<AppState>();
    *app_state.recording_config_cache.write().await = None;
    let _ = app.emit("settings-changed", ());
    log::info!("Shortcut updated formatting preset to {:?}", preset);
    Ok(())
}

pub async fn cycle_formatting_preset(app: AppHandle) -> Result<(), String> {
    let current = current_formatting_preset(&app).unwrap_or(EnhancementPreset::PersonalDictation);
    let next = match current {
        EnhancementPreset::PersonalDictation => EnhancementPreset::CleanDictation,
        EnhancementPreset::CleanDictation => EnhancementPreset::Writing,
        EnhancementPreset::Writing => EnhancementPreset::Notes,
        EnhancementPreset::Notes => EnhancementPreset::Message,
        EnhancementPreset::Message => EnhancementPreset::Code,
        EnhancementPreset::Code => EnhancementPreset::PersonalDictation,
    };
    set_formatting_preset(app, next).await
}

/// Pure decision function: what should ai_enabled become?
/// Returns None if enabling is refused (no usable AI setup).
pub fn next_ai_enabled(current: bool, can_enable: bool) -> Option<bool> {
    if current {
        Some(false) // always allow disabling
    } else if can_enable {
        Some(true)
    } else {
        None // refuse to enable without a usable AI setup
    }
}

pub async fn toggle_ai_formatting(app: AppHandle) -> Result<(), String> {
    use crate::commands::ai::get_ai_settings;
    let ai_settings = get_ai_settings(app.clone()).await?;
    let current = ai_settings.enabled;
    let can_enable = ai_settings.has_api_key && !ai_settings.model.is_empty();

    match next_ai_enabled(current, can_enable) {
        Some(true) => {
            let store = app.store("settings").map_err(|e| e.to_string())?;
            store.set("ai_enabled", serde_json::Value::Bool(true));
            store.save().map_err(|e| e.to_string())?;
            drop(store);
            crate::commands::audio::invalidate_recording_config_cache(&app).await;
            crate::commands::audio::pill_toast(&app, "AI formatting on", 2500);
            let _ = crate::emit_to_window(&app, "main", "ai-enabled-changed", true);
        }
        Some(false) => {
            let store = app.store("settings").map_err(|e| e.to_string())?;
            store.set("ai_enabled", serde_json::Value::Bool(false));
            store.save().map_err(|e| e.to_string())?;
            drop(store);
            crate::commands::audio::invalidate_recording_config_cache(&app).await;
            crate::commands::audio::pill_toast(&app, "AI formatting off", 2500);
            let _ = crate::emit_to_window(&app, "main", "ai-enabled-changed", false);
        }
        None => {
            crate::commands::audio::pill_toast(
                &app,
                "Set up an AI model in Settings to use formatting",
                3500,
            );
        }
    }
    Ok(())
}

#[cfg(test)]
pub fn validate_shortcut_settings(
    settings: ShortcutSettings,
    existing: &ExistingShortcutStrings,
) -> Result<ShortcutSettings, String> {
    prepare_shortcut_settings(settings, existing).map(|prepared| ShortcutSettings {
        bindings: prepared.into_iter().map(|item| item.binding).collect(),
    })
}

fn prepare_shortcut_settings(
    settings: ShortcutSettings,
    existing: &ExistingShortcutStrings,
) -> Result<Vec<PreparedShortcutBinding>, String> {
    let mut seen_enabled = HashSet::new();
    let primary = existing
        .primary_hotkey
        .as_deref()
        .map(normalize_shortcut_keys)
        .filter(|value| !value.is_empty());
    let ptt = existing
        .ptt_hotkey
        .as_deref()
        .map(normalize_shortcut_keys)
        .filter(|value| !value.is_empty());

    let mut prepared = Vec::with_capacity(settings.bindings.len());
    for binding in settings.bindings {
        let sanitized = ShortcutBinding {
            id: binding.id.trim().to_string(),
            shortcut: binding.shortcut.trim().to_string(),
            ..binding
        };

        if sanitized.enabled && crate::trigger::mapping::is_engine_kind(&sanitized) {
            // Native engine kinds are not tauri Shortcuts; validate separately
            // and keep them out of the global_shortcut registration vector.
            crate::trigger::mapping::validate(&sanitized)?;
            prepared.push(PreparedShortcutBinding {
                binding: sanitized,
                shortcut: None,
            });
        } else if sanitized.enabled {
            let shortcut = prepare_enabled_shortcut(
                &sanitized,
                primary.as_deref(),
                ptt.as_deref(),
                &mut seen_enabled,
            )?;
            prepared.push(PreparedShortcutBinding {
                binding: sanitized,
                shortcut: Some(shortcut),
            });
        } else {
            prepared.push(PreparedShortcutBinding {
                binding: sanitized,
                shortcut: None,
            });
        }
    }

    let single_key_count = prepared
        .iter()
        .filter(|pb| {
            pb.binding.enabled
                && pb.shortcut.is_some()
                && is_single_key_shortcut(&normalize_shortcut_keys(&pb.binding.shortcut))
        })
        .count();
    if single_key_count > MAX_SINGLE_KEY_BINDINGS {
        return Err(format!(
            "You can set at most {} single-key shortcuts -- you have {}. \
             Disable or remove some single-key bindings.",
            MAX_SINGLE_KEY_BINDINGS, single_key_count
        ));
    }

    Ok(prepared)
}

fn prepare_enabled_shortcut(
    binding: &ShortcutBinding,
    primary: Option<&str>,
    ptt: Option<&str>,
    seen_enabled: &mut HashSet<String>,
) -> Result<Shortcut, String> {
    validate_enabled_binding(binding)?;
    let normalized = normalize_shortcut_keys(&binding.shortcut);

    if primary == Some(normalized.as_str()) {
        return Err(format!(
            "Shortcut '{}' duplicates the primary recording hotkey",
            binding.shortcut
        ));
    }
    if ptt == Some(normalized.as_str()) {
        return Err(format!(
            "Shortcut '{}' duplicates the push-to-talk hotkey",
            binding.shortcut
        ));
    }
    if !seen_enabled.insert(normalized.clone()) {
        return Err(format!(
            "Duplicate enabled shortcut binding: {}",
            binding.shortcut
        ));
    }

    if normalized == "Escape" {
        return Err("Escape is reserved for recording cancellation".to_string());
    }

    normalized
        .parse::<Shortcut>()
        .map_err(|e| format!("Invalid shortcut '{}': {}", binding.shortcut, e))
}

pub(crate) fn load_shortcut_settings(app: &AppHandle) -> Result<ShortcutSettings, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    match store.get(SHORTCUT_BINDINGS_KEY) {
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|e| format!("Failed to parse shortcut settings: {}", e)),
        None => Ok(ShortcutSettings::default()),
    }
}

fn save_shortcut_settings(app: &AppHandle, settings: &ShortcutSettings) -> Result<(), String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    store.set(
        SHORTCUT_BINDINGS_KEY,
        serde_json::to_value(settings)
            .map_err(|e| format!("Failed to serialize shortcut settings: {}", e))?,
    );
    store.save().map_err(|e| e.to_string())
}

fn current_existing_shortcuts(app: &AppHandle) -> ExistingShortcutStrings {
    let store = match app.store("settings") {
        Ok(store) => store,
        Err(_) => {
            return ExistingShortcutStrings {
                primary_hotkey: Some("CommandOrControl+Shift+Space".to_string()),
                ptt_hotkey: None,
            }
        }
    };

    let primary_hotkey = store
        .get("hotkey")
        .and_then(|value| value.as_str().map(str::to_string))
        .or_else(|| Some("CommandOrControl+Shift+Space".to_string()));

    let recording_mode = store
        .get("recording_mode")
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "toggle".to_string());
    let use_different_ptt_key = store
        .get("use_different_ptt_key")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let ptt_hotkey = if recording_mode == "push_to_talk" && use_different_ptt_key {
        store
            .get("ptt_hotkey")
            .and_then(|value| value.as_str().map(str::to_string))
    } else {
        None
    };

    ExistingShortcutStrings {
        primary_hotkey,
        ptt_hotkey,
    }
}

fn validate_enabled_binding(binding: &ShortcutBinding) -> Result<(), String> {
    if binding.id.is_empty() {
        return Err("Shortcut binding id is required".to_string());
    }
    if binding.shortcut.is_empty() || binding.shortcut.len() > 100 {
        return Err(format!("Invalid shortcut for binding '{}'", binding.id));
    }

    validate_trigger_matches_action(binding)?;

    let normalized = normalize_shortcut_keys(&binding.shortcut);
    if allows_single_key(binding) {
        validate_key_combination_allowing_safe_single_key(&normalized)?;
    } else {
        validate_key_combination(&normalized)?;
    }

    Ok(())
}

fn validate_trigger_matches_action(binding: &ShortcutBinding) -> Result<(), String> {
    match (binding.action, binding.trigger) {
        (ShortcutAction::HoldToRecord, ShortcutTrigger::Hold) => Ok(()),
        (ShortcutAction::HoldToRecord, ShortcutTrigger::Pressed) => {
            Err("Hold-to-record shortcuts must use the hold trigger".to_string())
        }
        (_, ShortcutTrigger::Pressed) => Ok(()),
        (_, ShortcutTrigger::Hold) => {
            Err("Only hold-to-record shortcuts can use the hold trigger".to_string())
        }
    }
}

fn allows_single_key(binding: &ShortcutBinding) -> bool {
    binding.allow_risky_combo
}

fn unregister_registered_shortcuts(app: &AppHandle, bindings: &[RegisteredShortcutBinding]) {
    for binding in bindings {
        if let Err(error) = app.global_shortcut().unregister(binding.shortcut) {
            log::debug!(
                "Failed to unregister custom shortcut {:?}: {}",
                binding.action,
                error
            );
        }
    }
}

fn clear_active_custom_shortcut_state(app_state: &AppState) {
    if let Ok(mut active_holds) = app_state.active_custom_hold_bindings.lock() {
        active_holds.clear();
    }
    if let Ok(mut active_pressed) = app_state.active_custom_pressed_bindings.lock() {
        active_pressed.clear();
    }
}

fn restore_registered_shortcuts(
    app: &AppHandle,
    app_state: &tauri::State<'_, AppState>,
    previous: Vec<RegisteredShortcutBinding>,
) {
    let mut restored = Vec::with_capacity(previous.len());
    for binding in previous {
        match app.global_shortcut().register(binding.shortcut) {
            Ok(_) => restored.push(binding),
            Err(error) => log::error!(
                "Failed to restore custom shortcut {:?} after update failure: {}",
                binding.action,
                error
            ),
        }
    }

    if let Ok(mut guard) = app_state.custom_shortcut_bindings.lock() {
        *guard = restored;
        clear_active_custom_shortcut_state(app_state);
    } else {
        log::error!("Failed to lock custom shortcut state while restoring bindings");
    }
}

fn shortcut_action_definitions() -> Vec<ShortcutActionDefinition> {
    vec![
        ShortcutActionDefinition {
            action: ShortcutAction::ToggleRecording,
            label: "Toggle recording",
            description: "Start or stop recording with one press.",
            section: "Recording",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::HoldToRecord,
            label: "Hold to record",
            description: "Record while the shortcut is held.",
            section: "Recording",
            recommended_trigger: ShortcutTrigger::Hold,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::CancelRecording,
            label: "Cancel recording",
            description: "Cancel the current recording.",
            section: "Recording",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::CopyLastTranscription,
            label: "Copy last transcription",
            description: "Copy the latest finished transcription to the clipboard.",
            section: "History",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::PasteLastTranscription,
            label: "Paste last transcription",
            description: "Paste the latest finished transcription into the active app.",
            section: "History",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::CycleFormattingMode,
            label: "Cycle formatting mode",
            description: "Switch to the next formatting mode.",
            section: "Formatting",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::SetPersonalDictation,
            label: "Personal dictation",
            description: "Switch formatting to Personal Dictation.",
            section: "Formatting",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::SetCleanDictation,
            label: "Clean dictation",
            description: "Switch formatting to Clean Dictation.",
            section: "Formatting",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::SetWriting,
            label: "Writing",
            description: "Switch formatting to Writing.",
            section: "Formatting",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::SetNotes,
            label: "Notes",
            description: "Switch formatting to Notes.",
            section: "Formatting",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::SetMessage,
            label: "Message",
            description: "Switch formatting to Message.",
            section: "Formatting",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::SetCode,
            label: "Code",
            description: "Switch formatting to Code.",
            section: "Formatting",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::ToggleAiFormatting,
            label: "Toggle AI formatting",
            description: "Turn AI formatting on or off.",
            section: "Formatting",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
        ShortcutActionDefinition {
            action: ShortcutAction::OpenDashboard,
            label: "Open dashboard",
            description: "Focus the VoiceTypr dashboard.",
            section: "Dashboard",
            recommended_trigger: ShortcutTrigger::Pressed,
            allows_single_key: true,
        },
    ]
}

fn current_formatting_preset(app: &AppHandle) -> Option<EnhancementPreset> {
    let store = app.store("settings").ok()?;
    let value = store.get("enhancement_options")?;
    let preset = value.get("preset")?.as_str()?;
    Some(match preset {
        "PersonalDictation" => EnhancementPreset::PersonalDictation,
        "CleanDictation" => EnhancementPreset::CleanDictation,
        "Writing" => EnhancementPreset::Writing,
        "Notes" => EnhancementPreset::Notes,
        "Message" => EnhancementPreset::Message,
        "Code" | "Coding" | "Prompts" | "Commit" => EnhancementPreset::Code,
        _ => EnhancementPreset::PersonalDictation,
    })
}
