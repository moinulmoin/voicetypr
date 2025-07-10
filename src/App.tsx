import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { Mic, Settings } from "lucide-react";
import { useEffect, useState, useMemo, useCallback } from "react";
import { HotkeyInput } from "./components/HotkeyInput";
import { ModelCard } from "./components/ModelCard";
import { Button } from "./components/ui/button";
import { Label } from "./components/ui/label";
import { ScrollArea } from "./components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "./components/ui/select";
import { Separator } from "./components/ui/separator";
import { Switch } from "./components/ui/switch";
import { useRecording } from "./hooks/useRecording";
import { useEventCoordinator } from "./hooks/useEventCoordinator";
import { AppSettings, ModelInfo, TranscriptionHistory } from "./types";
import { AppErrorBoundary, RecordingErrorBoundary, SettingsErrorBoundary, ModelManagementErrorBoundary } from "./components/ErrorBoundary";

// Helper function to calculate balanced performance score
function calculateBalancedScore(model: ModelInfo): number {
  // Weighted average: 40% speed, 60% accuracy
  return (model.speed_score * 0.4 + model.accuracy_score * 0.6) / 10 * 100;
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
  const recording = useRecording();
  const { registerEvent } = useEventCoordinator("main");
  const [currentView, setCurrentView] = useState<"recorder" | "settings" | "onboarding">(
    "recorder"
  );
  const [models, setModels] = useState<Record<string, ModelInfo>>({});
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [history, setHistory] = useState<TranscriptionHistory[]>([]);
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({});

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
          setCurrentView("onboarding");
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
          setCurrentView("onboarding");
        };
        window.addEventListener("no-models-available", handleNoModels);
        
        // Listen for history updates from backend
        // Backend is the single source of truth for transcription history
        registerEvent("history-updated", async () => {
          console.log("[EventCoordinator] Main window: reloading history after update");
          await loadHistory();
        });

        registerEvent<{ model: string; progress: number }>("download-progress", ({ model, progress }) => {
          setDownloadProgress((prev) => ({
            ...prev,
            [model]: progress
          }));
        });

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
  const deleteModel = useCallback(async (modelName: string) => {
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
  }, [settings]);

  // Cancel download (placeholder - backend support needed)
  const cancelDownload = useCallback(async (modelName: string) => {
    try {
      // TODO: Implement backend support for cancelling downloads
      console.log(`Cancelling download for model: ${modelName}`);

      // For now, just remove from progress
      setDownloadProgress((prev) => {
        const newProgress = { ...prev };
        delete newProgress[modelName];
        return newProgress;
      });
    } catch (error) {
      console.error("Failed to cancel download:", error);
    }
  }, []);

  // Save settings
  const saveSettings = useCallback(async (newSettings: AppSettings) => {
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
  }, [settings]);

  // Memoize sorted models to avoid recalculation on every render
  const sortedModels = useMemo(() => sortModels(Object.entries(models), "accuracy"), [models]);

  // Onboarding View
  if (currentView === "onboarding") {
    return (
      <AppErrorBoundary>
        <div className="flex flex-col h-screen bg-background">
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
                    auto_insert: true,
                    show_window_on_record: false,
                    theme: "system"
                  };

                  // Save with selected model
                  await saveSettings({ ...newSettings, current_model: modelName });
                  setCurrentView("recorder");
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

  // Settings View
  if (currentView === "settings") {
    return (
      <SettingsErrorBoundary>
        <div className="flex flex-col h-screen bg-background">
          <div className="flex items-center justify-between p-4 border-b">
            <h2 className="text-lg font-semibold">Settings</h2>
            <Button onClick={() => setCurrentView("recorder")} variant="ghost" size="sm">
              ✕
            </Button>
          </div>

          <div className="flex-1 overflow-y-auto p-4 space-y-6">
          {/* Hotkey Setting */}
          <div className="space-y-2">
            <Label htmlFor="hotkey">Hotkey</Label>
            <HotkeyInput
              value={settings?.hotkey || ""}
              onChange={(hotkey) => settings && saveSettings({ ...settings, hotkey })}
              placeholder="Click to set hotkey"
            />
          </div>

          {/* Language Setting */}
          <div className="space-y-2">
            <Label htmlFor="language">Language</Label>
            <Select
              value={settings?.language || "auto"}
              onValueChange={(value) => settings && saveSettings({ ...settings, language: value })}
            >
              <SelectTrigger id="language">
                <SelectValue placeholder="Select language" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="auto">Auto-detect</SelectItem>
                <SelectItem value="en">English</SelectItem>
                <SelectItem value="es">Spanish</SelectItem>
                <SelectItem value="fr">French</SelectItem>
                <SelectItem value="de">German</SelectItem>
                <SelectItem value="it">Italian</SelectItem>
                <SelectItem value="pt">Portuguese</SelectItem>
                <SelectItem value="ru">Russian</SelectItem>
                <SelectItem value="ja">Japanese</SelectItem>
                <SelectItem value="ko">Korean</SelectItem>
                <SelectItem value="zh">Chinese</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {/* Auto Insert Toggle */}
          <div className="flex items-center justify-between">
            <Label htmlFor="auto-insert" className="flex-1">
              Auto-insert text at cursor
            </Label>
            <Switch
              id="auto-insert"
              checked={settings?.auto_insert || false}
              onCheckedChange={(checked) =>
                settings && saveSettings({ ...settings, auto_insert: checked })
              }
            />
          </div>

          {/* Show Window on Record Toggle */}
          <div className="flex items-center justify-between">
            <Label htmlFor="show-window" className="flex-1">
              Show window when recording
            </Label>
            <Switch
              id="show-window"
              checked={settings?.show_window_on_record || false}
              onCheckedChange={(checked) =>
                settings && saveSettings({ ...settings, show_window_on_record: checked })
              }
            />
          </div>

          {/* Show Pill Widget Toggle */}
          <div className="flex items-center justify-between">
            <Label htmlFor="show-pill" className="flex-1">
              Show floating pill when recording
            </Label>
            <Switch
              id="show-pill"
              checked={settings?.show_pill_widget ?? true}
              onCheckedChange={(checked) =>
                settings && saveSettings({ ...settings, show_pill_widget: checked })
              }
            />
          </div>

          {/* Transcription History Cleanup */}
          <div className="space-y-2">
            <Label htmlFor="cleanup">Keep transcription history</Label>
            <Select
              value={settings?.transcription_cleanup_days?.toString() || "forever"}
              onValueChange={(value) => {
                const days = value === "forever" ? null : parseInt(value);
                settings && saveSettings({ ...settings, transcription_cleanup_days: days });
              }}
            >
              <SelectTrigger id="cleanup">
                <SelectValue placeholder="Select retention period" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="forever">Forever</SelectItem>
                <SelectItem value="7">7 days</SelectItem>
                <SelectItem value="15">15 days</SelectItem>
                <SelectItem value="30">30 days</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {/* Model Management */}
          <Separator />
          <ModelManagementErrorBoundary>
            <div className="space-y-3">
              <div>
                <h3 className="text-base font-semibold">Available Models</h3>
                <p className="text-xs text-muted-foreground mt-1">
                  Select a model based on your needs
                </p>
              </div>

              <ScrollArea className="h-[280px]">
                <div className="space-y-2 pr-3">
                  {/* Sort by balanced score (downloaded models always first) */}
                  {sortedModels
                    .map(([name, model]) => (
                    <ModelCard
                      key={name}
                      name={name}
                      model={model}
                      downloadProgress={downloadProgress[name]}
                      onDownload={downloadModel}
                      onDelete={deleteModel}
                      onCancelDownload={cancelDownload}
                      onSelect={async (modelName) => {
                        if (model.downloaded && settings) {
                          await saveSettings({ ...settings, current_model: modelName });
                        }
                      }}
                      showSelectButton={model.downloaded}
                      isSelected={settings?.current_model === name}
                    />
                  ))}
                </div>
              </ScrollArea>
            </div>
          </ModelManagementErrorBoundary>
        </div>
      </div>
      </SettingsErrorBoundary>
    );
  }

  // Main Recorder View
  return (
    <AppErrorBoundary>
      <div className="flex flex-col h-screen bg-background">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b">
          <h1 className="text-lg font-semibold">VoiceType</h1>
          <div className="flex items-center gap-3">
            {/* Recording Status Indicator */}
            {recording.isActive && (
              <div className="flex items-center gap-2 text-sm">
                <div className={`w-2 h-2 rounded-full ${
                  recording.state === "recording" || recording.state === "starting"
                    ? "bg-destructive animate-pulse"
                    : recording.state === "transcribing"
                    ? "bg-yellow-500 animate-pulse"
                    : "bg-muted"
                }`} />
                <span className={`font-medium ${
                  recording.state === "recording" || recording.state === "starting"
                    ? "text-destructive"
                    : recording.state === "transcribing"
                    ? "text-yellow-600"
                    : "text-muted-foreground"
                }`}>
                  {recording.state === "starting" && "Starting..."}
                  {recording.state === "recording" && "Recording"}
                  {recording.state === "stopping" && "Stopping..."}
                  {recording.state === "transcribing" && "Processing"}
                </span>
              </div>
            )}
            <Button onClick={() => setCurrentView("settings")} variant="ghost" size="icon" aria-label="Settings">
              <Settings className="w-5 h-5" />
            </Button>
          </div>
        </div>

        <RecordingErrorBoundary>
          {/* Transcription History - Now the main content */}
          <div className="flex-1 flex flex-col p-6 overflow-hidden">
            <h2 className="text-lg font-semibold mb-4">Recent Transcriptions</h2>
            {history.length > 0 ? (
              <ScrollArea className="flex-1">
                <div className="space-y-3 pr-4">
                  {history.map((item) => (
                    <div
                      key={item.id}
                      className="p-4 rounded-lg border cursor-pointer hover:bg-accent/50 transition-colors"
                      onClick={() => navigator.clipboard.writeText(item.text)}
                      title="Click to copy"
                    >
                      <p className="text-sm text-foreground leading-relaxed">{item.text}</p>
                      <p className="text-xs text-muted-foreground mt-2">
                        {new Date(item.timestamp).toLocaleTimeString()} • {item.model}
                      </p>
                    </div>
                  ))}
                </div>
              </ScrollArea>
            ) : (
              <div className="flex-1 flex items-center justify-center">
                <div className="text-center">
                  <Mic className="w-12 h-12 text-muted-foreground/30 mx-auto mb-4" />
                  <p className="text-sm text-muted-foreground">No transcriptions yet</p>
                  <p className="text-xs text-muted-foreground mt-2">
                    Press {settings?.hotkey || "Cmd+Shift+Space"} to start recording
                  </p>
                </div>
              </div>
            )}
          </div>
        </RecordingErrorBoundary>
      </div>
    </AppErrorBoundary>
  );
}
