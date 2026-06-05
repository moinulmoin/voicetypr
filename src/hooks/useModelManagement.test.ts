import { describe, it, expect } from 'vitest';
import { sortModels } from './useModelManagement';
import { LocalModelInfo } from '@/types';

function whisperModel(
  name: string,
  accuracy_score: number,
  speed_score: number,
  size: number
): LocalModelInfo {
  return {
    name,
    display_name: name,
    engine: 'whisper',
    kind: 'local',
    recommended: false,
    downloaded: false,
    requires_setup: false,
    size,
    url: `https://example.com/${name}.bin`,
    sha256: `${name}-sha256`,
    speed_score,
    accuracy_score,
  };
}

describe('sortModels', () => {
  it('orders whisper models by accuracy with medium after small', () => {
    const entries: [string, LocalModelInfo][] = [
      ['large-v3-q5_0', whisperModel('large-v3-q5_0', 8, 4, 1_081_140_203)],
      ['medium', whisperModel('medium', 7, 5, 1_533_763_059)],
      ['small.en', whisperModel('small.en', 6, 7, 487_614_201)],
      ['base.en', whisperModel('base.en', 5, 8, 147_964_211)],
    ];

    const sorted = sortModels(entries, 'accuracy');

    expect(sorted.map(([name]) => name)).toEqual([
      'base.en',
      'small.en',
      'medium',
      'large-v3-q5_0',
    ]);
  });
});
