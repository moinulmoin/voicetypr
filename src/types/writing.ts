import type { EnhancementPreset } from '@/types/ai'

export interface AppFormattingRule {
  app_name: string
  preset: EnhancementPreset
  enabled: boolean
}

export interface TextReplacementRule {
  from: string
  to: string
  language?: string | null
  enabled: boolean
}

export interface VoiceCommandRule {
  phrase: string
  output: string
  language?: string | null
  enabled: boolean
}

export interface CustomWord {
  phrase: string
  spoken_form?: string | null
  language?: string | null
  enabled: boolean
}

export interface Snippet {
  trigger: string
  body: string
  language?: string | null
  enabled: boolean
  preserve_literal: boolean
}

export interface WritingSettings {
  replacements: TextReplacementRule[]
  custom_words: CustomWord[]
  snippets: Snippet[]
  voice_commands: VoiceCommandRule[]
  app_formatting_rules: AppFormattingRule[]
}

// Built-in voice commands. MUST mirror the Rust `default_voice_commands()` in
// `src-tauri/src/writing.rs` so the TypeScript default agrees with the serde
// default Rust applies when `voice_commands` is absent. Rust treats an explicit
// `[]` as "built-ins disabled", so the TS default must never be `[]` for an
// unconfigured user — that would round-trip through `update_writing_settings`
// and persistently disable every built-in command (new line, insert period, …).
export const defaultVoiceCommands: VoiceCommandRule[] = [
  { phrase: 'new paragraph', output: 'paragraph', language: 'en', enabled: true },
  { phrase: 'new line', output: 'new_line', language: 'en', enabled: true },
  { phrase: 'question mark', output: 'question_mark', language: 'en', enabled: true },
  { phrase: 'exclamation point', output: 'exclamation_mark', language: 'en', enabled: true },
  { phrase: 'exclamation mark', output: 'exclamation_mark', language: 'en', enabled: true },
  { phrase: 'full stop', output: 'period', language: 'en', enabled: true },
  { phrase: 'insert comma', output: 'comma', language: 'en', enabled: true },
  { phrase: 'insert period', output: 'period', language: 'en', enabled: true },
]

export const defaultWritingSettings: WritingSettings = {
  replacements: [],
  custom_words: [],
  snippets: [],
  voice_commands: defaultVoiceCommands,
  app_formatting_rules: [],
}

// Only known fields are merged, so old persisted settings that still carry a
// removed `context_policy` field load without error and the stale field is
// dropped on the next save.
export const mergeWritingSettings = (
  partial: Partial<WritingSettings> | WritingSettings,
): WritingSettings => ({
  replacements: partial.replacements ?? defaultWritingSettings.replacements,
  custom_words: partial.custom_words ?? defaultWritingSettings.custom_words,
  snippets: partial.snippets ?? defaultWritingSettings.snippets,
  voice_commands: partial.voice_commands ?? defaultWritingSettings.voice_commands,
  app_formatting_rules:
    partial.app_formatting_rules ?? defaultWritingSettings.app_formatting_rules,
})
