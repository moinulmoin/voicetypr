import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { Toaster, toast } from "sonner";
import { AppErrorBoundary } from "./components/ErrorBoundary";
import { Sidebar } from "./components/Sidebar";
import { LicenseSection } from "./components/license";
import { OnboardingDesktop } from "./components/onboarding/OnboardingDesktop";
import { AboutSection } from "./components/sections/AboutSection";
import { GeneralSettings } from "./components/sections/GeneralSettings";
import { ModelsSection } from "./components/sections/ModelsSection";
import { RecentRecordings } from "./components/sections/RecentRecordings";
import { SidebarInset, SidebarProvider } from "./components/ui/sidebar";
import { LicenseProvider } from "./contexts/LicenseContext";
import { useAccessibilityPermission } from "./hooks/useAccessibilityPermission";
import { useEventCoordinator } from "./hooks/useEventCoordinator";
import { useModelManagement } from "./hooks/useModelManagement";
import { AppSettings, TranscriptionHistory } from "./types";

// Main App Component
export default function App() {
  const { registerEvent } = useEventCoordinator("main");
  const [activeSection, setActiveSection] = useState<string>("recordings");
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [history, setHistory] = useState<TranscriptionHistory[]>([]);
  
  // Use the new model management hook
  const {
    downloadProgress,
    downloadModel,
    cancelDownload,
    deleteModel,
    sortedModels
  } = useModelManagement({ windowId: "main", showToasts: true });

  // Check accessibility permissions on macOS
  useAccessibilityPermission();

  // Load history function
  const loadHistory = useCallback(async () => {
    try {
      const storedHistory = await invoke<any[]>("get_transcription_history", { limit: 50 });
      const formattedHistory: TranscriptionHistory[] = storedHistory.map((item) => ({
        id: item.timestamp || Date.now().toString(),
        text: item.text,
        timestamp: new Date(item.timestamp),
        model: item.model
      }));
      setHistory(formattedHistory);
    } catch (error) {
      console.error("Failed to load transcription history:", error);
    }
  }, []);

  // Initialize app
  useEffect(() => {
    const init = async () => {
      try {
        // Load settings
        const appSettings = await invoke<AppSettings>("get_settings");
        setSettings(appSettings);

        // Models are loaded automatically by useModelManagement hook

        // Check if onboarding is completed
        if (!appSettings.onboarding_completed) {
          setShowOnboarding(true);
        }

        // Run cleanup if enabled
        if (appSettings.transcription_cleanup_days) {
          await invoke("cleanup_old_transcriptions", {
            days: appSettings.transcription_cleanup_days
          });
        }

        // Load initial transcription history
        await loadHistory();

        // All recording event handling is now managed by the useRecording hook

        // Listen for no-models event to redirect to onboarding
        const handleNoModels = () => {
          console.log("No models available - redirecting to onboarding");
          setShowOnboarding(true);
        };
        window.addEventListener("no-models-available", handleNoModels);

        // Listen for history updates from backend
        // Backend is the single source of truth for transcription history
        registerEvent("history-updated", async () => {
          console.log("[EventCoordinator] Main window: reloading history after update");
          await loadHistory();
        });

        // Model download events are handled by useModelManagement hook

        // Listen for navigate-to-settings event from tray menu
        registerEvent("navigate-to-settings", () => {
          console.log("Navigate to settings requested from tray menu");
          setActiveSection("settings");
        });

        // Listen for license-required event
        registerEvent<{ title: string; message: string; action: string }>(
          "license-required",
          async (event) => {
            console.log("License required event:", event);

            // Small delay to prevent window animation conflicts
            await new Promise((resolve) => setTimeout(resolve, 100));

            // Focus main window and navigate to license section
            try {
              await invoke("focus_main_window");
              setActiveSection("license");

              // Show toast after window is focused to ensure it appears on top
              setTimeout(() => {
                toast.error(event.message, {
                  duration: 2000
                });
              }, 200);
            } catch (error) {
              console.error("Failed to focus window:", error);
              // If window focus fails, still show the toast
              toast.error(event.message);
            }
          }
        );

        return () => {
          window.removeEventListener("no-models-available", handleNoModels);
        };
      } catch (error) {
        console.error("Failed to initialize:", error);
      }
    };

    init();
  }, [registerEvent, loadHistory]);

  // Handle deleting a model with settings update
  const handleDeleteModel = useCallback(
    async (modelName: string) => {
      await deleteModel(modelName);
      
      // If deleted model was the current one, clear selection in settings
      if (settings?.current_model === modelName) {
        await saveSettings({ ...settings, current_model: "" });
      }
    },
    [deleteModel, settings]
  );

  // Save settings
  const saveSettings = useCallback(
    async (newSettings: AppSettings) => {
      try {
        await invoke("save_settings", { settings: newSettings });

        // Update global shortcut in backend if changed
        if (newSettings.hotkey !== settings?.hotkey) {
          try {
            await invoke("set_global_shortcut", { shortcut: newSettings.hotkey });
          } catch (err) {
            console.error("Failed to update hotkey:", err);
            // Still update UI even if hotkey registration fails
          }
        }

        setSettings(newSettings);
      } catch (error) {
        console.error("Failed to save settings:", error);
      }
    },
    [settings]
  );

  // sortedModels is provided by useModelManagement hook

  // Onboarding View
  if (showOnboarding) {
    return (
      <AppErrorBoundary>
        <OnboardingDesktop
          onComplete={() => {
            setShowOnboarding(false);
            // Reload settings after onboarding
            invoke<AppSettings>("get_settings").then(setSettings);
          }}
        />
      </AppErrorBoundary>
    );
  }

  // Render section content based on active section
  const renderSectionContent = () => {
    switch (activeSection) {
      case "recordings":
        return (
          <RecentRecordings
            history={history}
            hotkey={settings?.hotkey || "Cmd+Shift+Space"}
            onHistoryUpdate={loadHistory}
          />
        );

      case "general":
        return <GeneralSettings settings={settings} onSettingsChange={saveSettings} />;

      case "models":
        return (
          <ModelsSection
            models={sortedModels}
            downloadProgress={downloadProgress}
            currentModel={settings?.current_model}
            onDownload={downloadModel}
            onDelete={handleDeleteModel}
            onCancelDownload={cancelDownload}
            onSelect={async (modelName) => {
              if (settings) {
                await saveSettings({ ...settings, current_model: modelName });
              }
            }}
          />
        );

      case "about":
        return <AboutSection />;

      case "license":
        return <LicenseSection />;

      default:
        return (
          <RecentRecordings
            history={history}
            hotkey={settings?.hotkey || "Cmd+Shift+Space"}
            onHistoryUpdate={loadHistory}
          />
        );
    }
  };

  // Main App Layout
  return (
    <AppErrorBoundary>
      <LicenseProvider>
        <div className="h-screen flex flex-col">
          <SidebarProvider>
            <div className="flex-1 flex overflow-hidden">
              <Sidebar
                activeSection={activeSection}
                onSectionChange={setActiveSection}
              />
              <SidebarInset>
                <div className="h-full overflow-auto">{renderSectionContent()}</div>
              </SidebarInset>
            </div>
          </SidebarProvider>
        </div>
        <Toaster
          position="top-center"
          toastOptions={{
            classNames: {
              toast: "!w-fit"
            }
          }}
        />
      </LicenseProvider>
    </AppErrorBoundary>
  );
}
