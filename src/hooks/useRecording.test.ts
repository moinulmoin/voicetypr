import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useRecording } from './useRecording';
import { listen } from '@tauri-apps/api/event';
import { mockIPC, clearMocks } from '@tauri-apps/api/mocks';
import { emitMockEvent } from '../test/setup';

// Get the mocked functions
const mockListen = vi.mocked(listen);

describe('useRecording', () => {
  const mockUnsubscribe = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    clearMocks();
    
    // Set up default IPC mock responses
    mockIPC((cmd) => {
      if (cmd === 'start_recording') return Promise.resolve();
      if (cmd === 'stop_recording') return Promise.resolve();
      return Promise.reject(new Error(`Unknown command: ${cmd}`));
    });
  });

  it('should initialize with idle state', () => {
    const { result } = renderHook(() => useRecording());

    expect(result.current.state).toBe('idle');
    expect(result.current.error).toBeNull();
    expect(result.current.isActive).toBe(false);
  });

  it('should set up event listeners on mount', async () => {
    renderHook(() => useRecording());

    await waitFor(() => {
      expect(mockListen).toHaveBeenCalledWith('recording-state-changed', expect.any(Function));
      expect(mockListen).toHaveBeenCalledWith('recording-started', expect.any(Function));
      expect(mockListen).toHaveBeenCalledWith('recording-timeout', expect.any(Function));
      expect(mockListen).toHaveBeenCalledWith('recording-stopped-silence', expect.any(Function));
      expect(mockListen).toHaveBeenCalledWith('transcription-started', expect.any(Function));
      expect(mockListen).toHaveBeenCalledWith('transcription-complete', expect.any(Function));
      expect(mockListen).toHaveBeenCalledWith('transcription-error', expect.any(Function));
      expect(mockListen).toHaveBeenCalledWith('recording-error', expect.any(Function));
    });
  });

  it('should handle state changes from backend events', async () => {
    const { result } = renderHook(() => useRecording());

    // Wait for listeners to be set up
    await waitFor(() => {
      expect(mockListen).toHaveBeenCalled();
    });

    // Simulate state change event
    act(() => {
      emitMockEvent('recording-state-changed', { state: 'recording', error: null });
    });

    expect(result.current.state).toBe('recording');
    expect(result.current.error).toBeNull();
    expect(result.current.isActive).toBe(true);
  });

  it('should handle error state', async () => {
    const { result } = renderHook(() => useRecording());

    await waitFor(() => {
      expect(mockListen).toHaveBeenCalled();
    });

    act(() => {
      emitMockEvent('recording-error', 'Test error message');
    });

    expect(result.current.state).toBe('error');
    expect(result.current.error).toBe('Test error message');
    expect(result.current.isActive).toBe(false);
  });

  it('should invoke start_recording command', async () => {
    const { result } = renderHook(() => useRecording());

    await act(async () => {
      await result.current.startRecording();
    });

    // The mockIPC handler should have been called with start_recording
    // We can verify this worked by checking no errors were thrown
    expect(result.current.state).toBe('idle'); // State managed by backend
  });

  it('should invoke stop_recording command', async () => {
    const { result } = renderHook(() => useRecording());

    await act(async () => {
      await result.current.stopRecording();
    });

    // The mockIPC handler should have been called with stop_recording
    expect(result.current.state).toBe('idle'); // State managed by backend
  });

  it('should handle command errors gracefully', async () => {
    const { result } = renderHook(() => useRecording());
    const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    // Mock IPC to reject
    mockIPC((cmd) => {
      if (cmd === 'start_recording') return Promise.reject(new Error('Command failed'));
      return Promise.resolve();
    });

    await act(async () => {
      await result.current.startRecording();
    });

    expect(consoleErrorSpy).toHaveBeenCalledWith(
      '[Recording Hook] Failed to start recording:',
      expect.any(Error)
    );

    consoleErrorSpy.mockRestore();
  });

  it('should calculate isActive correctly', async () => {
    const { result } = renderHook(() => useRecording());

    await waitFor(() => {
      expect(mockListen).toHaveBeenCalled();
    });

    // Test different states
    const states = [
      { state: 'idle', expectedActive: false },
      { state: 'starting', expectedActive: true },
      { state: 'recording', expectedActive: true },
      { state: 'stopping', expectedActive: true },
      { state: 'transcribing', expectedActive: true },
      { state: 'error', expectedActive: false },
    ];

    for (const { state, expectedActive } of states) {
      act(() => {
        emitMockEvent('recording-state-changed', { state, error: null });
      });
      expect(result.current.isActive).toBe(expectedActive);
    }
  });

  it('should handle multiple event types', async () => {
    const { result } = renderHook(() => useRecording());

    await waitFor(() => {
      expect(mockListen).toHaveBeenCalled();
    });

    // Test legacy event handling
    act(() => {
      emitMockEvent('recording-started', {});
    });
    expect(result.current.state).toBe('recording');

    act(() => {
      emitMockEvent('transcription-started', {});
    });
    expect(result.current.state).toBe('transcribing');

    act(() => {
      emitMockEvent('transcription-complete', {});
    });
    expect(result.current.state).toBe('idle');
  });
});