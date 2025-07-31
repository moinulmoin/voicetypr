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

  // Check initial state on mount by requesting current state
  useEffect(() => {
    const checkInitialState = async () => {
      try {
        const currentState = await invoke<{ state: RecordingState; error: string | null }>('get_current_recording_state');
        console.log('[Recording Hook] Initial state:', currentState);
        setState(currentState.state);
        setError(currentState.error);
      } catch (err) {
        console.error('[Recording Hook] Failed to get initial state:', err);
      }
    };
    checkInitialState();
  }, []);

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

      unsubscribers.push(await listen('recording-stopped-silence', () => {
        console.log('[Recording Hook] Recording stopped due to silence');
        // You could show a toast or notification here
        // For now, just log it
      }));

      unsubscribers.push(await listen('transcription-started', () => {
        console.log('[Recording Hook] Transcription started');
        setState('transcribing');
      }));

      // NOTE: We don't listen to transcription-complete here anymore
      // The state will be set to idle by recording-state-changed event
      // after the pill finishes processing and calls transcription_processed

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