import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-shell';
import { toast } from 'sonner';

export type PermissionStatus = 'checking' | 'granted' | 'denied';

export interface PermissionState {
  microphone: PermissionStatus;
  accessibility: PermissionStatus;
}

export interface UsePermissionsReturn {
  permissions: PermissionState;
  checkPermissions: () => Promise<void>;
  requestPermission: (type: keyof PermissionState) => Promise<void>;
  isChecking: boolean;
  isRequesting: string | null;
  error: Error | null;
  allGranted: boolean;
}

/**
 * Comprehensive hook for managing all VoiceTypr permissions
 * Handles checking, requesting, and monitoring permission states
 */
export function usePermissions(options?: {
  checkOnMount?: boolean;
  checkInterval?: number;
  showToasts?: boolean;
}) {
  const { 
    checkOnMount = true, 
    checkInterval = 0,  // 0 means no interval
    showToasts = false 
  } = options || {};

  const [permissions, setPermissions] = useState<PermissionState>({
    microphone: 'checking',
    accessibility: 'checking',
  });

  const [isChecking, setIsChecking] = useState(false);
  const [isRequesting, setIsRequesting] = useState<string | null>(null);
  const [error, setError] = useState<Error | null>(null);
  
  const intervalRef = useRef<NodeJS.Timeout | null>(null);
  const timeoutRef = useRef<NodeJS.Timeout | null>(null);

  // Calculate if all permissions are granted
  const allGranted = 
    permissions.microphone === 'granted' && 
    permissions.accessibility === 'granted';

  const checkPermissions = async () => {
    setIsChecking(true);
    setError(null);
    
    try {
      // Check each permission separately to handle individual failures
      let mic = false;
      let accessibility = false;

      try {
        mic = await invoke<boolean>('check_microphone_permission');
      } catch (err) {
        console.error('Failed to check microphone permission:', err);
      }

      try {
        accessibility = await invoke<boolean>('check_accessibility_permission');
      } catch (err) {
        console.error('Failed to check accessibility permission:', err);
      }

      setPermissions({
        microphone: mic ? 'granted' : 'denied',
        accessibility: accessibility ? 'granted' : 'denied',
      });

      // Show success toast only if permissions changed from denied to granted
      if (showToasts && mic && accessibility) {
        const hadDeniedPermission = 
          permissions.microphone === 'denied' || 
          permissions.accessibility === 'denied';
        
        if (hadDeniedPermission) {
          toast.success('All permissions granted! You\'re ready to use VoiceTypr.');
        }
      }
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to check permissions');
      setError(error);
      console.error('Failed to check permissions:', error);
      
      if (showToasts) {
        toast.error('Failed to check permissions. Please try again.');
      }
    } finally {
      setIsChecking(false);
    }
  };

  const requestPermission = async (type: keyof PermissionState) => {
    setIsRequesting(type);
    setError(null);

    try {
      let granted = false;
      let settingsUrl = '';

      switch (type) {
        case 'microphone':
          granted = await invoke<boolean>('request_microphone_permission');
          settingsUrl = 'x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone';
          break;

        case 'accessibility':
          await invoke('request_accessibility_permission');
          // Accessibility permission request doesn't return a boolean
          settingsUrl = 'x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility';
          break;

      }

      // Open settings if permission wasn't granted (except for accessibility which always opens)
      if (!granted && type !== 'accessibility') {
        await open(settingsUrl);
        
        if (showToasts) {
          toast.info(`Please grant ${type} permission in System Settings`, {
            duration: 8000,
          });
        }
      } else if (type === 'accessibility') {
        // Always open settings for accessibility
        await open(settingsUrl);
        
        if (showToasts) {
          toast.info('Please grant accessibility permission in System Settings', {
            duration: 8000,
          });
        }
      }

      // Clear any existing timeout
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }

      // Re-check permissions after a delay
      timeoutRef.current = setTimeout(() => {
        checkPermissions();
      }, 1500);

    } catch (err) {
      const error = err instanceof Error ? err : new Error(`Failed to request ${type} permission`);
      setError(error);
      console.error(`Failed to request ${type} permission:`, error);
      
      if (showToasts) {
        toast.error(`Failed to request ${type} permission`);
      }
    } finally {
      setIsRequesting(null);
    }
  };

  useEffect(() => {
    // Check permissions on mount if enabled
    if (checkOnMount) {
      checkPermissions();
    }

    // Set up interval if specified
    if (checkInterval > 0) {
      intervalRef.current = setInterval(checkPermissions, checkInterval);
    }

    // Cleanup
    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
      }
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, []); // Empty deps, only run on mount

  return {
    permissions,
    checkPermissions,
    requestPermission,
    isChecking,
    isRequesting,
    error,
    allGranted,
  };
}
