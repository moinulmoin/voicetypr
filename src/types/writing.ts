export type WritingContextPolicy = 'off' | 'app_hint_only'

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
}

export const defaultWritingSettings: WritingSettings = {
  replacements: [],
  custom_words: [],
  snippets: [],
  context_policy: 'off',
}
