import { useCallback } from "react";
import { toast } from "sonner";
import { ModelsSection } from "../sections/ModelsSection";
import { useSettings } from "@/contexts/SettingsContext";
import { useModelManagementContext } from "@/contexts/ModelManagementContext";
import { AppSettings } from "@/types";

export function ModelsTab() {
  const { settings, updateSettings } = useSettings();

  // Use the model management context
  const {
    downloadProgress,
    downloadErrors,
    verifyingModels,
    downloadModel,
    cancelDownload,
    deleteModel,
    loadModels,
    sortedModels,
    isLoading,
  } = useModelManagementContext();

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

  // Handle deleting a model with settings update
  const handleDeleteModel = useCallback(
    async (modelName: string) => {
      try {
        const deleted = await deleteModel(modelName);
        if (!deleted) {
          return;
        }

        // If deleted model was the current one, clear selection in settings
        if (settings?.current_model === modelName) {
          await saveSettings({ current_model: "", current_model_engine: 'whisper' });
        }
      } catch {
        // deleteModel shows a toast and rethrows; keep current selection on failure
      }
    },
    [deleteModel, settings, saveSettings]
  );


  return (
    <ModelsSection
      models={sortedModels}
      downloadProgress={downloadProgress}
      downloadErrors={downloadErrors}
      verifyingModels={verifyingModels}
      currentModel={settings?.current_model}
      onDownload={downloadModel}
      onDelete={handleDeleteModel}
      onCancelDownload={cancelDownload}
      onSelect={async (modelName) => {
        if (!settings) return;
        const engine = sortedModels.find(([name]) => name === modelName)?.[1]?.engine ?? 'whisper';

        await saveSettings({
          current_model: modelName,
          current_model_engine: engine,
          language: 'en',
        });

        if (settings.language !== 'en') {
          toast.info('Spoken language reset to English for the new model.');
        }
      }}
      refreshModels={async () => {
        await loadModels();
      }}
      isLoading={isLoading}
    />
  );
}
