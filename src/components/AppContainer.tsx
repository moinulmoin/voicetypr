import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { Sparkles } from "lucide-react";
import { AppErrorBoundary } from "./ErrorBoundary";
import { Sidebar } from "./Sidebar";
import { OnboardingDesktop } from "./onboarding/OnboardingDesktop";
import { SidebarInset, SidebarProvider } from "./ui/sidebar";
import { TabContainer } from "./tabs/TabContainer";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { useReadiness } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import { useModelManagementContext } from "@/contexts/ModelManagementContext";
import { updateService } from "@/services/updateService";
const ACTIVE_SECTION_STORAGE_KEY = "voicetypr:activeSection";

const VALID_SECTIONS = new Set([
  "overview",
  "recordings",
  "audio",
  "general",
  "models",
  "formatting",
  "advanced",
  "license",
  "about",
  "help",
]);

function readStoredActiveSection(): string {
  try {
    const stored = sessionStorage.getItem(ACTIVE_SECTION_STORAGE_KEY);
    if (stored && VALID_SECTIONS.has(stored)) {
      return stored;
    }
  } catch {
    // sessionStorage may be unavailable
  }
  return "overview";
}

function isEditableContextTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }

  if (target.isContentEditable) {
    return true;
  }

  const editable = target.closest(
    'input, textarea, select, [contenteditable=""], [contenteditable="true"]'
  );
  if (!editable) {
    return false;
  }

  if (editable instanceof HTMLInputElement) {
    switch (editable.type) {
      case "button":
      case "checkbox":
      case "color":
      case "file":
      case "hidden":
      case "image":
      case "radio":
      case "range":
      case "reset":
      case "submit":
        return false;
      default:
        return true;
    }
  }

  return true;
}


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
  const [activeSection, setActiveSectionState] = useState(readStoredActiveSection);
  const setActiveSection = useCallback((section: string) => {
    setActiveSectionState(section);
    try {
      sessionStorage.setItem(ACTIVE_SECTION_STORAGE_KEY, section);
    } catch {
      // sessionStorage may be unavailable
    }
  }, []);
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [justUpdatedVersion, setJustUpdatedVersion] = useState<string | null>(null);
  const { settings, refreshSettings } = useSettings();
  const { checkAccessibilityPermission, checkMicrophonePermission } = useReadiness();

  // Use the model management context for onboarding
  const modelManagement = useModelManagementContext();

  // Use a ref to track if we've just completed onboarding
  const hasJustCompletedOnboarding = useRef(false);

  // Register event listeners once — settings changes must not re-register
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    let disposed = false;

    const trackEvent = (unlistenPromise: Promise<UnlistenFn>) => {
      void unlistenPromise
        .then((unlisten) => {
          if (disposed) {
            unlisten();
          } else {
            unlisteners.push(unlisten);
          }
        })
        .catch((error) => {
          console.error("Failed to register app event listener:", error);
        });
    };

    // Listen for no-models event to redirect to onboarding
    const handleNoModels = () => {
      console.log("No models available - redirecting to onboarding");
      setShowOnboarding(true);
    };
    window.addEventListener("no-models-available", handleNoModels);

    // Listen for navigate-to-settings event from tray menu
    trackEvent(registerEvent("navigate-to-settings", () => {
      console.log("Navigate to settings requested from tray menu");
      setActiveSection("general");
    }));

    // Listen for manual update checks triggered from tray
    trackEvent(registerEvent("tray-check-updates", async () => {
      try {
        await updateService.checkForUpdatesManually();
      } catch (e) {
        console.error("Manual update check failed:", e);
        toast.error("Failed to check for updates");
      }
    }));

    // Listen for tray action errors
    trackEvent(registerEvent("tray-action-error", (event) => {
      console.error("Tray action error:", event.payload);
      toast.error(event.payload as string);
    }));

    trackEvent(registerEvent<string>("parakeet-unavailable", (message) => {
      const description = typeof message === "string" && message.trim().length > 0
        ? message
        : "Parakeet is unavailable on this Mac. Please reinstall VoiceTypr or remove the quarantine flag.";
      console.error("Parakeet unavailable:", description);
      toast.error("Parakeet Unavailable", {
        description,
        duration: 8000
      });
    }));

    // Listen for license-required event and navigate to License section
    trackEvent(registerEvent<{ title: string; message: string; action?: string }>(
      "license-required", 
      (data) => {
        console.log("License required event received in AppContainer:", data);
        setActiveSection("license");
        toast.error(data.title || "License Required", {
          description: data.message || "Please purchase or restore a license to continue",
          duration: 5000
        });
      }
    ));

    // Listen for no models error (when trying to record without any models)
    trackEvent(registerEvent<ErrorEventPayload>("no-models-error", (data) => {
      console.error("No models available:", data);
      toast.error(data.title || 'No Models Available', {
        description:
          data.message ||
          'Connect a cloud provider or download a local model in Models before recording.',
        duration: 8000
      });
    }));

    return () => {
      disposed = true;
      window.removeEventListener("no-models-available", handleNoModels);
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, [registerEvent, setActiveSection]);

  // Settings-dependent initialization (onboarding check, cleanup, update service)
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

        // Check if the app was just updated and show post-update dialog
        const updatedVersion = updateService.getJustUpdatedVersion();
        if (updatedVersion) {
          setJustUpdatedVersion(updatedVersion);
          try {
            await invoke('focus_main_window');
          } catch {
            // Window focus is best-effort; dialog still renders
          }
        }
      } catch (error) {
        console.error("Failed to initialize:", error);
      }
    };

    init();

    return () => {
      updateService.dispose();
    };
  }, [settings]);

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
    <div
      className="h-full w-full"
      onContextMenu={(event) => {
        if (!isEditableContextTarget(event.target)) {
          event.preventDefault();
        }
      }}
    >
      <SidebarProvider>
        <Sidebar
          activeSection={activeSection}
          onSectionChange={setActiveSection}
        />
        <SidebarInset>
          <TabContainer activeSection={activeSection} />
        </SidebarInset>

        {/* Post-update notification dialog */}
        <Dialog
          open={!!justUpdatedVersion}
          onOpenChange={(open) => { if (!open) setJustUpdatedVersion(null); }}
        >
          <DialogContent className="sm:max-w-md">
            <DialogHeader>
              <DialogTitle className="flex items-center gap-2">
                <Sparkles className="h-5 w-5 text-primary" />
                VoiceTypr Updated
              </DialogTitle>
              <DialogDescription>
                Successfully updated to version {justUpdatedVersion}
              </DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button onClick={() => setJustUpdatedVersion(null)}>
                Dismiss
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </SidebarProvider>
    </div>
  );
}