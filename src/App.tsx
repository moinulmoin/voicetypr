import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { Toaster, toast } from "sonner";
import { AppErrorBoundary } from "./components/ErrorBoundary";
import { Sidebar } from "./components/Sidebar";
import { OnboardingDesktop } from "./components/onboarding/OnboardingDesktop";
import { AccountSection } from "./components/sections/AccountSection";
import { AdvancedSection } from "./components/sections/AdvancedSection";
import { EnhancementsSection } from "./components/sections/EnhancementsSection";
import { GeneralSettings } from "./components/sections/GeneralSettings";
import { ModelsSection } from "./components/sections/ModelsSection";
import { RecentRecordings } from "./components/sections/RecentRecordings";
import { SidebarInset, SidebarProvider } from "./components/ui/sidebar";
import { LicenseProvider } from "./contexts/LicenseContext";
import { ReadinessProvider } from "./contexts/ReadinessContext";
import { SettingsProvider, useSettings } from "./contexts/SettingsContext";
import { useEventCoordinator } from "./hooks/useEventCoordinator";
import { useModelManagement } from "./hooks/useModelManagement";
import { updateService } from "./services/updateService";
import { AppSettings, TranscriptionHistory } from "./types";
import { loadApiKeysToCache } from "./utils/keyring";

// Main App Component
function AppContent() {
  const { registerEvent } = useEventCoordinator("main");
  const [activeSection, setActiveSection] = useState<string>("recordings");
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [history, setHistory] = useState<TranscriptionHistory[]>([]);
  const { settings, refreshSettings, updateSettings } = useSettings();

  // Use the new model management hook
  const modelManagement = useModelManagement({
    windowId: "main",
    showToasts: true,
  });
  const {
    downloadProgress,
    verifyingModels,
    downloadModel,
    cancelDownload,
    deleteModel,
    sortedModels,
  } = modelManagement;

  // Load history function
  const loadHistory = useCallback(async () => {
    try {
      const storedHistory = await invoke<any[]>("get_transcription_history", {
        limit: 50,
      });
      const formattedHistory: TranscriptionHistory[] = storedHistory.map(
        (item) => ({
          id: item.timestamp || Date.now().toString(),
          text: item.text,
          timestamp: new Date(item.timestamp),
          model: item.model,
        }),
      );
      setHistory(formattedHistory);
    } catch (error) {
      console.error("Failed to load transcription history:", error);
    }
  }, []);

  // Initialize app
  useEffect(() => {
    const init = async () => {
      try {
        // Settings are loaded by SettingsProvider

        // Models are loaded automatically by useModelManagement hook

        // Check if onboarding is completed
        if (!settings?.onboarding_completed) {
          setShowOnboarding(true);
        }

        // Run cleanup if enabled
        if (settings?.transcription_cleanup_days) {
          await invoke("cleanup_old_transcriptions", {
            days: settings.transcription_cleanup_days,
          });
        }

        // Load initial transcription history
        await loadHistory();

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
          console.log(
            "[EventCoordinator] Main window: reloading history after update",
          );
          await loadHistory();
        });

        // Model download events are handled by useModelManagement hook

        // Listen for navigate-to-settings event from tray menu
        registerEvent("navigate-to-settings", () => {
          console.log("Navigate to settings requested from tray menu");
          setActiveSection("recordings");
        });

        // Settings events are handled by SettingsProvider

        // Listen for tray action errors
        registerEvent("tray-action-error", (event) => {
          console.error("Tray action error:", event.payload);
          toast.error(event.payload as string);
        });

        // Listen for AI enhancement errors
        registerEvent("ai-enhancement-auth-error", (event) => {
          console.error("AI authentication error:", event.payload);
          toast.error(event.payload as string, {
            description: "Navigate to Enhancements to update your API key",
            action: {
              label: "Go to Settings",
              onClick: () => setActiveSection("enhancements"),
            },
          });
        });

        registerEvent("ai-enhancement-error", (event) => {
          console.warn("AI enhancement error:", event.payload);
          toast.warning(event.payload as string);
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
              setActiveSection("account");

              // Show toast after window is focused to ensure it appears on top
              setTimeout(() => {
                toast.error(event.message, {
                  duration: 2000,
                });
              }, 200);
            } catch (error) {
              console.error("Failed to focus window:", error);
              // If window focus fails, still show the toast
              toast.error(event.message);
            }
          },
        );

        return () => {
          window.removeEventListener("no-models-available", handleNoModels);
          updateService.dispose();
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
    [deleteModel, settings],
  );

  // Save settings
  const saveSettings = useCallback(
    async (newSettings: AppSettings) => {
      try {
        // Update global shortcut in backend if changed
        if (newSettings.hotkey !== settings?.hotkey) {
          try {
            await invoke("set_global_shortcut", {
              shortcut: newSettings.hotkey,
            });
          } catch (err) {
            console.error("Failed to update hotkey:", err);
            // Still update UI even if hotkey registration fails
          }
        }

        await updateSettings(newSettings);
      } catch (error) {
        console.error("Failed to save settings:", error);
      }
    },
    [settings, updateSettings],
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
            refreshSettings();
          }}
          modelManagement={modelManagement}
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
        return <GeneralSettings />;

      case "models":
        return (
          <ModelsSection
            models={sortedModels}
            downloadProgress={downloadProgress}
            verifyingModels={verifyingModels}
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

      case "advanced":
        return <AdvancedSection />;

      case "enhancements":
        return <EnhancementsSection />;

      case "account":
      case "about":
      case "license":
        return <AccountSection />;

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
    <SidebarProvider>
      <Sidebar
        activeSection={activeSection}
        onSectionChange={setActiveSection}
      />
      <SidebarInset>
        <div className="h-full flex flex-col">{renderSectionContent()}</div>
      </SidebarInset>
    </SidebarProvider>
  );
}

export default function App() {
  return (
    <AppErrorBoundary>
      <LicenseProvider>
        <SettingsProvider>
          <ReadinessProvider>
            <AppContent />
            <Toaster position="top-center" />
          </ReadinessProvider>
        </SettingsProvider>
      </LicenseProvider>
    </AppErrorBoundary>
  );
}
