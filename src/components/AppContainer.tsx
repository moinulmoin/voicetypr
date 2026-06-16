import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { sendNotification, isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { invoke } from "@tauri-apps/api/core";
import { AppErrorBoundary } from "./ErrorBoundary";
import { AppShell } from "./AppShell";
import type { ScreenId } from "./navigation";
import { OnboardingDesktop } from "./onboarding/OnboardingDesktop";
import { UpdateAnnouncementDialog } from "./UpdateAnnouncementDialog";
import { useReadiness } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import { useModelManagementContext } from "@/contexts/ModelManagementContext";
import { useModelAvailability } from "@/hooks/useModelAvailability";
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
  const [activeSection, setActiveSection] = useState<ScreenId>("overview");
  const [forceShowOnboarding, setForceShowOnboarding] = useState(false);
  const [justUpdatedVersion, setJustUpdatedVersion] = useState<string | null>(null);
  const { settings, refreshSettings } = useSettings();
  const { checkAccessibilityPermission, checkMicrophonePermission } = useReadiness();
  const modelAvailability = useModelAvailability();

  // Use the model management context for onboarding
  const modelManagement = useModelManagementContext();

  // Track explicit onboarding completion so recovery-driven onboarding doesn't trigger post-onboarding effects.
  const hasCompletedOnboarding = useRef(false);
  const previousHasModels = useRef<boolean | null>(modelAvailability.hasModels);
  const forceOnboardingNeedsFreshAvailability = useRef(false);

  // Initialize app
  useEffect(() => {
    let cancelled = false;

    const timeoutId = window.setTimeout(() => {
      if (cancelled) return;
      loadApiKeysToCache().catch((error) => {
        console.error("Failed to load API keys to cache:", error);
      });
    }, 100);

    const init = async () => {
      try {
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

        // Check if the app was just updated and show post-update dialog
        const updatedVersion = updateService.getJustUpdatedVersion?.();
        if (updatedVersion) {
          setJustUpdatedVersion(updatedVersion);
          try {
            await invoke("focus_main_window");
          } catch {
            // Window focus is best-effort; dialog still renders.
          }
        }
      } catch (error) {
        console.error("Failed to initialize:", error);
      }
    };

    void init();

    return () => {
      cancelled = true;
      clearTimeout(timeoutId);
      updateService.dispose();
    };
  }, [settings]);

  useEffect(() => {
    let isMounted = true;
    const unlisteners: Array<() => void> = [];

    const register = async <T,>(eventName: string, handler: (payload: T) => void | Promise<void>) => {
      const unlisten = await registerEvent<T>(eventName, handler);
      if (typeof unlisten !== "function") {
        return;
      }
      if (!isMounted) {
        unlisten();
        return;
      }
      unlisteners.push(unlisten);
    };

    const setup = async () => {
      try {
        await register("navigate-to-overview", () => {
          setActiveSection("overview");
        });

        await register("tray-check-updates", async () => {
          try {
            await updateService.checkForUpdatesManually();
          } catch (e) {
            console.error("Manual update check failed:", e);
            toast.error("Failed to check for updates");
          }
        });

        await register<string>("tray-action-error", (message) => {
          console.error("Tray action error:", message);
          toast.error(message);
        });

        await register<string>("parakeet-unavailable", (message) => {
          const description = typeof message === "string" && message.trim().length > 0
            ? message
            : "Parakeet is unavailable on this Mac. Please reinstall VoiceTypr or remove the quarantine flag.";
          console.error("Parakeet unavailable:", description);
          toast.error("Parakeet Unavailable", {
            description,
            duration: 8000
          });
        });

        const getRemoteServerErrorCopy = (data: {
          title?: string;
          message?: string;
          can_retry_from_history?: boolean;
        }) => {
          const title = data.title?.trim() || "Remote Server Unreachable";
          const message = data.message?.trim() || "The remote server could not complete this recording.";
          const historyGuidance = data.can_retry_from_history
            ? "Go to History to re-transcribe this recording, or select a different model."
            : "";

          return {
            title,
            message: historyGuidance ? `${message} ${historyGuidance}` : message
          };
        };

        await register<{
          title?: string;
          message?: string;
          can_retry_from_history?: boolean;
        }>("remote-server-error", async (data) => {
          console.error("Remote server error:", data);

          // Show toast with backend-owned error details and clear action
          const { title, message } = getRemoteServerErrorCopy(data);
          toast.error(title, {
            description: message,
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
                title,
                body: message
              });
            }
          } catch (err) {
            console.error("Failed to send system notification:", err);
          }
        });

        await register<{ title: string; message: string; action?: string }>("license-required", (data) => {
          console.log("License required event received in AppContainer:", data);
          // Navigate to License section to show license management
          setActiveSection("license");
          // Show a toast to inform the user
          toast.error(data.title || "License Required", {
            description: data.message || "Please purchase or restore a license to continue",
            duration: 5000
          });
        });

        await register<ErrorEventPayload>("no-models-error", async (data) => {
          console.error("No models available:", data);
          setForceShowOnboarding(true);
          forceOnboardingNeedsFreshAvailability.current = true;
          const refreshedAvailability = await modelAvailability.checkModels();
          if (refreshedAvailability.hasModels === true) {
            forceOnboardingNeedsFreshAvailability.current = false;
            setForceShowOnboarding(false);
          }
          toast.error(data.title || 'No Models Available', {
            description:
              data.suggestion ??
              data.message ??
              'Connect a cloud provider or download a local model in Models before recording.',
            duration: 8000
          });
        });
      } catch (error) {
        console.error("Failed to register app event listeners:", error);
      }
    };

    void setup();

    return () => {
      isMounted = false;
      unlisteners.forEach((unlisten) => {
        if (typeof unlisten === "function") {
          unlisten();
        }
      });
    };
  }, [registerEvent, modelAvailability.checkModels]);

  useEffect(() => {
    const previous = previousHasModels.current;
    previousHasModels.current = modelAvailability.hasModels;

    if (!(forceShowOnboarding && modelAvailability.hasModels === true)) {
      return;
    }

    if (forceOnboardingNeedsFreshAvailability.current && previous === true) {
      forceOnboardingNeedsFreshAvailability.current = false;
      return;
    }

    forceOnboardingNeedsFreshAvailability.current = false;
    const timeoutId = window.setTimeout(() => {
      setForceShowOnboarding(false);
    }, 0);

    return () => window.clearTimeout(timeoutId);
  }, [forceShowOnboarding, modelAvailability.hasModels]);

  const showOnboarding = Boolean(
    settings?.onboarding_completed === false ||
    forceShowOnboarding ||
    modelAvailability.hasModels === false
  );

  const markOnboardingCompletionPersisted = () => {
    hasCompletedOnboarding.current = true;
  };

  const clearOnboardingCompletionMarker = () => {
    hasCompletedOnboarding.current = false;
  };

  // Check permissions only after an explicit onboarding completion.
  useEffect(() => {
    if (!showOnboarding && hasCompletedOnboarding.current) {
      hasCompletedOnboarding.current = false;

      Promise.all([checkAccessibilityPermission(), checkMicrophonePermission()]).then(() => {
        console.log("Permissions refreshed after onboarding completion");
      });

      updateService.requestNotificationPermission();
    }
  }, [
    showOnboarding,
    checkAccessibilityPermission,
    checkMicrophonePermission,
  ]);

  // Onboarding View
  if (showOnboarding) {
    return (
      <AppErrorBoundary>
        <OnboardingDesktop
          onCompletionError={clearOnboardingCompletionMarker}
          onComplete={() => {
            markOnboardingCompletionPersisted();
            setForceShowOnboarding(false);
            refreshSettings();
            void modelAvailability.checkModels();
          }}
          modelManagement={modelManagement}
        />
      </AppErrorBoundary>
    );
  }

  // Main App Layout
  return (
    <>
      <AppShell
        activeSection={activeSection}
        onSectionChange={setActiveSection}
      />
      <UpdateAnnouncementDialog
        version={justUpdatedVersion}
        onClose={() => setJustUpdatedVersion(null)}
      />
    </>
  );
}
