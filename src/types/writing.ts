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

export const defaultWritingSettings: WritingSettings = {
  replacements: [],
  custom_words: [],
  snippets: [],
  voice_commands: [],
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
