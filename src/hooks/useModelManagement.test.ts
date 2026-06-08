import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, it, expect, vi } from 'vitest';
import { sortModels, useModelManagement } from './useModelManagement';
import { LocalModelInfo } from '@/types';

const mockTauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  ask: vi.fn(),
  handlers: {} as Record<string, (payload: unknown) => void>,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockTauri.invoke,
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: mockTauri.ask,
}));

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    info: vi.fn(),
    success: vi.fn(),
  },
}));

vi.mock('./useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn((event: string, callback: (payload: unknown) => void) => {
      mockTauri.handlers[event] = callback;
      return Promise.resolve(vi.fn());
    }),
  }),
}));


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

beforeEach(() => {
  vi.clearAllMocks();
  for (const key of Object.keys(mockTauri.handlers)) {
    delete mockTauri.handlers[key];
  }
  mockTauri.ask.mockResolvedValue(true);
  mockTauri.invoke.mockImplementation((command: string) => {
    if (command === 'get_model_status') {
      return Promise.resolve({
        models: [whisperModel('base.en', 5, 8, 147_964_211)],
      });
    }
    return Promise.resolve();
  });
});


describe('sortModels', () => {
  it('orders whisper models by accuracy without medium', () => {
    const entries: [string, LocalModelInfo][] = [
      ['large-v3-q5_0', whisperModel('large-v3-q5_0', 8, 4, 1_081_140_203)],
      ['small.en', whisperModel('small.en', 6, 7, 487_614_201)],
      ['base.en', whisperModel('base.en', 5, 8, 147_964_211)],
    ];

    const sorted = sortModels(entries, 'accuracy');

    expect(sorted.map(([name]) => name)).toEqual([
      'base.en',
      'small.en',
      'large-v3-q5_0',
    ]);
  });
});

describe('useModelManagement terminal download events', () => {
  it('clears verifying state when post-download verification fails', async () => {
    const { result } = renderHook(() => useModelManagement({ showToasts: false }));

    await waitFor(() => {
      expect(mockTauri.handlers['model-verifying']).toEqual(expect.any(Function));
      expect(mockTauri.handlers['download-error']).toEqual(expect.any(Function));
    });

    act(() => {
      mockTauri.handlers['model-verifying']({ model: 'base.en' });
    });

    await waitFor(() => {
      expect(result.current.verifyingModels.has('base.en')).toBe(true);
    });

    act(() => {
      mockTauri.handlers['download-error']({
        model: 'base.en',
        error: 'verification_failed',
      });
    });

    await waitFor(() => {
      expect(result.current.verifyingModels.has('base.en')).toBe(false);
    });
    expect(result.current.downloadProgress['base.en']).toBeUndefined();
    expect(result.current.downloadErrors['base.en']).toBe('verification_failed');
  });

  it('keeps detailed event error when invoke later rejects generically', async () => {
    let rejectDownload!: (error: string) => void;
    mockTauri.invoke.mockImplementation((command: string) => {
      if (command === 'get_model_status') {
        return Promise.resolve({
          models: [whisperModel('base.en', 5, 8, 147_964_211)],
        });
      }
      if (command === 'download_model') {
        return new Promise((_, reject) => {
          rejectDownload = reject;
        });
      }
      return Promise.resolve();
    });

    const { result } = renderHook(() => useModelManagement({ showToasts: false }));

    await waitFor(() => {
      expect(result.current.models['base.en']).toBeDefined();
      expect(mockTauri.handlers['download-error']).toEqual(expect.any(Function));
    });

    act(() => {
      result.current.downloadModel('base.en');
    });

    act(() => {
      mockTauri.handlers['download-error']({
        model: 'base.en',
        error: 'Checksum verification failed: expected abc, calculated def',
      });
    });

    await waitFor(() => {
      expect(result.current.downloadErrors['base.en']).toBe('Checksum verification failed: expected abc, calculated def');
    });

    await act(async () => {
      rejectDownload('verification_failed');
      await Promise.resolve();
    });

    expect(result.current.downloadErrors['base.en']).toBe('Checksum verification failed: expected abc, calculated def');
  });
});
