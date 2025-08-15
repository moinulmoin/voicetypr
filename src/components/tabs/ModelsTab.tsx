import { useCallback, useEffect } from "react";
import { toast } from "sonner";
import { ModelsSection } from "../sections/ModelsSection";
import { useSettings } from "@/contexts/SettingsContext";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import { useModelManagement } from "@/hooks/useModelManagement";
import { AppSettings } from "@/types";

export function ModelsTab() {
  const { registerEvent } = useEventCoordinator("main");
  const { settings, updateSettings } = useSettings();

  // Use the model management hook
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
        await updateSettings(newSettings);
      } catch (error) {
        console.error("Failed to save settings:", error);
      }
    },
    [updateSettings]
  );

  // Initialize models tab
  useEffect(() => {
    const init = async () => {
      try {
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
      } catch (error) {
        console.error("Failed to initialize models tab:", error);
      }
    };

    init();
  }, [registerEvent]);

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
}