import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface PermissionState {
  microphone: boolean;
  accessibility: boolean;
  automation: boolean;
  allGranted: boolean;
  isChecking: boolean;
}

/**
 * Hook to check all required permissions
 */
export function usePermissionCheck() {
  const [permissions, setPermissions] = useState<PermissionState>({
    microphone: false,
    accessibility: false,
    automation: false,
    allGranted: false,
    isChecking: true,
  });

  useEffect(() => {
    checkAllPermissions();
  }, []);

  const checkAllPermissions = async () => {
    try {
      const [mic, accessibility, automation] = await Promise.all([
        invoke<boolean>('check_microphone_permission'),
        invoke<boolean>('check_accessibility_permission'),
        invoke<boolean>('test_automation_permission'),
      ]);

      const allGranted = mic && accessibility && automation;

      setPermissions({
        microphone: mic,
        accessibility: accessibility,
        automation: automation,
        allGranted,
        isChecking: false,
      });
    } catch (error) {
      console.error('Failed to check permissions:', error);
      setPermissions(prev => ({ ...prev, isChecking: false }));
    }
  };

  return { ...permissions, recheckPermissions: checkAllPermissions };
}