import { useCallback, useEffect } from "react";
import { toast } from "sonner";
import { ModelsSection } from "../sections/ModelsSection";
import { useSettings } from "@/contexts/SettingsContext";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import { useModelManagementContext } from "@/contexts/ModelManagementContext";
import { AppSettings } from "@/types";

export function ModelsTab() {
  const { registerEvent } = useEventCoordinator("main");
  const { settings, updateSettings } = useSettings();

  // Use the model management context
  const {
    downloadProgress,
    verifyingModels,
    downloadModel,
    cancelDownload,
    deleteModel,
    sortedModels
  } = useModelManagementContext();

  // Handle deleting a model with settings update
  const handleDeleteModel = useCallback(
    async (modelName: string) => {
      await deleteModel(modelName);

      // If deleted model was the current one, clear selection in settings
      if (settings?.current_model === modelName) {
        await saveSettings({ current_model: "" });
      }
    },
    [deleteModel, settings]
  );

  // Save settings
  const saveSettings = useCallback(
    async (updates: Partial<AppSettings>) => {
      try {
        await updateSettings(updates);
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
        // Listen for download error events (when download fails)
        registerEvent<{ model: string; error: string }>(
          "download-error",
          (errorData) => {
            const { model, error } = errorData;
            console.error("Download error:", errorData);

            // Don't show error toast if it was cancelled - cancellation has its own toast
            if (!error.toLowerCase().includes('cancel')) {
              // Show user-friendly error message
              toast.error(`Download Failed`, {
                description: `Failed to download ${model}. Please try again.`,
                duration: 5000
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
          await saveSettings({ current_model: modelName });
        }
      }}
    />
  );
}