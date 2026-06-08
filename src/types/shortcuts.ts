export type ShortcutAction =
  | "toggle_recording"
  | "hold_to_record"
  | "cancel_recording"
  | "copy_last_transcription"
  | "paste_last_transcription"
  | "cycle_formatting_mode"
  | "set_personal_dictation"
  | "set_clean_dictation"
  | "set_writing"
  | "set_notes"
  | "set_message"
  | "set_code"
  | "open_dashboard";

export type ShortcutTrigger = "pressed" | "hold";

export interface ShortcutBinding {
  id: string;
  action: ShortcutAction;
  shortcut: string;
  trigger: ShortcutTrigger;
  enabled: boolean;
  allow_risky_combo: boolean;
}

export interface ShortcutSettings {
  bindings: ShortcutBinding[];
}

export interface ShortcutActionDefinition {
  action: ShortcutAction;
  label: string;
  description: string;
  section: string;
  recommended_trigger: ShortcutTrigger;
  allows_single_key: boolean;
}
