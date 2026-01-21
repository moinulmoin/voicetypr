import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

interface AccessibilityPermissionOptions {
  // When false, skip the automatic check on mount and rely on explicit calls.
  checkOnMount?: boolean;
}

export function useAccessibilityPermission(options?: AccessibilityPermissionOptions) {
  const { checkOnMount = true } = options || {};
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

  // Optionally check permission on mount
  useEffect(() => {
    if (!checkOnMount) return;
    checkPermission();
  }, [checkPermission, checkOnMount]);

  // Poll for permission changes when permission is not granted
  // This catches when user grants permission manually in System Settings
  useEffect(() => {
    if (!checkOnMount) return;
    if (hasPermission === true) return; // Stop polling once granted

    const interval = setInterval(() => {
      checkPermission();
    }, 3000); // Check every 3 seconds

    return () => clearInterval(interval);
  }, [checkOnMount, hasPermission, checkPermission]);

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