import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';
import { open } from '@tauri-apps/plugin-shell';

export function useAccessibilityPermission() {
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const checkPermissions = async () => {
      try {
        // Check if we have accessibility permissions
        const hasPermission = await invoke<boolean>('check_accessibility_permission');
        
        if (!hasPermission) {
          // Show a dialog to inform the user
          const shouldOpenSettings = await ask(
            'VoiceTypr needs accessibility permission to insert text at your cursor position.\n\nWould you like to open System Settings to grant permission?',
            {
              title: 'Accessibility Permission Required',
              okLabel: 'Open Settings',
              cancelLabel: 'Later'
            }
          );
          
          if (shouldOpenSettings) {
            // Request permission (this will guide the user)
            await invoke('request_accessibility_permission');
            // On macOS, this will open the accessibility settings
            // Try the new format first, fallback to old format
            try {
              await open('x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility');
            } catch {
              // Fallback for newer macOS versions
              await open('x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility');
            }
          }
        }
      } catch (error) {
        console.error('Failed to check accessibility permissions:', error);
      }
    };

    // Set up listener for permission check event
    const setupListener = async () => {
      unlisten = await listen('check-accessibility-permission', () => {
        checkPermissions();
      });
    };

    setupListener();

    // Cleanup
    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, []);
}