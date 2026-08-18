import { ApiKeyModal } from "@/components/ApiKeyModal";
import { EnhancementSettings } from "@/components/EnhancementSettings";
import { OpenAICompatConfigModal } from "@/components/OpenAICompatConfigModal";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Switch } from "@/components/ui/switch";
import type {
  EnhancementOptions,
  EnhancementPreset,
} from "@/types/ai";
import { fromBackendOptions, toBackendOptions } from "@/types/ai";
import type { WritingSettings } from "@/types/writing";
import { defaultWritingSettings, mergeWritingSettings } from "@/types/writing";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { HelpCircle } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { getErrorMessage } from "@/utils/error";
import { getApiKey, saveApiKey } from "@/utils/keyring";
import { useReadinessState } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import {
  useEnhancementsStore,
  type PolishErrorKind,
} from "@/state/enhancements";
import { ProviderSetupCard } from "@/components/polish/ProviderSetupCard";
import {
  useAiProviderSettings,
  type ProviderTab,
} from "@/components/polish/useAiProviderSettings";
import {
  AGENT_CLI_DEFAULT_LABEL,
  formatAgentCliReasoning,
  defaultAgentCliReasoning,
  isAgentCliProvider,
  shortModelName,
} from "@/components/polish/agentCli";

const log = createLogger("enhancements");

export function EnhancementsSection() {
  const readiness = useReadinessState();
  const { settings, updateSettings } = useSettings();

  const [providerSearch, setProviderSearch] = useState("");
  const [providerSetupOpen, setProviderSetupOpen] = useState(false);
  const [providerTab, setProviderTab] = useState<ProviderTab>("cloud");

  const [enhancementOptions, setEnhancementOptions] = useState<{
    preset: EnhancementPreset;
  }>({
    preset: "PersonalDictation",
  });
  const [writingSettings, setWritingSettings] = useState<WritingSettings>(
    defaultWritingSettings,
  );
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const writingSaveGeneration = useRef(0);
  const enhancementSaveGeneration = useRef(0);
  const writingSettingsRef = useRef(writingSettings);
  const writingSaveQueueRef = useRef<Promise<void> | null>(null);
  const settingsLoadStartedRef = useRef(false);

  useEffect(() => {
    writingSettingsRef.current = writingSettings;
  }, [writingSettings]);

  const loadEnhancementOptions = async (aiEnabled: boolean) => {
    try {
      const options = await invoke<EnhancementOptions>(
        "get_enhancement_options",
      );
      setEnhancementOptions(fromBackendOptions(options, aiEnabled));
    } catch (error) {
      log.error("Failed to load Polish options:", error);
    }
  };

  const loadWritingSettings = async () => {
    try {
      const nextSettings = await invoke<Partial<WritingSettings>>(
        "get_writing_settings",
      );
      setWritingSettings(mergeWritingSettings(nextSettings));
      return true;
    } catch (error) {
      log.error("Failed to load writing settings:", error);
      return false;
    }
  };

  const persistEnhancementOptions = async (nextOptions: {
    preset: EnhancementPreset;
  }) => {
    const rollbackOptions = enhancementOptions;
    const generationAtEnqueue = enhancementSaveGeneration.current + 1;
    enhancementSaveGeneration.current = generationAtEnqueue;
    setEnhancementOptions(nextOptions);
    try {
      await invoke("update_enhancement_options", {
        options: toBackendOptions(nextOptions),
      });
    } catch (error) {
      if (enhancementSaveGeneration.current === generationAtEnqueue) {
        setEnhancementOptions(rollbackOptions);
      }
      const message = getErrorMessage(error, "Failed to save Polish settings");
      toast.error(message);
    }
  };

  const enqueueWritingSettingsSave = (
    settingsToSave: WritingSettings,
    rollbackSettings: WritingSettings,
    generationAtEnqueue: number,
  ) => {
    const queue = writingSaveQueueRef.current ?? Promise.resolve();
    writingSaveQueueRef.current = queue.then(async () => {
      try {
        await invoke("update_writing_settings", { settings: settingsToSave });
      } catch (error) {
        if (writingSaveGeneration.current === generationAtEnqueue) {
          setWritingSettings(rollbackSettings);
          writingSettingsRef.current = rollbackSettings;
          const message = getErrorMessage(
            error,
            "Failed to save writing settings",
          );
          toast.error(message);
        }
      }
    });
  };

  const handleWritingSettingsChange = (nextSettings: WritingSettings) => {
    const rollbackSettings = writingSettingsRef.current;
    const generationAtEnqueue = writingSaveGeneration.current + 1;
    writingSaveGeneration.current = generationAtEnqueue;
    setWritingSettings(nextSettings);
    writingSettingsRef.current = nextSettings;
    if (settingsLoaded) {
      enqueueWritingSettingsSave(
        nextSettings,
        rollbackSettings,
        generationAtEnqueue,
      );
    }
  };

  const handleFinalTextLanguageChange = async (value: string) => {
    if (!settings) return;
    const nextTask = value === "en" ? "translate_to_english" : "transcribe";
    try {
      await updateSettings({
        final_text_language: value,
        transcription_task: nextTask,
      });
    } catch (error) {
      const message = getErrorMessage(
        error,
        "Failed to save final text language",
      );
      toast.error(message);
    }
  };

  // Always-fresh handler refs: the callbacks passed to useAiProviderSettings
  // keep stable identities and honest dependency arrays while still invoking
  // the latest closures. This does not change when saves fire — the
  // generation-counter / save-queue semantics live in the functions above.
  const persistEnhancementOptionsRef = useRef(persistEnhancementOptions);
  const loadEnhancementOptionsRef = useRef(loadEnhancementOptions);
  const loadWritingSettingsRef = useRef(loadWritingSettings);
  const handleFinalTextLanguageChangeRef = useRef(handleFinalTextLanguageChange);
  const updateSettingsRef = useRef(updateSettings);
  useEffect(() => {
    persistEnhancementOptionsRef.current = persistEnhancementOptions;
    loadEnhancementOptionsRef.current = loadEnhancementOptions;
    loadWritingSettingsRef.current = loadWritingSettings;
    handleFinalTextLanguageChangeRef.current = handleFinalTextLanguageChange;
    updateSettingsRef.current = updateSettings;
  });

  const handlePolishEnabled = useCallback(async () => {
    if (enhancementOptions.preset !== "CleanDictation") {
      await persistEnhancementOptionsRef.current({ preset: "CleanDictation" });
    }
  }, [enhancementOptions.preset]);

  const handleEnabledModelSelected = useCallback(async () => {
    await loadEnhancementOptionsRef.current(true);
  }, []);

  const handlePolishToggled = useCallback(
    async (enabled: boolean) => {
      const nextPreset: EnhancementPreset = enabled
        ? "CleanDictation"
        : "PersonalDictation";

      if (
        nextPreset === "PersonalDictation" &&
        settings?.final_text_language &&
        settings.final_text_language !== "same_as_transcript"
      ) {
        await handleFinalTextLanguageChangeRef.current("same_as_transcript");
      }

      if (nextPreset !== enhancementOptions.preset) {
        await persistEnhancementOptionsRef.current({ preset: nextPreset });
      }
    },
    [settings?.final_text_language, enhancementOptions.preset],
  );

  const handleActiveProviderCleared = useCallback(async () => {
    setEnhancementOptions({ preset: "PersonalDictation" });
    try {
      await updateSettingsRef.current({
        final_text_language: "same_as_transcript",
        transcription_task: "transcribe",
      });
    } catch (error) {
      log.error(
        "Failed to refresh language settings after API key removal:",
        error,
      );
    }
  }, []);

  const provider = useAiProviderSettings({
    readinessAiReady: readiness?.ai_ready,
    settingsLoaded,
    onPolishEnabled: handlePolishEnabled,
    onModelSelected: () => setProviderSetupOpen(true),
    onEnabledModelSelected: handleEnabledModelSelected,
    onPolishToggled: handlePolishToggled,
    onActiveProviderCleared: handleActiveProviderCleared,
  });

  const {
    aiSettings,
    aiModelNeedsReselection,
    providers,
    providerApiKeys,
    agentCliStatus,
    agentCliProbing,
    showApiKeyModal,
    setShowApiKeyModal,
    showOpenAIConfig,
    setShowOpenAIConfig,
    openAIDefaultBaseUrl,
    setOpenAIDefaultBaseUrl,
    customModelName,
    setCustomModelName,
    selectedProvider,
    guidedSetupProvider,
    setGuidedSetupProvider,
    isLoading,
    setIsLoading,
    setProviderApiKeys,
    setAISettings,
    loadAISettings,
    probeAgentCli,
    handleRefreshAgentCli,
    handleToggleEnabled,
    handleSetupApiKey,
    handleApiKeySubmit,
    handleRemoveApiKey,
    handleSelectModel,
    handleSelectReasoning,
    handleToggleFastMode,
    getDisplayModels,
    isModelsLoading,
    getModels,
    fetchModels,
    getError,
  } = provider;

  // The settings load below stays the authoritative load across re-renders
  // (loadAISettings changes identity as its own state settles); only an
  // unmount makes its post-await state updates stale.
  const settingsLoadMountedRef = useRef(true);
  useEffect(() => {
    settingsLoadMountedRef.current = true;
    return () => {
      settingsLoadMountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    if (settingsLoaded || settingsLoadStartedRef.current) {
      return;
    }

    settingsLoadStartedRef.current = true;
    (async () => {
      const loadedAISettings = await loadAISettings();
      await loadEnhancementOptionsRef.current(loadedAISettings?.enabled ?? false);
      const writingSettingsLoaded = await loadWritingSettingsRef.current();
      if (!settingsLoadMountedRef.current) return;
      setSettingsLoaded(writingSettingsLoaded);
      if (!writingSettingsLoaded) {
        settingsLoadStartedRef.current = false;
      }
    })().catch((error) => {
      settingsLoadStartedRef.current = false;
      log.error("Failed to load Polish settings:", error);
    });
  }, [settingsLoaded, loadAISettings]);

  const setPolishError = useEnhancementsStore((s) => s.setPolishError);
  const clearPolishError = useEnhancementsStore((s) => s.clearPolishError);

  // Surface persistent Polish failures inline on the Polish card (toasts are
  // left to EnhancementsTab); a successful polish run clears the banner.
  useTauriEvent<unknown>("ai-enhancement-auth-error", (payload) => {
    if (typeof payload === "string") {
      setPolishError("auth", payload);
    }
  });

  useTauriEvent<unknown>("ai-enhancement-error", (payload) => {
    if (typeof payload === "string") {
      setPolishError("generic", payload);
    }
  });

  useTauriEvent<{ category?: string; message?: string } | null>(
    "enhancing-failed",
    (payload) => {
      if (!payload || payload.category === "canceled") return;
      const kind: PolishErrorKind =
        payload.category === "missing_api_key" ||
        payload.category === "invalid_api_key"
          ? "auth"
          : "generic";
      setPolishError(kind, payload.message || "Polish failed");
    },
  );

  useTauriEvent("enhancing-completed", () => {
    clearPolishError();
  });

  const isUsingCustomProvider = aiSettings.provider === "custom";
  const hasSelectedModel = Boolean(
    aiSettings.provider &&
    // Agent-CLI providers carry no model — waive the model requirement
    // (availability still requires a probed provider key below).
    (aiSettings.model || isAgentCliProvider(aiSettings.provider)) &&
    (isUsingCustomProvider || providerApiKeys[aiSettings.provider]),
  );

  const showAiModelReselectionNotice =
    aiModelNeedsReselection && aiSettings.enabled && !aiSettings.model;

  const storedFinalTextLanguage =
    settings?.final_text_language ?? "same_as_transcript";
  const effectiveFinalTextLanguage =
    enhancementOptions.preset === "PersonalDictation"
      ? "same_as_transcript"
      : storedFinalTextLanguage;

  const selectedDisplayModel = getDisplayModels(aiSettings.provider).find(
    (model) => model.id === aiSettings.model,
  );
  const activeModelName = isUsingCustomProvider
    ? customModelName
    : !aiSettings.model && isAgentCliProvider(aiSettings.provider)
      ? AGENT_CLI_DEFAULT_LABEL
      : shortModelName(aiSettings.model, selectedDisplayModel?.name);
  const activeProviderName =
    providers.find((p) => p.id === aiSettings.provider)?.name ||
    aiSettings.provider;
  const activeReasoningLevels =
    agentCliStatus[aiSettings.provider]?.reasoningLevels ?? [];
  const activeReasoningName =
    isAgentCliProvider(aiSettings.provider) && activeReasoningLevels.length > 0
      ? formatAgentCliReasoning(
          aiSettings.provider,
          aiSettings.reasoningByProvider[aiSettings.provider] ??
            defaultAgentCliReasoning(aiSettings.provider),
        )
      : "";

  const openProviderSetup = () => {
    setProviderTab(isAgentCliProvider(aiSettings.provider) ? "local" : "cloud");
    setProviderSearch("");
    setProviderSetupOpen(true);
  };

  const showGuidedSetup = settingsLoaded && !hasSelectedModel;
  const polishHeaderActions = hasSelectedModel ? (
    <div className="flex items-center gap-3 rounded-lg border border-border/60 bg-card px-3 py-1.5">
      <div className="min-w-0 max-w-56 flex-1 text-right">
        <p className="truncate text-xs font-medium text-foreground">
          {activeProviderName}
        </p>
        <p className="truncate text-[11px] text-muted-foreground">
          {activeModelName || "Connected"}
        </p>
      </div>
      <Switch
        id="polish-enabled"
        aria-label="Polish"
        className="shrink-0"
        checked={aiSettings.enabled}
        onCheckedChange={handleToggleEnabled}
      />
    </div>
  ) : (
    <Button type="button" variant="outline" onClick={openProviderSetup}>
      Select a provider to enable Polish
    </Button>
  );

  const polishSetupExpanded = showGuidedSetup || providerSetupOpen;
  useEffect(() => {
    if (!polishSetupExpanded || providerTab !== "local") return;

    for (const candidate of providers) {
      if (!isAgentCliProvider(candidate.id)) continue;
      if (!agentCliStatus[candidate.id]) {
        void probeAgentCli(candidate.id, false);
      }
    }
  }, [agentCliStatus, polishSetupExpanded, probeAgentCli, providerTab, providers]);

  return (
    <div className="h-full min-h-0 min-w-0 flex flex-col overflow-x-hidden">
      <div className="shrink-0 py-5 pl-2 pr-4">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h1 className="text-2xl font-semibold tracking-tight">Polish</h1>
              <Dialog>
                <DialogTrigger render={<Button
                    type="button"
                    variant="secondary"
                    size="icon"
                    aria-label="Polish guide"
                    className="rounded-full"
                  />}><HelpCircle className="h-4.5 w-4.5" /></DialogTrigger>
                <DialogContent className="sm:max-w-lg">
                  <DialogHeader>
                    <DialogTitle>Polish guide</DialogTitle>
                    <DialogDescription>
                      Configure the provider, dictionary, corrections, snippets,
                      and writing modes.
                    </DialogDescription>
                  </DialogHeader>
                  <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                    <p>
                      <strong className="text-foreground">Provider</strong>{" "}
                      chooses the cloud API or isolated local agent used by
                      Polish.
                    </p>
                    <p>
                      <strong className="text-foreground">Dictionary</strong>{" "}
                      protects words and names and can improve recognition.
                    </p>
                    <p>
                      <strong className="text-foreground">Corrections</strong>{" "}
                      applies exact replacements with or without Polish.
                    </p>
                    <p>
                      <strong className="text-foreground">Snippets</strong>{" "}
                      expands “insert” triggers into saved text.
                    </p>
                    <p>
                      <strong className="text-foreground">Modes</strong>{" "}
                      sets the default writing mode and optional per-app
                      overrides.
                    </p>
                  </div>
                </DialogContent>
              </Dialog>
            </div>
            <p className="mt-0.5 text-sm leading-relaxed text-muted-foreground">
              AI cleanup when enabled; dictionary, corrections, snippets, and
              app identity remain available independently.
            </p>
          </div>
          <div className="ml-auto shrink-0">{polishHeaderActions}</div>
        </div>
      </div>

      <ScrollArea className="flex-1 min-h-0 min-w-0 overflow-x-hidden">
        <div className="min-w-0 max-w-full overflow-x-hidden pt-5 pb-4 pl-2 pr-4">
          <EnhancementSettings
            preset={enhancementOptions.preset}
            finalTextLanguage={effectiveFinalTextLanguage}
            writingSettings={writingSettings}
            aiFormattingEnabled={aiSettings.enabled}
            providerContent={
              <ProviderSetupCard
                aiSettings={aiSettings}
                providers={providers}
                providerApiKeys={providerApiKeys}
                agentCliStatus={agentCliStatus}
                agentCliProbing={agentCliProbing}
                customModelName={customModelName}
                guidedSetupProvider={guidedSetupProvider}
                setGuidedSetupProvider={setGuidedSetupProvider}
                isModelsLoading={isModelsLoading}
                getModels={getModels}
                fetchModels={(providerId) => void fetchModels(providerId)}
                getError={getError}
                getDisplayModels={getDisplayModels}
                hasSelectedModel={hasSelectedModel}
                showGuidedSetup={showGuidedSetup}
                showAiModelReselectionNotice={showAiModelReselectionNotice}
                activeProviderName={activeProviderName}
                activeModelName={activeModelName}
                activeReasoningName={activeReasoningName}
                providerTab={providerTab}
                setProviderTab={setProviderTab}
                providerSearch={providerSearch}
                setProviderSearch={setProviderSearch}
                providerSetupOpen={providerSetupOpen}
                setProviderSetupOpen={setProviderSetupOpen}
                onSelectModel={handleSelectModel}
                onSelectReasoning={handleSelectReasoning}
                onToggleFastMode={handleToggleFastMode}
                onRefreshAgentCli={handleRefreshAgentCli}
                onSetupApiKey={handleSetupApiKey}
                onRemoveApiKey={handleRemoveApiKey}
              />
            }
            onPresetChange={(preset) =>
              void persistEnhancementOptions({ preset })
            }
            onFinalTextLanguageChange={handleFinalTextLanguageChange}
            onWritingSettingsChange={handleWritingSettingsChange}
            writingSettingsDisabled={!settingsLoaded}
          />
        </div>
      </ScrollArea>

      <ApiKeyModal
        isOpen={showApiKeyModal}
        onClose={() => {
          setShowApiKeyModal(false);
          setGuidedSetupProvider(null);
        }}
        onSubmit={handleApiKeySubmit}
        providerName={selectedProvider}
        isLoading={isLoading}
      />

      <OpenAICompatConfigModal
        isOpen={showOpenAIConfig}
        defaultBaseUrl={openAIDefaultBaseUrl}
        defaultModel={customModelName || ""}
        onClose={() => setShowOpenAIConfig(false)}
        onSubmit={async ({ baseUrl, model, apiKey }) => {
          try {
            setIsLoading(true);
            const trimmedBase = baseUrl.trim();
            const trimmedModel = model.trim();
            const trimmedKey = apiKey?.trim() || "";

            const existingKey = trimmedKey ? "" : await getApiKey("custom");
            const validationKey = trimmedKey || existingKey || "";
            const noAuth = !validationKey;

            if (trimmedKey) {
              await saveApiKey("custom", trimmedKey, {
                baseUrl: trimmedBase,
                model: trimmedModel,
                noAuth: false,
              });
            } else {
              await invoke("validate_ai_api_key", {
                args: {
                  provider: "custom",
                  apiKey: validationKey,
                  baseUrl: trimmedBase,
                  model: trimmedModel,
                  noAuth,
                },
              });
              if (validationKey) {
                await invoke("cache_ai_api_key", {
                  args: { provider: "custom", apiKey: validationKey },
                });
              }
            }

            await invoke("set_openai_config", {
              args: { baseUrl: trimmedBase, noAuth },
            });

            await invoke("update_ai_settings", {
              enabled: aiSettings.enabled,
              provider: "custom",
              model: trimmedModel,
            });

            setCustomModelName(trimmedModel);
            setOpenAIDefaultBaseUrl(trimmedBase);
            setAISettings((prev) => ({
              ...prev,
              provider: "custom",
              model: trimmedModel,
              hasApiKey: true,
              modelsByProvider: {
                ...prev.modelsByProvider,
                custom: trimmedModel,
              },
            }));
            setProviderApiKeys((prev) => ({ ...prev, custom: true }));

            toast.success("Custom provider configured");
            setShowOpenAIConfig(false);
          } catch (error) {
            const message = getErrorMessage(
              error,
              "Failed to save configuration",
            );
            toast.error(message);
          } finally {
            setIsLoading(false);
          }
        }}
      />
    </div>
  );
}
