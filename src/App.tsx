import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useMemo, useState } from "react";
import { AppErrorBoundary, ModelManagementErrorBoundary } from "./components/ErrorBoundary";
import { ModelCard } from "./components/ModelCard";
import { Sidebar } from "./components/Sidebar";
import { RecentRecordings } from "./components/sections/RecentRecordings";
import { GeneralSettings } from "./components/sections/GeneralSettings";
import { ModelsSection } from "./components/sections/ModelsSection";
import { AboutSection } from "./components/sections/AboutSection";
import { useEventCoordinator } from "./hooks/useEventCoordinator";
import { useAccessibilityPermission } from "./hooks/useAccessibilityPermission";
import { AppSettings, ModelInfo, TranscriptionHistory } from "./types";
import { SidebarProvider, SidebarInset } from "./components/ui/sidebar";
import { Toaster } from "sonner";

// Helper function to calculate balanced performance score
function calculateBalancedScore(model: ModelInfo): number {
  // Weighted average: 40% speed, 60% accuracy
  return ((model.speed_score * 0.4 + model.accuracy_score * 0.6) / 10) * 100;
}

// Helper function to sort models by various criteria
function sortModels(
  models: [string, ModelInfo][],
  sortBy: "balanced" | "speed" | "accuracy" | "size" = "balanced"
): [string, ModelInfo][] {
  return [...models].sort(([, a], [, b]) => {
    // Sort by the specified criteria
    switch (sortBy) {
      case "speed":
        return b.speed_score - a.speed_score;
      case "accuracy":
        return a.accuracy_score - b.accuracy_score;
      case "size":
        return a.size - b.size;
      case "balanced":
      default:
        return calculateBalancedScore(b) - calculateBalancedScore(a);
    }
  });
}

// Main App Component
export default function App() {
  const { registerEvent } = useEventCoordinator("main");
  const [activeSection, setActiveSection] = useState<string>("general");
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [models, setModels] = useState<Record<string, ModelInfo>>({});
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [history, setHistory] = useState<TranscriptionHistory[]>([]);
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({});

  // Check accessibility permissions on macOS
  useAccessibilityPermission();

  // Initialize app
  useEffect(() => {
    const init = async () => {
      try {
        // Load settings
        const appSettings = await invoke<AppSettings>("get_settings");
        setSettings(appSettings);

        // Load model status
        const modelStatus = await invoke<Record<string, ModelInfo>>("get_model_status");
        console.log("Model status from backend:", modelStatus);
        setModels(modelStatus);

        // Check if any model is downloaded
        const hasModel = Object.values(modelStatus).some((m) => m.downloaded);
        console.log("Has downloaded model:", hasModel);
        if (!hasModel) {
          setShowOnboarding(true);
        }

        // Run cleanup if enabled
        if (appSettings.transcription_cleanup_days) {
          await invoke("cleanup_old_transcriptions", {
            days: appSettings.transcription_cleanup_days
          });
        }

        // Define loadHistory function
        const loadHistory = async () => {
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
        };

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

        registerEvent<{ model: string; progress: number }>(
          "download-progress",
          ({ model, progress }) => {
            // If progress reaches 100%, remove from download progress
            // The model-downloaded event will handle setting it as downloaded
            if (progress >= 100) {
              setDownloadProgress((prev) => {
                const newProgress = { ...prev };
                delete newProgress[model];
                return newProgress;
              });
            } else {
              setDownloadProgress((prev) => ({
                ...prev,
                [model]: progress
              }));
            }
          }
        );

        registerEvent<string>("model-downloaded", (modelName) => {
          setModels((prev) => ({
            ...prev,
            [modelName]: { ...prev[modelName], downloaded: true }
          }));
          setDownloadProgress((prev) => {
            const newProgress = { ...prev };
            delete newProgress[modelName];
            return newProgress;
          });
        });

        registerEvent<string>("download-cancelled", (modelName) => {
          console.log(`Download cancelled for model: ${modelName}`);
          // Remove from download progress
          setDownloadProgress((prev) => {
            const newProgress = { ...prev };
            delete newProgress[modelName];
            return newProgress;
          });
        });

        // Listen for navigate-to-settings event from tray menu
        registerEvent("navigate-to-settings", () => {
          console.log("Navigate to settings requested from tray menu");
          setActiveSection("settings");
        });

        return () => {
          window.removeEventListener("no-models-available", handleNoModels);
        };
      } catch (error) {
        console.error("Failed to initialize:", error);
      }
    };

    init();
  }, [registerEvent]);

  // Download model
  const downloadModel = useCallback(async (modelName: string) => {
    try {
      console.log(`Starting download for model: ${modelName}`);
      // Set initial progress to show download started
      setDownloadProgress((prev) => ({
        ...prev,
        [modelName]: 0
      }));

      // Don't await - let it run async so progress events can update UI
      invoke("download_model", { modelName }).catch((error) => {
        console.error("Failed to download model:", error);
        alert(`Failed to download model: ${error}`);
        // Remove from progress on error
        setDownloadProgress((prev) => {
          const newProgress = { ...prev };
          delete newProgress[modelName];
          return newProgress;
        });
      });
    } catch (error) {
      console.error("Failed to start download:", error);
      alert(`Failed to start download: ${error}`);
    }
  }, []);

  // Delete model
  const deleteModel = useCallback(
    async (modelName: string) => {
      console.log("deleteModel called with:", modelName);
      try {
        const confirmed = await ask(`Are you sure you want to delete the ${modelName} model?`, {
          title: "Delete Model",
          kind: "warning"
        });

        if (!confirmed) {
          return;
        }

        console.log("Calling delete_model command...");
        await invoke("delete_model", { modelName });
        console.log("delete_model command completed");

        // Refresh model status
        const modelStatus = await invoke<Record<string, ModelInfo>>("get_model_status");
        setModels(modelStatus);

        // If deleted model was the current one, clear selection
        if (settings?.current_model === modelName) {
          await saveSettings({ ...settings, current_model: "" });
        }
      } catch (error) {
        console.error("Failed to delete model:", error);
        alert(`Failed to delete model: ${error}`);
      }
    },
    [settings]
  );

  // Cancel download
  const cancelDownload = useCallback(async (modelName: string) => {
    try {
      console.log(`Cancelling download for model: ${modelName}`);
      
      // Call backend to cancel download (deletes partial file)
      await invoke("cancel_download", { modelName });
      
      // Remove from progress tracking
      setDownloadProgress((prev) => {
        const newProgress = { ...prev };
        delete newProgress[modelName];
        return newProgress;
      });
    } catch (error) {
      console.error("Failed to cancel download:", error);
      alert(`Failed to cancel download: ${error}`);
    }
  }, []);

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

  // Memoize sorted models to avoid recalculation on every render
  const sortedModels = useMemo(() => sortModels(Object.entries(models), "accuracy"), [models]);

  // Onboarding View
  if (showOnboarding) {
    return (
      <AppErrorBoundary>
        <div className="flex flex-col h-screen bg-gray-50 dark:bg-gray-950">
          <div className="flex-1 flex flex-col items-center justify-center p-8">
            <h1 className="text-4xl font-bold mb-2">Welcome to VoiceType</h1>
            <p className="text-lg text-muted-foreground mb-8">Choose a model to get started</p>

            <ModelManagementErrorBoundary>
              <div className="space-y-4 w-full max-w-md">
                {sortedModels.map(([name, model]) => (
                  <ModelCard
                    key={name}
                    name={name}
                    model={model}
                    downloadProgress={downloadProgress[name]}
                    onDownload={downloadModel}
                    onDelete={deleteModel}
                    onCancelDownload={cancelDownload}
                    onSelect={async (modelName) => {
                      // Create default settings if none exist
                      const newSettings = settings || {
                        hotkey: "CommandOrControl+Shift+Space",
                        language: "auto",
                        theme: "system"
                      };

                      // Save with selected model
                      await saveSettings({ ...newSettings, current_model: modelName });
                      setShowOnboarding(false);
                    }}
                  />
                ))}
              </div>
            </ModelManagementErrorBoundary>
          </div>
        </div>
      </AppErrorBoundary>
    );
  }

  // Render section content based on active section
  const renderSectionContent = () => {
    switch (activeSection) {
      case "recordings":
        return (
          <RecentRecordings history={history} hotkey={settings?.hotkey || "Cmd+Shift+Space"} />
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
            onDelete={deleteModel}
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

      default:
        return <GeneralSettings settings={settings} onSettingsChange={saveSettings} />;
    }
  };

  // Main App Layout
  return (
    <AppErrorBoundary>
      <SidebarProvider>
        <Sidebar activeSection={activeSection} onSectionChange={setActiveSection} />
        <SidebarInset>
          <div className="h-full overflow-auto">{renderSectionContent()}</div>
        </SidebarInset>
      </SidebarProvider>
      <Toaster position="top-right" />
    </AppErrorBoundary>
  );
}
