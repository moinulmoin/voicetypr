import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
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
import { ReadinessProvider, useReadiness } from "./contexts/ReadinessContext";
import { SettingsProvider, useSettings } from "./contexts/SettingsContext";
import { useEventCoordinator } from "./hooks/useEventCoordinator";
import { useModelManagement } from "./hooks/useModelManagement";
import { updateService } from "./services/updateService";
import { AppSettings, TranscriptionHistory } from "./types";

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
import { loadApiKeysToCache } from "./utils/keyring";

// Main App Component
function AppContent() {
  const { registerEvent } = useEventCoordinator("main");
  const [activeSection, setActiveSection] = useState<string>("recordings");
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [history, setHistory] = useState<TranscriptionHistory[]>([]);
  const { settings, refreshSettings, updateSettings } = useSettings();
  const { checkAccessibilityPermission, checkMicrophonePermission } = useReadiness();

  // Use the new model management hook
  const modelManagement = useModelManagement({
    windowId: "main",
    showToasts: true
  });
  const {
    downloadProgress,
    verifyingModels,
    downloadModel,
    cancelDownload,
    deleteModel,
    sortedModels
  } = modelManagement;

  // Load history function
  const loadHistory = useCallback(async () => {
    try {
      const storedHistory = await invoke<any[]>("get_transcription_history", {
        limit: 50
      });
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
        // Settings are loaded by SettingsProvider

        // Models are loaded automatically by useModelManagement hook

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
          console.log("[EventCoordinator] Main window: reloading history after update");
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
              onClick: () => setActiveSection("enhancements")
            }
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

        // Listen for no speech detected events (various audio validation failures)
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
                  setActiveSection('general');
                } else {
                  // Trigger recording again
                  invoke('start_recording').catch(console.error);
                }
              }
            },
            duration: data.severity === 'error' ? 8000 : 5000
          });
        });

        // Listen for transcription empty (when speech was detected but no text generated)
        registerEvent<string>("transcription-empty", (message) => {
          console.warn("Transcription empty:", message);
          
          toast.warning('No Text Generated', {
            description: 'The recording did not produce any text. Try speaking more clearly.',
            action: {
              label: 'Tips',
              onClick: () => {
                toast.info('Recording Tips', {
                  description: '• Speak clearly and at normal volume\n• Keep microphone 6-12 inches away\n• Minimize background noise\n• Ensure you have the correct language model',
                  duration: 8000
                });
              }
            },
            duration: 5000
          });
        });

        // Listen for hotkey registration failures
        registerEvent<ErrorEventPayload>("hotkey-registration-failed", (data) => {
          console.error("Hotkey registration failed:", data);
          
          toast.error('Hotkey Registration Failed', {
            description: data.suggestion || 'The hotkey is in use by another application',
            action: {
              label: 'Change Hotkey',
              onClick: () => {
                setActiveSection('general');
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

        // Listen for no models error (when trying to record without any models)
        registerEvent<ErrorEventPayload>("no-models-error", (data) => {
          console.error("No models available:", data);
          
          toast.error(data.title || 'No Models Available', {
            description: data.message || 'Please download at least one model from Settings before recording.',
            action: {
              label: 'Download Models',
              onClick: () => {
                setActiveSection('models');
                // Show additional guidance after navigation
                setTimeout(() => {
                  toast.info('Download Required', {
                    description: 'Choose a model size based on your needs. Larger models are more accurate but require more storage space.',
                    duration: 6000
                  });
                }, 500);
              }
            },
            duration: 8000
          });
        });

        // Listen for recording errors
        registerEvent<string>("recording-error", (errorMessage) => {
          console.error("Recording error:", errorMessage);
          
          toast.error('Recording Failed', {
            description: errorMessage || 'An error occurred during recording. Please try again.',
            action: {
              label: 'Try Again',
              onClick: () => {
                invoke('start_recording').catch(console.error);
              }
            },
            duration: 6000
          });
        });

        // Listen for transcription errors
        registerEvent<string>("transcription-error", (errorMessage) => {
          console.error("Transcription error:", errorMessage);
          
          toast.error('Transcription Failed', {
            description: errorMessage || 'An error occurred during transcription. Please try again.',
            action: {
              label: 'Try Again',
              onClick: () => {
                invoke('start_recording').catch(console.error);
              }
            },
            duration: 6000
          });
        });

        // Listen for download retry events (when download fails and retries)
        registerEvent<{ model: string; attempt: number; max_attempts: number; error: string }>(
          "download-retry", 
          (retryData) => {
            const { model, attempt, max_attempts } = retryData;
            console.warn("Download retry:", retryData);
            
            // Only show toast for the first retry to avoid spam
            if (attempt === 1) {
              toast.warning(`Download Retry`, {
                description: `Download of ${model} failed, retrying... (Attempt ${attempt}/${max_attempts})`,
                duration: 4000
              });
            }
          }
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
  }, [registerEvent, loadHistory, settings]);

  // Use a ref to track if we've just completed onboarding
  const hasJustCompletedOnboarding = useRef(false);

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
    }
  }, [
    showOnboarding,
    settings?.onboarding_completed,
    checkAccessibilityPermission,
    checkMicrophonePermission
  ]);

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
        // Update global shortcut in backend if changed
        if (newSettings.hotkey !== settings?.hotkey) {
          try {
            await invoke("set_global_shortcut", {
              shortcut: newSettings.hotkey
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
    [settings, updateSettings]
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
      <Sidebar activeSection={activeSection} onSectionChange={setActiveSection} />
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
