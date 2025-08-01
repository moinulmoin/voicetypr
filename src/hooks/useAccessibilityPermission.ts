import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export function useAccessibilityPermission() {
  const [hasPermission, setHasPermission] = useState<boolean | null>(null);
  const [isChecking, setIsChecking] = useState(false);

  const checkPermission = useCallback(async () => {
    setIsChecking(true);
    try {
      const result = await invoke<boolean>('check_accessibility_permission');
      setHasPermission(result);
      return result;
    } catch (error) {
      console.error('Failed to check accessibility permission:', error);
      setHasPermission(false);
      return false;
    } finally {
      setIsChecking(false);
    }
  }, []);

  const requestPermission = useCallback(async () => {
    try {
      const result = await invoke<boolean>('request_accessibility_permission');
      setHasPermission(result);
      return result;
    } catch (error) {
      console.error('Failed to request accessibility permission:', error);
      return false;
    }
  }, []);

  // Check permission on mount
  useEffect(() => {
    checkPermission();
  }, [checkPermission]);

  // Listen for permission changes
  useEffect(() => {
    const unlistenGranted = listen('accessibility-granted', () => {
      console.log('[useAccessibilityPermission] Permission granted event received');
      setHasPermission(true);
    });

    const unlistenDenied = listen('accessibility-denied', () => {
      console.log('[useAccessibilityPermission] Permission denied event received');
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