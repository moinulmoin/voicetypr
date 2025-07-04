import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

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
      const result = await invoke<string>('stop_recording');
      console.log('[Recording Hook] Stop recording result:', result);
      
      if (!result || result === '') {
        // No recording was active or no audio to transcribe
        console.log('[Recording Hook] No audio to transcribe, resetting to idle');
        setState('idle');
        setError(null);
      } else {
        setState('transcribing');
        console.log('[Recording Hook] Transcription in progress...');
        
        // Timeout fallback: If transcription takes too long, reset
        setTimeout(() => {
          setState(prevState => {
            if (prevState === 'transcribing') {
              console.warn('[Recording Hook] Transcription timeout - resetting to idle');
              setError('Transcription took too long');
              return 'idle';
            }
            return prevState;
          });
        }, 15000); // 15 second timeout for transcription
      }
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