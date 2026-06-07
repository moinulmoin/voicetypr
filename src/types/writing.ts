import type { EnhancementPreset } from '@/types/ai'

export type WritingContextPolicy = 'off' | 'app_hint_only'

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
  context_policy: WritingContextPolicy
  app_formatting_rules: AppFormattingRule[]
}

export const defaultWritingSettings: WritingSettings = {
  replacements: [],
  custom_words: [],
  snippets: [],
  context_policy: 'off',
  app_formatting_rules: [],
}

export const mergeWritingSettings = (
  partial: Partial<WritingSettings> | WritingSettings,
): WritingSettings => ({
  ...defaultWritingSettings,
  ...partial,
  replacements: partial.replacements ?? defaultWritingSettings.replacements,
  custom_words: partial.custom_words ?? defaultWritingSettings.custom_words,
  snippets: partial.snippets ?? defaultWritingSettings.snippets,
  context_policy: partial.context_policy ?? defaultWritingSettings.context_policy,
  app_formatting_rules:
    partial.app_formatting_rules ?? defaultWritingSettings.app_formatting_rules,
})
