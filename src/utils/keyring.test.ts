import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { loadApiKeysToCache, removeApiKey } from './keyring';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(),
}));

describe('keyring provider isolation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('clears custom cache using custom provider key', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

    await removeApiKey('custom');

    expect(invoke).toHaveBeenNthCalledWith(1, 'keyring_delete', { key: 'ai_api_key_custom' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'clear_ai_api_key_cache', { provider: 'custom' });
    expect(emit).toHaveBeenCalledWith('api-key-removed', { provider: 'custom' });
  });

  it('loads custom and openai keys into separate backend cache providers', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'keyring_get') {
        const key = (args as { key: string }).key;
        if (key === 'ai_api_key_openai') return Promise.resolve('openai-key');
        if (key === 'ai_api_key_custom') return Promise.resolve('custom-key');
        return Promise.resolve(null);
      }
      return Promise.resolve(undefined);
    });

    await loadApiKeysToCache();

    expect(invoke).toHaveBeenCalledWith('cache_ai_api_key', {
      args: { provider: 'openai', apiKey: 'openai-key' },
    });
    expect(invoke).toHaveBeenCalledWith('cache_ai_api_key', {
      args: { provider: 'custom', apiKey: 'custom-key' },
    });
    expect(invoke).not.toHaveBeenCalledWith('cache_ai_api_key', {
      args: { provider: 'openai', apiKey: 'custom-key' },
    });
  });
});
