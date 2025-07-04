import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef, useState } from 'react';

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
  const transcriptionTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  // Effect to manage transcription timeout based on state
  useEffect(() => {
    if (state === 'transcribing') {
      // Clear any existing timeout
      if (transcriptionTimeoutRef.current) {
        clearTimeout(transcriptionTimeoutRef.current);
      }
      
      // Set new timeout
      transcriptionTimeoutRef.current = setTimeout(() => {
        console.warn('[Recording Hook] Transcription timeout - resetting to idle');
        setError('Transcription took too long');
        setState('idle');
      }, 30000);
    } else {
      // Clear timeout when not transcribing
      if (transcriptionTimeoutRef.current) {
        clearTimeout(transcriptionTimeoutRef.current);
        transcriptionTimeoutRef.current = null;
      }
    }
    
    // Cleanup on unmount or state change
    return () => {
      if (transcriptionTimeoutRef.current) {
        clearTimeout(transcriptionTimeoutRef.current);
        transcriptionTimeoutRef.current = null;
      }
    };
  }, [state]);

  useEffect(() => {
    const unsubscribers: Array<() => void> = [];

    const setupListeners = async () => {
      // Backend confirms recording started
      unsubscribers.push(await listen('recording-started', () => {
        console.log('[Recording Hook] Backend confirmed recording started');
        setState('recording');
        setError(null);
      }));

      // Recording timeout
      unsubscribers.push(await listen('recording-timeout', () => {
        console.log('[Recording Hook] Recording timeout');
        setState('stopping');
      }));

      // Transcription complete
      unsubscribers.push(await listen('transcription-complete', () => {
        console.log('[Recording Hook] Transcription complete');
        setState('idle');
        setError(null);
      }));

      // Transcription started
      unsubscribers.push(await listen('transcription-started', () => {
        console.log('[Recording Hook] Transcription started');
        setState('transcribing');
        // Timeout is now managed by the useEffect watching state changes
      }));

      // Transcription error
      unsubscribers.push(await listen<string>('transcription-error', (event) => {
        console.error('[Recording Hook] Transcription error:', event.payload);
        setError(event.payload);
        setState('error');
      }));

      // Recording error
      unsubscribers.push(await listen<string>('recording-error', (event) => {
        console.error('[Recording Hook] Recording error:', event.payload);
        setError(event.payload);
        setState('error');
      }));

      // Global hotkey events
      unsubscribers.push(await listen('start-recording', async () => {
        console.log('[Recording Hook] Hotkey start recording');
        await startRecording();
      }));

      unsubscribers.push(await listen('stop-recording', async () => {
        console.log('[Recording Hook] Hotkey stop recording');
        await stopRecording();
      }));
    };

    setupListeners();

    return () => {
      unsubscribers.forEach(unsub => unsub());
    };
  }, []);

  const startRecording = useCallback(async () => {
    if (state !== 'idle' && state !== 'error') {
      console.warn(`[Recording Hook] Cannot start recording - current state: ${state}`);
      return;
    }

    try {
      console.log('[Recording Hook] Starting recording...');
      setState('starting');
      await invoke('start_recording');
      // State will transition to 'recording' when backend confirms
    } catch (err) {
      console.error('[Recording Hook] Failed to start recording:', err);
      setError(String(err));
      setState('error');
    }
  }, [state]);

  const stopRecording = useCallback(async () => {
    // Allow stopping from 'starting' or 'recording' states
    if (state !== 'recording' && state !== 'starting') {
      console.warn(`[Recording Hook] Cannot stop recording - current state: ${state}`);
      return;
    }

    try {
      console.log('[Recording Hook] Stopping recording...');
      setState('stopping');
      await invoke('stop_recording');
      console.log('[Recording Hook] Backend acknowledged stop, waiting for transcription events');
    } catch (err) {
      console.error('[Recording Hook] Failed to stop recording:', err);
      const errorMessage = String(err);
      setError(errorMessage);
      setState('idle'); // Always return to idle on error

      // If no models downloaded, we should redirect to onboarding
      if (errorMessage.includes('No models downloaded')) {
        // Emit an event that App.tsx can listen to
        window.dispatchEvent(new CustomEvent('no-models-available'));
      }
    }
  }, [state]);

  return {
    state,
    error,
    startRecording,
    stopRecording,
    isActive: state !== 'idle' && state !== 'error'
  };
}