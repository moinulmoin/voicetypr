import { useCallback, useEffect, useMemo, useRef } from "react";
import { toast } from "sonner";
import { ModelsSection } from "../sections/ModelsSection";
import { useSettings } from "@/contexts/SettingsContext";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import { useModelManagementContext } from "@/contexts/ModelManagementContext";
import { AppSettings } from "@/types";
import { getModelDisplayName } from "@/lib/model-display";

export function ModelsTab() {
  const { registerEvent } = useEventCoordinator("main");
  const { settings, updateSettings } = useSettings();

  // Use the model management context
  const {
    downloadProgress,
    downloadPhases,
    verifyingModels,
    downloadModel,
    cancelDownload,
    deleteModel,
    repairModel,
    loadModels,
    sortedModels
  } = useModelManagementContext();

  const modelLabels = useMemo(
    () => new Map(sortedModels.map(([name, model]) => [name, getModelDisplayName(name, { [name]: model }) ?? name])),
    [sortedModels]
  );
  const modelLabelsRef = useRef(modelLabels);

  useEffect(() => {
    modelLabelsRef.current = modelLabels;
  }, [modelLabels]);

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
      await deleteModel(modelName);

      // If deleted model was the current one, clear selection in settings
      if (settings?.current_model === modelName) {
        await saveSettings({ current_model: "", current_model_engine: 'whisper' });
      }
    },
    [deleteModel, settings, saveSettings]
  );

  // Initialize models tab
  useEffect(() => {
    const init = async () => {
      try {
        // Listen for download error events (when download fails)
        registerEvent<{ model: string; engine?: string; error: string }>(
          "download-error",
          (errorData) => {
            const { model, error } = errorData;
            const modelLabel = modelLabelsRef.current.get(model) || getModelDisplayName(model) || "Unknown model";
            console.error("Download error:", errorData);

            // Don't show error toast if it was cancelled - cancellation has its own toast

            if (!error.toLowerCase().includes('cancel')) {
              // Show user-friendly error message
              toast.error(`Download Failed`, {
                description: `Failed to download ${modelLabel}. Please try again.`,
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
      downloadPhases={downloadPhases}
      verifyingModels={verifyingModels}
      currentModel={settings?.current_model}
      onDownload={downloadModel}
      onDelete={handleDeleteModel}
      onCancelDownload={cancelDownload}
      onRepair={repairModel}
      onSelect={async (modelName) => {
        if (!settings) return;
        const engine = sortedModels.find(([name]) => name === modelName)?.[1]?.engine ?? 'whisper';
        const previousSpeechLanguage = settings.speech_language;
        const parakeetSupportedLanguages = new Set([
          'bg', 'cs', 'da', 'de', 'el', 'en', 'es', 'et', 'fi', 'fr', 'hr', 'hu',
          'it', 'lt', 'lv', 'mt', 'nl', 'pl', 'pt', 'ro', 'ru', 'sk', 'sl', 'sv', 'uk',
        ]);
        const requiresSpeechLanguageReset =
          (engine === 'whisper' && /\.en$/i.test(modelName) && previousSpeechLanguage !== 'en') ||
          (engine === 'parakeet' &&
            ((modelName.includes('-v2') && previousSpeechLanguage !== 'en') ||
              (!modelName.includes('-v2') &&
                !parakeetSupportedLanguages.has(previousSpeechLanguage))));

        await saveSettings({
          current_model: modelName,
          current_model_engine: engine,
          ...(requiresSpeechLanguageReset ? { speech_language: 'en' } : {}),
        });

        if (requiresSpeechLanguageReset) {
          toast.info('Spoken language reset to English for the new model.');
        }
      }}
      refreshModels={async () => {
        await loadModels();
      }}
    />
  );
}
