import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useProviderModels, useAllProviderModels } from './useProviderModels';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockModels = [
  { id: 'gpt-5-nano', name: 'GPT-5 Nano', recommended: true },
  { id: 'gpt-5-mini', name: 'GPT-5 Mini', recommended: true },
];

describe('useProviderModels', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('starts with empty state', () => {
    const { result } = renderHook(() => useProviderModels('openai'));
    
    expect(result.current.models).toEqual([]);
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('fetches models successfully', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(mockModels);
    
    const { result } = renderHook(() => useProviderModels('openai'));
    
    await act(async () => {
      await result.current.fetchModels();
    });
    
    expect(invoke).toHaveBeenCalledWith('list_provider_models', { provider: 'openai' });
    expect(result.current.models).toEqual(mockModels);
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('handles fetch error', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('API key invalid'));
    
    const { result } = renderHook(() => useProviderModels('openai'));
    
    await act(async () => {
      await result.current.fetchModels();
    });
    
    expect(result.current.models).toEqual([]);
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBe('API key invalid');
  });

  it('sets loading state during fetch', async () => {
    let resolvePromise: (value: unknown) => void;
    const pendingPromise = new Promise((resolve) => {
      resolvePromise = resolve;
    });
    (invoke as ReturnType<typeof vi.fn>).mockReturnValue(pendingPromise);
    
    const { result } = renderHook(() => useProviderModels('openai'));
    
    // Start fetch (don't await)
    act(() => {
      result.current.fetchModels();
    });
    
    // Should be loading
    expect(result.current.loading).toBe(true);
    
    // Resolve and check loading is false
    await act(async () => {
      resolvePromise!(mockModels);
      await pendingPromise;
    });
    
    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });
  });

  it('clears models', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(mockModels);
    
    const { result } = renderHook(() => useProviderModels('openai'));
    
    await act(async () => {
      await result.current.fetchModels();
    });
    
    expect(result.current.models).toHaveLength(2);
    
    act(() => {
      result.current.clearModels();
    });
    
    expect(result.current.models).toEqual([]);
    expect(result.current.error).toBeNull();
  });

  it('does not auto-fetch on mount', () => {
    renderHook(() => useProviderModels('openai'));
    
    expect(invoke).not.toHaveBeenCalled();
  });

  it('skips fetch for custom provider', async () => {
    const { result } = renderHook(() => useProviderModels('custom'));
    
    await act(async () => {
      await result.current.fetchModels();
    });
    
    // Should not call invoke for custom provider
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe('useAllProviderModels', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('manages models for multiple providers', async () => {
    const geminiModels = [
      { id: 'gemini-2.5-flash', name: 'Gemini 2.5 Flash', recommended: true },
    ];
    
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((_cmd, args) => {
      if ((args as { provider: string })?.provider === 'openai') return Promise.resolve(mockModels);
      if ((args as { provider: string })?.provider === 'gemini') return Promise.resolve(geminiModels);
      return Promise.resolve([]);
    });
    
    const { result } = renderHook(() => useAllProviderModels());
    
    await act(async () => {
      await result.current.fetchModels('openai');
      await result.current.fetchModels('gemini');
    });
    
    expect(result.current.getModels('openai')).toEqual(mockModels);
    expect(result.current.getModels('gemini')).toEqual(geminiModels);
  });

  it('returns empty array for unknown provider', () => {
    const { result } = renderHook(() => useAllProviderModels());
    
    expect(result.current.getModels('unknown')).toEqual([]);
  });

  it('tracks loading state per provider', async () => {
    let resolveOpenAI: (value: unknown) => void;
    const openaiPromise = new Promise((resolve) => {
      resolveOpenAI = resolve;
    });
    
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((_cmd, args) => {
      if ((args as { provider: string })?.provider === 'openai') return openaiPromise;
      return Promise.resolve([]);
    });
    
    const { result } = renderHook(() => useAllProviderModels());
    
    act(() => {
      result.current.fetchModels('openai');
    });
    
    expect(result.current.isLoading('openai')).toBe(true);
    expect(result.current.isLoading('gemini')).toBe(false);
    
    await act(async () => {
      resolveOpenAI!(mockModels);
      await openaiPromise;
    });
    
    await waitFor(() => {
      expect(result.current.isLoading('openai')).toBe(false);
    });
  });

  it('tracks errors per provider', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((_cmd, args) => {
      if ((args as { provider: string })?.provider === 'openai') return Promise.reject(new Error('OpenAI error'));
      return Promise.resolve([]);
    });
    
    const { result } = renderHook(() => useAllProviderModels());
    
    await act(async () => {
      await result.current.fetchModels('openai');
    });
    
    expect(result.current.getError('openai')).toBe('OpenAI error');
    expect(result.current.getError('gemini')).toBeNull();
  });

  it('clears models for specific provider', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(mockModels);
    
    const { result } = renderHook(() => useAllProviderModels());
    
    await act(async () => {
      await result.current.fetchModels('openai');
    });
    
    expect(result.current.getModels('openai')).toHaveLength(2);
    
    act(() => {
      result.current.clearModels('openai');
    });
    
    expect(result.current.getModels('openai')).toEqual([]);
  });
});
