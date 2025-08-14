// AI Enhancement Types that match Rust structures

export type EnhancementPreset = 'Default' | 'Prompts' | 'Email' | 'Commit';

export interface EnhancementOptions {
  preset: EnhancementPreset;
  custom_vocabulary: string[];
}

export interface AISettings {
  enabled: boolean;
  provider: string;
  model: string;
  hasApiKey: boolean;
  enhancement_options?: EnhancementOptions;
}

export interface AIModel {
  id: string;
  name: string;
  provider: string;
  description?: string;
}

// Helper to convert between frontend camelCase and backend snake_case
export const toBackendOptions = (options: {
  preset: EnhancementPreset;
  customVocabulary: string[];
}): EnhancementOptions => ({
  preset: options.preset,
  custom_vocabulary: options.customVocabulary,
});

export const fromBackendOptions = (options: EnhancementOptions): {
  preset: EnhancementPreset;
  customVocabulary: string[];
} => ({
  preset: options.preset,
  customVocabulary: options.custom_vocabulary,
});