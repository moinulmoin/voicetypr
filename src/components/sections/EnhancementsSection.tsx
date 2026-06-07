import { ApiKeyModal } from "@/components/ApiKeyModal";
import { EnhancementSettings } from "@/components/EnhancementSettings";
import { OpenAICompatConfigModal } from "@/components/OpenAICompatConfigModal";
import { ProviderCard } from "@/components/ProviderCard";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLegend,
  FieldSet,
  FieldTitle,
} from "@/components/ui/field";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import type { EnhancementOptions, EnhancementPreset } from "@/types/ai";
import {
  fromBackendOptions,
  presetRequiresAiFormatting,
  toBackendOptions,
} from "@/types/ai";
import type { WritingSettings } from "@/types/writing";
import { defaultWritingSettings, mergeWritingSettings } from "@/types/writing";
import { AI_PROVIDERS } from "@/types/providers";
import { useAllProviderModels } from "@/hooks/useProviderModels";
import { hasApiKey, removeApiKey, saveApiKey, getApiKey } from "@/utils/keyring";
import { getErrorMessage } from "@/utils/error";
import { useReadinessState } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { humanizeModelId } from "@/lib/model-display";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState, useCallback, useRef } from "react";
import { toast } from "sonner";
import { HelpCircle } from "lucide-react";

interface AISettings {
  enabled: boolean;
  provider: string;
  model: string;
  hasApiKey: boolean;
}

export function EnhancementsSection() {
  const readiness = useReadinessState();
  const { settings, updateSettings } = useSettings();
  const { fetchModels, getModels, isLoading: isModelsLoading, getError, clearModels } =
    useAllProviderModels();

  const [aiSettings, setAISettings] = useState<AISettings>({
    enabled: false,
    provider: "",
    model: "",
    hasApiKey: false,
  });

  const [showApiKeyModal, setShowApiKeyModal] = useState(false);
  const [showOpenAIConfig, setShowOpenAIConfig] = useState(false);
  const [openAIDefaultBaseUrl, setOpenAIDefaultBaseUrl] = useState(
    "https://api.openai.com/v1",
  );
  const [customModelName, setCustomModelName] = useState<string>("");
  const [selectedProvider, setSelectedProvider] = useState<string>("");
  const [isLoading, setIsLoading] = useState(false);
  const [providerApiKeys, setProviderApiKeys] = useState<Record<string, boolean>>({});
  const [enhancementOptions, setEnhancementOptions] = useState<{
    preset: EnhancementPreset;
  }>({
    preset: "PersonalDictation",
  });
  const [writingSettings, setWritingSettings] =
    useState<WritingSettings>(defaultWritingSettings);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const writingSaveGeneration = useRef(0);
  const writingSettingsRef = useRef(writingSettings);
  const writingSaveQueueRef = useRef(Promise.resolve());

  useEffect(() => {
    writingSettingsRef.current = writingSettings;
  }, [writingSettings]);

  const loadEnhancementOptions = async (aiEnabled: boolean) => {
    try {
      const options = await invoke<EnhancementOptions>("get_enhancement_options");
      let nextOptions = fromBackendOptions(options, aiEnabled);
      if (!aiEnabled && presetRequiresAiFormatting(nextOptions.preset)) {
        nextOptions = { preset: "PersonalDictation" };
      }
      setEnhancementOptions(nextOptions);
    } catch (error) {
      console.error("Failed to load enhancement options:", error);
    }
  };

  const loadWritingSettings = async () => {
    try {
      const nextSettings = await invoke<Partial<WritingSettings>>("get_writing_settings");
      setWritingSettings(mergeWritingSettings(nextSettings));
    } catch (error) {
      console.error("Failed to load writing settings:", error);
    }
  };

  const loadAISettings = useCallback(async () => {
    try {
      const allProviders = AI_PROVIDERS.map((provider) => provider.id);
      const keyStatus: Record<string, boolean> = {};

      await Promise.all(
        allProviders.map(async (providerId) => {
          const keyId = providerId;
          let isConfigured = await hasApiKey(keyId);

          if ((providerId === "custom" || providerId === "openai") && !isConfigured) {
            try {
              const providerSettings = await invoke<AISettings>(
                "get_ai_settings_for_provider",
                {
                  provider: providerId,
                },
              );
              isConfigured = providerSettings.hasApiKey;
            } catch (error) {
              console.error(`Failed to resolve ${providerId} provider readiness:`, error);
            }
          }

          keyStatus[providerId] = isConfigured;

          if (isConfigured) {
            try {
              const apiKey = await getApiKey(keyId);
              if (apiKey) {
                await invoke("cache_ai_api_key", {
                  args: { provider: providerId, apiKey },
                });
              }
            } catch (error) {
              console.error(`Failed to cache ${keyId} API key:`, error);
            }
          }
        }),
      );

      setProviderApiKeys(keyStatus);

      try {
        const customConfig = await invoke<{ baseUrl: string }>("get_openai_config");
        setOpenAIDefaultBaseUrl(customConfig.baseUrl || "https://api.openai.com/v1");
      } catch (error) {
        console.error("Failed to load custom config:", error);
      }

      const loadedAISettings = await invoke<AISettings>("get_ai_settings");
      if (loadedAISettings.provider === "custom") {
        setCustomModelName(loadedAISettings.model);
      }
      setAISettings(loadedAISettings);

      if (readiness?.ai_ready && loadedAISettings.provider) {
        setProviderApiKeys((prev) => ({
          ...prev,
          [loadedAISettings.provider]: true,
        }));
      }

      const providersWithKeys = allProviders.filter(
        (providerId) => keyStatus[providerId] && providerId !== "custom",
      );
      providersWithKeys.forEach((providerId) => {
        fetchModels(providerId);
      });

      return loadedAISettings;
    } catch (error) {
      console.error("Failed to load AI settings:", error);
      return null;
    }
  }, [readiness, fetchModels]);

  useEffect(() => {
    if (!settingsLoaded) {
      (async () => {
        const loadedAISettings = await loadAISettings();
        await loadEnhancementOptions(loadedAISettings?.enabled ?? false);
        await loadWritingSettings();
        setSettingsLoaded(true);
      })().catch((error) => {
        console.error("Failed to load formatting settings:", error);
        setSettingsLoaded(true);
      });
    }
  }, [settingsLoaded, loadAISettings]);

  useEffect(() => {
    const unlistenReady = listen("ai-ready", async () => {
      if (settingsLoaded) {
        await loadAISettings();
      }
    });

    const unlistenApiKey = listen("api-key-saved", async (event) => {
      const loadedAISettings = await invoke<AISettings>("get_ai_settings");
      setAISettings(loadedAISettings);

      const provider = (event.payload as { provider?: string }).provider;
      if (provider) {
        setProviderApiKeys((prev) => ({ ...prev, [provider]: true }));
      }
    });

    const unlistenApiKeyRemoved = listen<{ provider: string }>(
      "api-key-removed",
      async (event) => {
        let providerStillConfigured = false;

        if (event.payload.provider === "custom" || event.payload.provider === "openai") {
          try {
            const providerSettings = await invoke<AISettings>(
              "get_ai_settings_for_provider",
              {
                provider: event.payload.provider,
              },
            );
            providerStillConfigured = providerSettings.hasApiKey;
            setProviderApiKeys((prev) => ({
              ...prev,
              [event.payload.provider]: providerStillConfigured,
            }));
          } catch (error) {
            console.error(
              `Failed to refresh ${event.payload.provider} provider readiness after key removal:`,
              error,
            );
            setProviderApiKeys((prev) => ({
              ...prev,
              [event.payload.provider]: false,
            }));
          }
        } else {
          setProviderApiKeys((prev) => ({
            ...prev,
            [event.payload.provider]: false,
          }));
        }

        clearModels(event.payload.provider);

        const isCurrentProviderRemoved =
          aiSettings.provider === event.payload.provider && !providerStillConfigured;

        if (isCurrentProviderRemoved) {
          setAISettings((prev) => ({
            ...prev,
            enabled: false,
            provider: "",
            model: "",
            hasApiKey: false,
          }));

          await invoke("update_ai_settings", {
            enabled: false,
            provider: "",
            model: "",
          });
          setEnhancementOptions({ preset: "PersonalDictation" });
          try {
            await updateSettings({
              final_text_language: "same_as_transcript",
              transcription_task: "transcribe",
            });
          } catch (error) {
            console.error("Failed to refresh language settings after API key removal:", error);
          }
        }
      },
    );

    const unlistenFormattingError = listen<string>("formatting-error", async (event) => {
      const msg = event.payload || "Formatting failed";
      toast.error(typeof msg === "string" ? msg : "Formatting failed");
    });

    return () => {
      Promise.all([
        unlistenReady,
        unlistenApiKey,
        unlistenApiKeyRemoved,
        unlistenFormattingError,
      ]).then((fns) => {
        fns.forEach((fn) => fn());
      });
    };
  }, [settingsLoaded, aiSettings.provider, clearModels, updateSettings]);

  const persistEnhancementOptions = async (nextOptions: { preset: EnhancementPreset }) => {
    setEnhancementOptions(nextOptions);
    try {
      await invoke("update_enhancement_options", {
        options: toBackendOptions(nextOptions),
      });
    } catch (error) {
      const message = getErrorMessage(error, "Failed to save enhancement options");
      toast.error(message);
    }
  };

  const handlePresetChange = async (preset: typeof enhancementOptions.preset) => {
    if (presetRequiresAiFormatting(preset) && !aiSettings.enabled) {
      return;
    }
    if (
      preset === "PersonalDictation" &&
      settings?.final_text_language &&
      settings.final_text_language !== "same_as_transcript"
    ) {
      await handleFinalTextLanguageChange("same_as_transcript");
    }
    await persistEnhancementOptions({ preset });
  };

  const enqueueWritingSettingsSave = (rollbackSettings: WritingSettings) => {
    const generationAtEnqueue = writingSaveGeneration.current;
    writingSaveQueueRef.current = writingSaveQueueRef.current.then(async () => {
      const settingsToSave = writingSettingsRef.current;
      try {
        await invoke("update_writing_settings", { settings: settingsToSave });
      } catch (error) {
        if (writingSaveGeneration.current === generationAtEnqueue) {
          setWritingSettings(rollbackSettings);
          const message = getErrorMessage(error, "Failed to save writing settings");
          toast.error(message);
        }
      }
    });
  };

  const handleWritingSettingsChange = (nextSettings: WritingSettings) => {
    const rollbackSettings = writingSettingsRef.current;
    writingSaveGeneration.current += 1;
    setWritingSettings(nextSettings);
    writingSettingsRef.current = nextSettings;
    enqueueWritingSettingsSave(rollbackSettings);
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
      const message = getErrorMessage(error, "Failed to save final text language");
      toast.error(message);
    }
  };

  const handleToggleEnabled = async (enabled: boolean) => {
    const hasActiveProviderKey = Boolean(providerApiKeys[aiSettings.provider]);

    if (enabled && (!hasActiveProviderKey || !aiSettings.model)) {
      toast.error("Please select a provider, add an API key, and select a model first");
      return;
    }

    try {
      await invoke("update_ai_settings", {
        enabled,
        provider: aiSettings.provider,
        model: aiSettings.model,
      });

      setAISettings((prev) => ({ ...prev, enabled }));

      let nextPreset = enhancementOptions.preset;
      if (!enabled && presetRequiresAiFormatting(enhancementOptions.preset)) {
        nextPreset = "PersonalDictation";
      } else if (enabled && enhancementOptions.preset === "PersonalDictation") {
        nextPreset = "CleanDictation";
      }

      if (
        nextPreset === "PersonalDictation" &&
        settings?.final_text_language &&
        settings.final_text_language !== "same_as_transcript"
      ) {
        await handleFinalTextLanguageChange("same_as_transcript");
      }

      const presetChanged = nextPreset !== enhancementOptions.preset;
      if (presetChanged) {
        await persistEnhancementOptions({ preset: nextPreset });
      }

      if (!enabled && presetChanged) {
        toast.success("AI formatting disabled. Switched to Dictation (no AI).");
      } else {
        toast.success(enabled ? "AI formatting enabled" : "AI formatting disabled");
      }
    } catch (error) {
      const message = getErrorMessage(error, "Failed to update AI settings");
      toast.error(message);
    }
  };

  const handleSetupApiKey = async (providerId: string) => {
    setSelectedProvider(providerId);

    if (providerId === "custom") {
      try {
        const savedConfig = await invoke<{ baseUrl: string }>("get_openai_config");
        setOpenAIDefaultBaseUrl(savedConfig.baseUrl || "https://api.openai.com/v1");
      } catch (error) {
        console.error("Failed to load custom config:", error);
      }
      setShowOpenAIConfig(true);
    } else {
      setShowApiKeyModal(true);
    }
  };

  const handleApiKeySubmit = async (apiKey: string) => {
    setIsLoading(true);
    try {
      const trimmedKey = apiKey.trim();
      await saveApiKey(selectedProvider, trimmedKey);
      setProviderApiKeys((prev) => ({ ...prev, [selectedProvider]: true }));
      setAISettings((prev) => ({
        ...prev,
        provider: selectedProvider,
        hasApiKey: true,
      }));
      setShowApiKeyModal(false);
      toast.success("API key saved securely");
    } catch (error) {
      const message = getErrorMessage(error, "Failed to save API key");
      toast.error(message);
    } finally {
      setIsLoading(false);
    }
  };

  const handleRemoveApiKey = async (providerId: string) => {
    try {
      await removeApiKey(providerId);
      clearModels(providerId);
      toast.success("API key removed");
    } catch (error) {
      const message = getErrorMessage(error, "Failed to remove API key");
      toast.error(message);
    }
  };

  const handleSelectModel = async (providerId: string, modelId: string) => {
    try {
      const hasKey = providerApiKeys[providerId];
      const shouldEnable = hasKey ? aiSettings.enabled : false;

      await invoke("update_ai_settings", {
        enabled: shouldEnable,
        provider: providerId,
        model: modelId,
      });

      setAISettings((prev) => ({
        ...prev,
        enabled: shouldEnable,
        provider: providerId,
        model: modelId,
        hasApiKey: hasKey,
      }));

      toast.success("Model selected");
    } catch (error) {
      const message = getErrorMessage(error, "Failed to select model");
      toast.error(message);
    }
  };

  const hasAnyValidConfig = Object.values(providerApiKeys).some(Boolean);
  const isUsingCustomProvider = aiSettings.provider === "custom";
  const hasSelectedModel = Boolean(
    aiSettings.provider &&
      aiSettings.model &&
      (isUsingCustomProvider || providerApiKeys[aiSettings.provider]),
  );

  const storedFinalTextLanguage = settings?.final_text_language ?? "same_as_transcript";
  const effectiveFinalTextLanguage =
    enhancementOptions.preset === "PersonalDictation"
      ? "same_as_transcript"
      : storedFinalTextLanguage;

  const activeModelName = isUsingCustomProvider
    ? customModelName
    : getModels(aiSettings.provider).find((model) => model.id === aiSettings.model)?.name ||
      humanizeModelId(aiSettings.model);

  const hasLoadingProviders = AI_PROVIDERS.some((provider) => isModelsLoading(provider.id));

  return (
    <div className="h-full min-h-0 flex flex-col">
      <div className="shrink-0 border-b border-border/40 px-6 py-4">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h1 className="text-2xl font-semibold">Formatting</h1>
              <Dialog>
                <DialogTrigger asChild>
                  <Button type="button" variant="secondary" size="icon" aria-label="Formatting guide" className="rounded-full">
                    <HelpCircle className="h-4.5 w-4.5" />
                  </Button>
                </DialogTrigger>
                <DialogContent className="sm:max-w-lg">
                  <DialogHeader>
                    <DialogTitle>Formatting guide</DialogTitle>
                    <DialogDescription>
                      Use modes for the shape of the final text. Corrections, words & names, text
                      shortcuts, and language rules still apply before optional AI cleanup.
                    </DialogDescription>
                  </DialogHeader>
                  <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                    <p><strong className="text-foreground">Setup</strong> works in order: set up one provider, save its API key, select a model, then turn on AI formatting when you want language conversion or heavier cleanup.</p>
                    <p><strong className="text-foreground">Personal Dictation</strong> transcribes with local mechanical cleanup and Personal Library rules, without semantic AI rewriting.</p>
                    <p><strong className="text-foreground">Clean Dictation</strong> fixes punctuation and grammar without changing intent.</p>
                    <p><strong className="text-foreground">Writing</strong> polishes dictated text into clear prose.</p>
                    <p><strong className="text-foreground">Notes</strong> organizes speech into concise structured notes.</p>
                    <p><strong className="text-foreground">Message</strong> formats a concise message for chat or email.</p>
                    <p><strong className="text-foreground">Code</strong> creates conventional commits and code annotations.</p>
                  </div>
                </DialogContent>
              </Dialog>
            </div>
            <p className="mt-1 text-sm text-muted-foreground">
              Premium writing cleanup with private local-first controls.
            </p>
          </div>
          <Field orientation="horizontal" className="w-auto items-center gap-3 rounded-lg border border-border/60 bg-card px-3 py-1.5">
            <FieldTitle className="text-sm">AI formatting</FieldTitle>
            <Switch
              id="ai-formatting"
              aria-label="AI formatting"
              checked={aiSettings.enabled}
              onCheckedChange={handleToggleEnabled}
              disabled={!hasAnyValidConfig || !hasSelectedModel}
            />
          </Field>
        </div>
      </div>

      <ScrollArea className="flex-1 min-h-0">
        <div className="space-y-5 p-6">
          <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
            <div className="mb-3 flex items-center justify-between gap-3">
              <FieldLegend className="mb-0 text-sm">AI Providers</FieldLegend>
              <div className="flex items-center gap-3 text-xs text-muted-foreground">
                {hasLoadingProviders && (
                  <span className="inline-flex items-center gap-1.5">
                    <Spinner className="h-3.5 w-3.5" />
                    Refreshing models
                  </span>
                )}
                {activeModelName && aiSettings.enabled && (
                  <span>
                    Active model: <span className="text-foreground">{activeModelName}</span>
                  </span>
                )}
              </div>
            </div>

            <FieldGroup className="gap-3">
              {AI_PROVIDERS.map((provider) => {
                const isCustomActive = Boolean(
                  provider.isCustom &&
                    aiSettings.provider === "custom" &&
                    providerApiKeys.custom &&
                    aiSettings.enabled,
                );
                const isActive = provider.isCustom
                  ? isCustomActive
                  : Boolean(aiSettings.provider === provider.id && aiSettings.enabled);

                return (
                  <ProviderCard
                    key={provider.id}
                    provider={provider}
                    hasApiKey={providerApiKeys[provider.id] || false}
                    isActive={isActive}
                    selectedModel={
                      provider.isCustom
                        ? isCustomActive
                          ? customModelName
                          : null
                        : aiSettings.provider === provider.id
                          ? aiSettings.model
                          : null
                    }
                    onSetupApiKey={() => handleSetupApiKey(provider.id)}
                    onRemoveApiKey={() => handleRemoveApiKey(provider.id)}
                    onSelectModel={(modelId) => handleSelectModel(provider.id, modelId)}
                    models={getModels(provider.id)}
                    modelsLoading={isModelsLoading(provider.id)}
                    modelsError={getError(provider.id)}
                    onRefreshModels={() => fetchModels(provider.id)}
                    customModelName={provider.isCustom ? customModelName : undefined}
                  />
                );
              })}
            </FieldGroup>
          </FieldSet>

          <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
            <FieldLegend className="mb-1 text-sm">Formatting Options</FieldLegend>
            <FieldDescription className="mb-3">
              Configure output style, language targeting, and deterministic corrections.
            </FieldDescription>

            {!aiSettings.enabled &&
              effectiveFinalTextLanguage !== "same_as_transcript" && (
                <div className="mb-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-300">
                  Final text language changes beyond the raw transcript require AI formatting to be enabled.
                </div>
              )}

            <EnhancementSettings
              preset={enhancementOptions.preset}
              finalTextLanguage={effectiveFinalTextLanguage}
              writingSettings={writingSettings}
              aiFormattingEnabled={aiSettings.enabled}
              onPresetChange={handlePresetChange}
              onFinalTextLanguageChange={handleFinalTextLanguageChange}
              onWritingSettingsChange={handleWritingSettingsChange}
            />
          </FieldSet>

        </div>
      </ScrollArea>

      <ApiKeyModal
        isOpen={showApiKeyModal}
        onClose={() => setShowApiKeyModal(false)}
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

            await invoke("set_openai_config", { args: { baseUrl: trimmedBase } });

            if (trimmedKey) {
              await saveApiKey("custom", trimmedKey);
            }

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
            }));
            setProviderApiKeys((prev) => ({ ...prev, custom: true }));

            toast.success("Custom provider configured");
            setShowOpenAIConfig(false);
          } catch (error) {
            const message = getErrorMessage(error, "Failed to save configuration");
            toast.error(message);
          } finally {
            setIsLoading(false);
          }
        }}
      />
    </div>
  );
}
