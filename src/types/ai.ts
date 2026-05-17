// AI Enhancement Types that match Rust structures

export type EnhancementPreset = 'Default' | 'Prompts' | 'Email' | 'Commit';

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

export const toBackendOptions = (options: {
  preset: EnhancementPreset;
}): EnhancementOptions => ({
  preset: options.preset,
});

export const fromBackendOptions = (options: EnhancementOptions): {
  preset: EnhancementPreset;
} => ({
  preset: options.preset,
});
