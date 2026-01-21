import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { sendNotification, isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { AppErrorBoundary } from "./ErrorBoundary";
import { Sidebar } from "./Sidebar";
import { OnboardingDesktop } from "./onboarding/OnboardingDesktop";
import { SidebarInset, SidebarProvider } from "./ui/sidebar";
import { TabContainer } from "./tabs/TabContainer";
import { useReadiness } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import { useModelManagementContext } from "@/contexts/ModelManagementContext";
import { updateService } from "@/services/updateService";
import { loadApiKeysToCache } from "@/utils/keyring";

// Type for error event payloads from backend
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

export function AppContainer() {
  const { registerEvent } = useEventCoordinator("main");
  const [activeSection, setActiveSection] = useState<string>("overview");
  const [showOnboarding, setShowOnboarding] = useState(false);
  const { settings, refreshSettings } = useSettings();
  const { checkAccessibilityPermission, checkMicrophonePermission } = useReadiness();

  // Use the model management context for onboarding
  const modelManagement = useModelManagementContext();

  // Use a ref to track if we've just completed onboarding
  const hasJustCompletedOnboarding = useRef(false);

  // Initialize app
  useEffect(() => {
    const init = async () => {
      try {
        // Check if onboarding is completed - only check when settings are loaded
        if (settings && !settings.onboarding_completed) {
          setShowOnboarding(true);
        }

        // Run cleanup if enabled
        if (settings?.transcription_cleanup_days) {
          await invoke("cleanup_old_transcriptions", {
            days: settings.transcription_cleanup_days
          });
        }

        // Initialize update service for automatic update checks
        if (settings) {
          await updateService.initialize(settings);
        }

        // Load API keys from Stronghold to backend cache
        // Small delay to ensure Stronghold is ready
        setTimeout(() => {
          loadApiKeysToCache().catch((error) => {
            console.error("Failed to load API keys to cache:", error);
          });
        }, 100);

        // Listen for no-models event to redirect to onboarding
        const handleNoModels = () => {
          console.log("No models available - redirecting to onboarding");
          setShowOnboarding(true);
        };
        window.addEventListener("no-models-available", handleNoModels);

        // Listen for navigate-to-settings event from tray menu
        registerEvent("navigate-to-settings", () => {
          console.log("Navigate to settings requested from tray menu");
          setActiveSection("overview");
        });

        // Listen for manual update checks triggered from tray
        registerEvent("tray-check-updates", async () => {
          try {
            await updateService.checkForUpdatesManually();
          } catch (e) {
            console.error("Manual update check failed:", e);
            toast.error("Failed to check for updates");
          }
        });

        // Listen for tray action errors
        registerEvent("tray-action-error", (event) => {
          console.error("Tray action error:", event.payload);
          toast.error(event.payload as string);
        });

        registerEvent<string>("parakeet-unavailable", (message) => {
          const description = typeof message === "string" && message.trim().length > 0
            ? message
            : "Parakeet is unavailable on this Mac. Please reinstall VoiceTypr or remove the quarantine flag.";
          console.error("Parakeet unavailable:", description);
          toast.error("Parakeet Unavailable", {
            description,
            duration: 8000
          });
        });

        // Listen for remote VoiceTypr errors
        registerEvent<{ title: string; message: string }>("remote-server-error", async (data) => {
          console.error("Remote server error:", data);

          // Show toast with error info and clear action
          toast.error("Remote Server Unreachable", {
            description: "Go to History to re-transcribe this recording, or select a different model.",
            duration: 8000
          });

          // Also show system notification so user sees it even if app is not focused
          try {
            let permitted = await isPermissionGranted();
            if (!permitted) {
              const permission = await requestPermission();
              permitted = permission === "granted";
            }
            if (permitted) {
              sendNotification({
                title: "Remote Server Unreachable",
                body: "Go to History to re-transcribe, or select a different model."
              });
            }
          } catch (err) {
            console.error("Failed to send system notification:", err);
          }
        });

        // Listen for license-required event and navigate to License section
        registerEvent<{ title: string; message: string; action?: string }>(
          "license-required", 
          (data) => {
            console.log("License required event received in AppContainer:", data);
            // Navigate to License section to show license management
            setActiveSection("license");
            // Show a toast to inform the user
            toast.error(data.title || "License Required", {
              description: data.message || "Please purchase or restore a license to continue",
              duration: 5000
            });
          }
        );

        // Listen for no models error (when trying to record without any models)
        registerEvent<ErrorEventPayload>("no-models-error", (data) => {
          console.error("No models available:", data);
          toast.error(data.title || 'No Models Available', {
            description:
              data.message ||
              'Connect a cloud provider or download a local model in Models before recording.',
            duration: 8000
          });
        });

        return () => {
          window.removeEventListener("no-models-available", handleNoModels);
          updateService.dispose();
        };
      } catch (error) {
        console.error("Failed to initialize:", error);
      }
    };

    init();
  }, [registerEvent, settings]);

  // Mark when onboarding is being shown
  useEffect(() => {
    if (showOnboarding) {
      hasJustCompletedOnboarding.current = true;
    }
  }, [showOnboarding]);

  // Check permissions only when transitioning from onboarding to dashboard
  useEffect(() => {
    // Only refresh if we just completed onboarding and are now showing dashboard
    if (!showOnboarding && hasJustCompletedOnboarding.current && settings?.onboarding_completed) {
      hasJustCompletedOnboarding.current = false;

      Promise.all([checkAccessibilityPermission(), checkMicrophonePermission()]).then(() => {
        console.log("Permissions refreshed after onboarding completion");
      });

      // Request notification permission for update notifications
      updateService.requestNotificationPermission();
    }
  }, [
    showOnboarding,
    settings?.onboarding_completed,
    checkAccessibilityPermission,
    checkMicrophonePermission
  ]);

  // Onboarding View
  if (showOnboarding) {
    return (
      <AppErrorBoundary>
        <OnboardingDesktop
          onComplete={() => {
            setShowOnboarding(false);
            // Reload settings after onboarding
            refreshSettings();
          }}
          modelManagement={modelManagement}
        />
      </AppErrorBoundary>
    );
  }

  // Main App Layout
  return (
    <SidebarProvider>
      <Sidebar 
        activeSection={activeSection} 
        onSectionChange={setActiveSection} 
      />
      <SidebarInset>
        <TabContainer activeSection={activeSection} />
      </SidebarInset>
    </SidebarProvider>
  );
}
