import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export function useMicrophonePermission() {
  const [hasPermission, setHasPermission] = useState<boolean | null>(null);
  const [isChecking, setIsChecking] = useState(false);

  const checkPermission = useCallback(async () => {
    setIsChecking(true);
    try {
      const result = await invoke<boolean>('check_microphone_permission');
      setHasPermission(result);
      return result;
    } catch (error) {
      console.error('Failed to check microphone permission:', error);
      setHasPermission(false);
      return false;
    } finally {
      setIsChecking(false);
    }
  }, []);

  const requestPermission = useCallback(async () => {
    try {
      const result = await invoke<boolean>('request_microphone_permission');
      setHasPermission(result);
      return result;
    } catch (error) {
      console.error('Failed to request microphone permission:', error);
      return false;
    }
  }, []);

  // Check permission on mount
  useEffect(() => {
    checkPermission();
  }, [checkPermission]);

  // Listen for permission changes
  useEffect(() => {
    const unlistenGranted = listen('microphone-granted', () => {
      console.log('[useMicrophonePermission] Permission granted event received');
      setHasPermission(true);
    });

    const unlistenDenied = listen('microphone-denied', () => {
      console.log('[useMicrophonePermission] Permission denied event received');
      setHasPermission(false);
    });

    return () => {
      Promise.all([unlistenGranted, unlistenDenied]).then(unsubs => {
        unsubs.forEach(unsub => unsub());
      });
    };
  }, []);

  return {
    hasPermission,
    isChecking,
    checkPermission,
    requestPermission
  };
}