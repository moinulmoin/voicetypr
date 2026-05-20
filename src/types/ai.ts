// AI Enhancement Types that match Rust structures

export type EnhancementPreset = 'Default' | 'Writing' | 'Notes' | 'Message' | 'Coding';

/** Mapping from current and legacy backend preset names to current frontend names. */
const PRESET_MIGRATION: Record<string, EnhancementPreset> = {
  Default: 'Default',
  Writing: 'Writing',
  Notes: 'Notes',
  Message: 'Message',
  Coding: 'Coding',
  Prompts: 'Coding',
  Email: 'Writing',
  Commit: 'Coding',
};

/** Migrate a backend preset value (possibly legacy) to the current type. */
export const migratePreset = (raw: string): EnhancementPreset =>
  PRESET_MIGRATION[raw] ?? 'Default';

export interface EnhancementOptions {
  preset: EnhancementPreset;
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
}): EnhancementOptions => ({
  preset: options.preset,
});

export const fromBackendOptions = (options: EnhancementOptions): {
  preset: EnhancementPreset;
} => ({
  preset: migratePreset(options.preset as string),
});
