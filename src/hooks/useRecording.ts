import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useState } from 'react';

type RecordingState = 'idle' | 'starting' | 'recording' | 'stopping' | 'transcribing' | 'error';

interface UseRecordingReturn {
  state: RecordingState;
  error: string | null;
  startRecording: () => Promise<void>;
  stopRecording: () => Promise<void>;
  isActive: boolean;
}

export function useRecording(): UseRecordingReturn {
  const [state, setState] = useState<RecordingState>('idle');
  const [error, setError] = useState<string | null>(null);

  // Listen to backend events - frontend is purely reactive
  useEffect(() => {
    const unsubscribers: Array<() => void> = [];

    const setupListeners = async () => {
      // Backend state changes
      unsubscribers.push(await listen('recording-state-changed', (event: any) => {
        console.log('[Recording Hook] State changed:', event.payload);
        setState(event.payload.state);
        setError(event.payload.error || null);
      }));

      // Legacy events for compatibility
      unsubscribers.push(await listen('recording-started', () => {
        console.log('[Recording Hook] Recording started');
        setState('recording');
        setError(null);
      }));

      unsubscribers.push(await listen('recording-timeout', () => {
        console.log('[Recording Hook] Recording timeout');
        setState('stopping');
      }));

      unsubscribers.push(await listen('transcription-started', () => {
        console.log('[Recording Hook] Transcription started');
        setState('transcribing');
      }));

      unsubscribers.push(await listen('transcription-complete', () => {
        console.log('[Recording Hook] Transcription complete');
        setState('idle');
        setError(null);
      }));

      unsubscribers.push(await listen<string>('transcription-error', (event) => {
        console.error('[Recording Hook] Transcription error:', event.payload);
        setError(event.payload);
        setState('error');
      }));

      unsubscribers.push(await listen<string>('recording-error', (event) => {
        console.error('[Recording Hook] Recording error:', event.payload);
        setError(event.payload);
        setState('error');
      }));
    };

    setupListeners();

    return () => {
      unsubscribers.forEach(unsub => unsub());
    };
  }, []);


  // Simple command invocations - let backend handle all state management
  const startRecording = useCallback(async () => {
    try {
      console.log('[Recording Hook] Invoking start_recording...');
      await invoke('start_recording');
    } catch (err) {
      console.error('[Recording Hook] Failed to start recording:', err);
      // Backend will emit appropriate error events
    }
  }, []);

  const stopRecording = useCallback(async () => {
    try {
      console.log('[Recording Hook] Invoking stop_recording...');
      await invoke('stop_recording');
    } catch (err) {
      console.error('[Recording Hook] Failed to stop recording:', err);
      // Backend will emit appropriate error events
    }
  }, []);

  return {
    state,
    error,
    startRecording,
    stopRecording,
    isActive: state !== 'idle' && state !== 'error'
  };
}