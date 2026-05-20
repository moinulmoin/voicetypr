import { describe, expect, it } from 'vitest';

import { fromBackendOptions, migratePreset, type EnhancementPreset } from './ai';

const CURRENT_PRESETS: EnhancementPreset[] = ['Default', 'Writing', 'Notes', 'Message', 'Coding'];

describe('AI enhancement preset migration', () => {
  it('preserves current preset values', () => {
    for (const preset of CURRENT_PRESETS) {
      expect(migratePreset(preset)).toBe(preset);
    }
  });

  it('maps legacy presets to product profiles', () => {
    expect(migratePreset('Prompts')).toBe('Coding');
    expect(migratePreset('Email')).toBe('Writing');
    expect(migratePreset('Commit')).toBe('Coding');
  });

  it('falls back to Default for unknown presets', () => {
    expect(migratePreset('CustomLegacyPreset')).toBe('Default');
  });

  it('migrates backend options without changing current presets', () => {
    expect(
      fromBackendOptions({
        preset: 'Message',
      }),
    ).toEqual({
      preset: 'Message',
    });
  });
});
