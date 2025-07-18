import { useEffect, useState, useRef } from 'react';
import { listen, TauriEvent } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';
import { 
  checkAccessibilityPermission, 
  requestAccessibilityPermission 
} from 'tauri-plugin-macos-permissions-api';
import { toast } from 'sonner';

export function useAccessibilityPermission() {
  const [hasPermission, setHasPermission] = useState<boolean | null>(null);
  const hasShownDialog = useRef(false);
  
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let unlistenFocus: (() => void) | undefined;

    const checkPermissions = async () => {
      try {
        const currentPermission = await checkAccessibilityPermission();
        
        // If permission changed from false to true, suggest restart
        if (hasPermission === false && currentPermission === true) {
          toast.success(
            'Accessibility permission granted! Please restart VoiceTypr to enable text insertion.',
            { duration: 8000 }
          );
        }
        
        setHasPermission(currentPermission);
        
        // Only show dialog once per session
        if (!currentPermission && !hasShownDialog.current) {
          hasShownDialog.current = true;
          
          const shouldOpenSettings = await ask(
            'VoiceTypr needs accessibility permission to insert text at your cursor position.\n\nWould you like to open System Settings to grant permission?',
            {
              title: 'Accessibility Permission Required',
              okLabel: 'Open Settings',
              cancelLabel: 'Later'
            }
          );
          
          if (shouldOpenSettings) {
            await requestAccessibilityPermission();
          }
        }
      } catch (error) {
        console.error('Failed to check accessibility permissions:', error);
      }
    };

    const setupListener = async () => {
      // Initial check
      checkPermissions();
      
      // Check when app starts
      unlisten = await listen('check-accessibility-permission', () => {
        checkPermissions();
      });
      
      // Check when window gains focus (like VoiceInk)
      unlistenFocus = await listen(TauriEvent.WINDOW_FOCUS, () => {
        checkPermissions();
      });
    };

    setupListener();

    return () => {
      if (unlisten) unlisten();
      if (unlistenFocus) unlistenFocus();
    };
  }, [hasPermission]);
  
  return { hasPermission };
}