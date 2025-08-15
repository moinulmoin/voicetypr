import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";
import { toast } from "sonner";
import { GeneralSettings } from "../sections/GeneralSettings";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";

interface ErrorEventPayload {
  title?: string;
  message: string;
  severity?: 'info' | 'warning' | 'error';
  actions?: string[];
  details?: string;
  hotkey?: string;
  error?: string;
  suggestion?: string;
}

export function SettingsTab() {
  const { registerEvent } = useEventCoordinator("main");


  // Initialize settings tab
  useEffect(() => {
    const init = async () => {
      try {
        // Listen for hotkey registration failures
        registerEvent<ErrorEventPayload>("hotkey-registration-failed", (data) => {
          console.error("Hotkey registration failed:", data);
          
          toast.error('Hotkey Registration Failed', {
            description: data.suggestion || 'The hotkey is in use by another application',
            action: {
              label: 'Change Hotkey',
              onClick: () => {
                // Show additional guidance
                setTimeout(() => {
                  toast.info('Hotkey Conflict', {
                    description: `The hotkey "${data.hotkey}" could not be registered. Please choose a different combination in General Settings.`,
                    duration: 6000
                  });
                }, 500);
              }
            },
            duration: 10000 // Persistent for important errors
          });
        });

        // Listen for no speech detected events with settings action
        registerEvent<ErrorEventPayload>("no-speech-detected", (data) => {
          console.warn("No speech detected:", data);
          
          // Determine toast type based on severity
          const toastFn = data.severity === 'error' ? toast.error : toast.warning;
          
          toastFn(data.title || 'No Speech Detected', {
            description: data.message || 'Please check your microphone and speak clearly',
            action: {
              label: data.actions?.includes('settings') ? 'Open Settings' : 'Try Again',
              onClick: () => {
                if (data.actions?.includes('settings')) {
                  // Already on settings tab, scroll to relevant section or show guidance
                  toast.info('Audio Settings', {
                    description: 'Check your microphone input level and ensure you have the correct input device selected.',
                    duration: 5000
                  });
                } else {
                  // Trigger recording again
                  invoke('start_recording').catch(console.error);
                }
              }
            },
            duration: data.severity === 'error' ? 8000 : 5000
          });
        });
      } catch (error) {
        console.error("Failed to initialize settings tab:", error);
      }
    };

    init();
  }, [registerEvent]);

  return <GeneralSettings />;
}