import { ApiKeyModal } from "@/components/ApiKeyModal";
import { EnhancementSettings } from "@/components/EnhancementSettings";
import { OpenAICompatConfigModal } from "@/components/OpenAICompatConfigModal";
import { Badge } from "@/components/ui/badge";
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
  FieldGroup,
  FieldLegend,
  FieldSet,
  FieldTitle,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import type { AISettings, EnhancementOptions, EnhancementPreset } from "@/types/ai";
import {
  fromBackendOptions,
  presetRequiresAiFormatting,
  toBackendOptions,
} from "@/types/ai";
import type { WritingSettings } from "@/types/writing";
import { defaultWritingSettings, mergeWritingSettings } from "@/types/writing";
import type { AiProvider, AIProviderConfig, AIProviderModel } from "@/types/providers";
import { toProviderConfig } from "@/types/providers";
import { useAllProviderModels } from "@/hooks/useProviderModels";
import { hasApiKey, removeApiKey, saveApiKey, getApiKey } from "@/utils/keyring";
import { getErrorMessage } from "@/utils/error";
import { useReadinessState } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { humanizeModelId } from "@/lib/model-display";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState, useCallback, useRef } from "react";
import { toast } from "sonner";
import { Check, ExternalLink, HelpCircle, Key, Loader2, RefreshCw, Search, Settings2, Star, Trash2 } from "lucide-react";
import { createLogger } from "@/lib/logger";

const log = createLogger("enhancements");


type AISettingsResponse = Omit<AISettings, "modelsByProvider"> & {
  modelsByProvider?: Record<string, string>;
  aiModelNeedsReselection?: boolean;
};

const normalizeAISettings = (settings: AISettingsResponse): AISettings => ({
  ...settings,
  modelsByProvider: settings.modelsByProvider ?? {},
});

const providerSupportsReasoning = (provider: AIProviderConfig) =>
  provider.supports_reasoning ?? provider.supportsReasoning ?? false;

const formatModelCost = (model: AIProviderModel) => {
  if (model.costInput == null && model.costOutput == null) {
    return null;
  }
  const input = model.costInput == null ? "?" : `$${model.costInput}`;
  const output = model.costOutput == null ? "?" : `$${model.costOutput}`;
  return `${input}/${output}`;
};

const modelMatchesQuery = (model: AIProviderModel, query: string) =>
  model.id.toLowerCase().includes(query) || model.name.toLowerCase().includes(query);

type EnhancementsView = "ai" | "rules" | "all";

export function EnhancementsSection({ view = "all" }: { view?: EnhancementsView } = {}) {
  const readiness = useReadinessState();
  const { settings, updateSettings } = useSettings();
  const { fetchModels, getModels, isLoading: isModelsLoading, getError, clearModels } =
    useAllProviderModels();

  const [aiSettings, setAISettings] = useState<AISettings>({
    enabled: false,
    provider: "",
    model: "",
    hasApiKey: false,
    modelsByProvider: {},
  });
  const [aiModelNeedsReselection, setAiModelNeedsReselection] = useState(false);

  const [providers, setProviders] = useState<AIProviderConfig[]>([]);

  const [providerSearch, setProviderSearch] = useState("");
  const [showAdvancedProviders, setShowAdvancedProviders] = useState(false);

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
  const enhancementSaveGeneration = useRef(0);
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
      log.error("Failed to load enhancement options:", error);
    }
  };

  const loadWritingSettings = async () => {
    try {
      const nextSettings = await invoke<Partial<WritingSettings>>("get_writing_settings");
      setWritingSettings(mergeWritingSettings(nextSettings));
      return true;
    } catch (error) {
      log.error("Failed to load writing settings:", error);
      return false;
    }
  };

  const loadAISettings = useCallback(async () => {
    try {
      const listedProviders = (await invoke<AiProvider[]>("list_ai_providers")).map(
        toProviderConfig,
      );
      setProviders(listedProviders);
      const allProviders = listedProviders.map((provider) => provider.id);
      const keyStatus: Record<string, boolean> = {};

      await Promise.all(
        allProviders.map(async (providerId) => {
          const keyId = providerId;
          let isConfigured = await hasApiKey(keyId);

          if ((providerId === "custom" || providerId === "openai") && !isConfigured) {
            try {
              const providerSettings = normalizeAISettings(
                await invoke<AISettingsResponse>("get_ai_settings_for_provider", {
                  provider: providerId,
                }),
              );
              isConfigured = providerSettings.hasApiKey;
            } catch (error) {
              log.error(`Failed to resolve ${providerId} provider readiness:`, error);
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
              log.error(`Failed to cache ${keyId} API key:`, error);
            }
          }
        }),
      );

      setProviderApiKeys(keyStatus);

      try {
        const customConfig = await invoke<{ baseUrl: string }>("get_openai_config");
        setOpenAIDefaultBaseUrl(customConfig.baseUrl || "https://api.openai.com/v1");
      } catch (error) {
        log.error("Failed to load custom config:", error);
      }

      const loadedAISettingsResponse = await invoke<AISettingsResponse>("get_ai_settings");
      const loadedAISettings = normalizeAISettings(loadedAISettingsResponse);
      const customModel =
        loadedAISettings.modelsByProvider.custom ||
        (loadedAISettings.provider === "custom" ? loadedAISettings.model : "");
      if (customModel) {
        setCustomModelName(customModel);
      }
      setAiModelNeedsReselection(Boolean(loadedAISettingsResponse.aiModelNeedsReselection));
      setAISettings(loadedAISettings);

      if (readiness?.ai_ready && loadedAISettings.provider) {
        setProviderApiKeys((prev) => ({
          ...prev,
          [loadedAISettings.provider]: true,
        }));
      }

      listedProviders
        .filter((provider) => !provider.isCustom)
        .forEach((provider) => {
          fetchModels(provider.id);
        });

      return loadedAISettings;
    } catch (error) {
      log.error("Failed to load AI settings:", error);
      return null;
    }
  }, [readiness, fetchModels]);

  useEffect(() => {
    if (!settingsLoaded) {
      (async () => {
        const loadedAISettings = await loadAISettings();
        await loadEnhancementOptions(loadedAISettings?.enabled ?? false);
        const writingSettingsLoaded = await loadWritingSettings();
        setSettingsLoaded(writingSettingsLoaded);
      })().catch((error) => {
        log.error("Failed to load formatting settings:", error);
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
      const loadedAISettings = normalizeAISettings(
        await invoke<AISettingsResponse>("get_ai_settings"),
      );
      const provider = (event.payload as { provider?: string }).provider;
      if (!provider || provider === loadedAISettings.provider) {
        setAISettings(loadedAISettings);
      } else {
        const rememberedModel = loadedAISettings.modelsByProvider[provider] || "";
        setAISettings({
          ...loadedAISettings,
          provider,
          enabled: false,
          model: rememberedModel,
          hasApiKey: true,
        });
      }

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
            const providerSettings = normalizeAISettings(
              await invoke<AISettingsResponse>("get_ai_settings_for_provider", {
                provider: event.payload.provider,
              }),
            );
            providerStillConfigured = providerSettings.hasApiKey;
            setProviderApiKeys((prev) => ({
              ...prev,
              [event.payload.provider]: providerStillConfigured,
            }));
          } catch (error) {
            log.error(`Failed to refresh ${event.payload.provider} provider readiness after key removal:`, error);
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
            log.error("Failed to refresh language settings after API key removal:", error);
          }
        }
      },
    );

    const unlistenFormattingError = listen<string>("formatting-error", async (event) => {
      const msg = event.payload || "Formatting failed";
      toast.error(typeof msg === "string" ? msg : "Formatting failed");
    });

    const unlistenAiEnabledChanged = listen<boolean>("ai-enabled-changed", (event) => {
      setAISettings((prev) => ({ ...prev, enabled: event.payload }));
    });

    return () => {
      Promise.all([
        unlistenReady,
        unlistenApiKey,
        unlistenApiKeyRemoved,
        unlistenFormattingError,
        unlistenAiEnabledChanged,
      ]).then((fns) => {
        fns.forEach((fn) => fn());
      });
    };
  }, [settingsLoaded, aiSettings.provider, clearModels, updateSettings]);

  const persistEnhancementOptions = async (nextOptions: { preset: EnhancementPreset }) => {
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

  const enqueueWritingSettingsSave = (
    settingsToSave: WritingSettings,
    rollbackSettings: WritingSettings,
    generationAtEnqueue: number,
  ) => {
    writingSaveQueueRef.current = writingSaveQueueRef.current.then(async () => {
      try {
        await invoke("update_writing_settings", { settings: settingsToSave });
      } catch (error) {
        if (writingSaveGeneration.current === generationAtEnqueue) {
          setWritingSettings(rollbackSettings);
          writingSettingsRef.current = rollbackSettings;
          const message = getErrorMessage(error, "Failed to save writing settings");
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
      enqueueWritingSettingsSave(nextSettings, rollbackSettings, generationAtEnqueue);
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
        toast.success("AI formatting disabled. Switched to Personal Dictation.");
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
        const [savedConfig, providerSettingsResponse] = await Promise.all([
          invoke<{ baseUrl: string }>("get_openai_config"),
          invoke<AISettingsResponse>("get_ai_settings_for_provider", {
            provider: providerId,
          }),
        ]);
        const providerSettings = normalizeAISettings(providerSettingsResponse);
        setOpenAIDefaultBaseUrl(savedConfig.baseUrl || "https://api.openai.com/v1");
        if (providerSettings.model) {
          setCustomModelName(providerSettings.model);
        }
      } catch (error) {
        log.error("Failed to load custom config:", error);
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
      const providerSettings = normalizeAISettings(
        await invoke<AISettingsResponse>("get_ai_settings_for_provider", {
          provider: selectedProvider,
        }),
      );
      const rememberedModel = providerSettings.model || "";
      setProviderApiKeys((prev) => ({ ...prev, [selectedProvider]: true }));
      setAISettings((prev) => ({
        ...prev,
        provider: selectedProvider,
        enabled: prev.provider === selectedProvider ? prev.enabled : false,
        model: rememberedModel,
        hasApiKey: true,
        modelsByProvider: providerSettings.modelsByProvider,
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
        modelsByProvider: {
          ...prev.modelsByProvider,
          [providerId]: modelId,
        },
      }));
      setAiModelNeedsReselection(false);

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

  const showAiModelReselectionNotice =
    aiModelNeedsReselection && aiSettings.enabled && !aiSettings.model;

  const storedFinalTextLanguage = settings?.final_text_language ?? "same_as_transcript";
  const effectiveFinalTextLanguage =
    enhancementOptions.preset === "PersonalDictation"
      ? "same_as_transcript"
      : storedFinalTextLanguage;

  const activeModelName = isUsingCustomProvider
    ? customModelName
    : getModels(aiSettings.provider).find((model) => model.id === aiSettings.model)?.name ||
      humanizeModelId(aiSettings.model);

  const visibleProviders = useMemo(
    () => providers.filter((provider) => showAdvancedProviders || provider.status !== "hidden"),
    [providers, showAdvancedProviders],
  );
  const hasHiddenProviders = useMemo(
    () => providers.some((provider) => provider.status === "hidden"),
    [providers],
  );
  const providerQuery = providerSearch.trim().toLowerCase();
  const filteredProviders = useMemo(() => {
    if (!providerQuery) {
      return visibleProviders;
    }

    return visibleProviders.filter((provider) => {
      const providerMatches = provider.name.toLowerCase().includes(providerQuery);
      const customModelMatches =
        provider.isCustom && customModelName.toLowerCase().includes(providerQuery);
      const modelsMatch = getModels(provider.id).some((model) =>
        modelMatchesQuery(model, providerQuery),
      );
      return providerMatches || customModelMatches || modelsMatch;
    });
  }, [customModelName, getModels, providerQuery, visibleProviders]);

  const hasLoadingProviders = providers.some((provider) => isModelsLoading(provider.id));

  return (
    <div className="h-full min-h-0 flex flex-col">
      <div className="shrink-0 border-b border-border/40 px-6 py-4">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h1 className="text-2xl font-semibold tracking-tight">{view === "rules" ? "Pre-AI Formatting" : view === "ai" ? "AI Formatting" : "Formatting"}</h1>
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
                    Modes shape the final text. Your text rules (corrections, words & names, voice
                    commands, shortcuts) always run first.
                  </DialogDescription>
                  </DialogHeader>
                  <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                    <p><strong className="text-foreground">Setup</strong> works in order: set up one provider, save its API key, select a model, then turn on AI formatting when you want language conversion or heavier cleanup.</p>
                    <p><strong className="text-foreground">Personal Dictation</strong> is no AI. Just transcription with local cleanup and your text rules.</p>
                    <p><strong className="text-foreground">Clean Dictation</strong> uses AI to fix grammar and punctuation. Keeps your meaning.</p>
                    <p><strong className="text-foreground">Writing</strong> uses AI to polish it into clear prose.</p>
                    <p><strong className="text-foreground">Notes</strong> uses AI to turn it into short, structured notes.</p>
                    <p><strong className="text-foreground">Message</strong> uses AI to format a short message.</p>
                    <p><strong className="text-foreground">Code</strong> uses AI to format commits and code notes.</p>
                  </div>
                </DialogContent>
              </Dialog>
            </div>
          </div>
          {view !== "rules" && (
          <div className="flex flex-col items-end gap-1">
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
            {!aiSettings.enabled && (!hasAnyValidConfig || !hasSelectedModel) && (
              <p className="max-w-56 text-right text-xs text-muted-foreground">
                Add an API key and choose a model below to turn on AI formatting.
              </p>
            )}
          </div>
          )}
        </div>
      </div>

      <ScrollArea className="flex-1 min-h-0">
        <div className="space-y-5 p-6">
          {view !== "rules" && (
          <>
          <header>
            <h2 className="text-base font-semibold">AI polish (optional)</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              Rewrites your words for meaning and format. Needs a provider. Off by default.
            </p>
          </header>
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
                {activeModelName && (
                  <span>
                    {aiSettings.enabled ? "Active model" : "Selected model"}:{" "}
                    <span className="text-foreground">{activeModelName}</span>
                    {!aiSettings.enabled && " (AI formatting off)"}
                  </span>
                )}
              </div>
            </div>

            {showAiModelReselectionNotice && (
              <div className="mb-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-300">
                Your previously selected AI model is no longer available. Please choose a model to
                continue using AI polish.
              </div>
            )}

            <FieldGroup className="gap-3">
              <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <div className="relative flex-1">
                  <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                  <Input
                    id="ai-provider-model-search"
                    aria-label="Search providers and models"
                    value={providerSearch}
                    onChange={(event) => setProviderSearch(event.target.value)}
                    placeholder="Search providers or models"
                    className="pl-9"
                  />
                </div>
                {hasHiddenProviders && (
                  <Field orientation="horizontal" className="w-auto items-center gap-2">
                    <FieldTitle className="text-sm">Advanced</FieldTitle>
                    <Switch
                      id="advanced-ai-providers"
                      aria-label="Show advanced AI providers"
                      checked={showAdvancedProviders}
                      onCheckedChange={setShowAdvancedProviders}
                    />
                  </Field>
                )}
              </div>

              {filteredProviders.length === 0 && (
                <div className="rounded-lg border border-dashed border-border/70 px-4 py-6 text-center text-sm text-muted-foreground">
                  No providers or models match your search.
                </div>
              )}

              {filteredProviders.map((provider) => {
                const hasKey = providerApiKeys[provider.id] || false;
                const isCustomActive = Boolean(
                  provider.isCustom &&
                    aiSettings.provider === "custom" &&
                    providerApiKeys.custom &&
                    aiSettings.enabled,
                );
                const isActive = provider.isCustom
                  ? isCustomActive
                  : Boolean(aiSettings.provider === provider.id && aiSettings.enabled);
                const selectedModel = provider.isCustom
                  ? aiSettings.modelsByProvider.custom || customModelName || null
                  : aiSettings.modelsByProvider[provider.id] ||
                    (aiSettings.provider === provider.id ? aiSettings.model : null);
                const models = getModels(provider.id);
                const providerMatches = provider.name.toLowerCase().includes(providerQuery);
                const displayModels =
                  providerQuery && !providerMatches
                    ? models.filter((model) => modelMatchesQuery(model, providerQuery))
                    : models;
                const recommendedModels = displayModels.filter((model) => model.recommended);
                const allModels = displayModels.filter((model) => !model.recommended);
                const selectedModelData = models.find((model) => model.id === selectedModel);
                const showModelPicker = !provider.isCustom && (hasKey || Boolean(providerQuery));
                const modelGroups = ([
                  ["Recommended", recommendedModels],
                  ["All", allModels],
                ] satisfies Array<[string, AIProviderModel[]]>).filter(
                  ([, groupModels]) => groupModels.length > 0,
                );

                return (
                  <div
                    key={provider.id}
                    className={`rounded-xl border border-border/60 bg-background p-4 transition-all ${
                      isActive ? "border-primary/50 bg-primary/5" : ""
                    }`}
                  >
                    <div className="flex items-start justify-between gap-4">
                      <div className="min-w-0 flex-1">
                        <div className="mb-1 flex flex-wrap items-center gap-2">
                          <h3 className={`font-semibold ${provider.color}`}>{provider.name}</h3>
                          {provider.status === "experimental" && (
                            <Badge variant="outline" className="border-amber-500/40 text-amber-700 dark:text-amber-300">
                              Experimental
                            </Badge>
                          )}
                          {provider.status === "hidden" && (
                            <Badge variant="outline">Advanced</Badge>
                          )}
                          {providerSupportsReasoning(provider) && (
                            <Badge variant="secondary">Reasoning</Badge>
                          )}
                          {isActive && (
                            <span className="rounded-full bg-primary/10 px-2 py-0.5 text-xs text-primary">
                              Active
                            </span>
                          )}
                        </div>

                        {provider.isCustom && hasKey && customModelName && (
                          <p className="text-sm text-muted-foreground">
                            Model: <span className="text-foreground">{customModelName}</span>
                          </p>
                        )}
                        {!hasKey && (
                          <p className="text-sm text-muted-foreground">
                            {provider.isCustom ? "Configure endpoint to enable" : "Add API key to enable"}
                          </p>
                        )}
                        {showModelPicker && selectedModel && (
                          <p className="text-sm text-muted-foreground">
                            Selected model:{" "}
                            <span className="text-foreground">
                              {selectedModelData?.name || humanizeModelId(selectedModel)}
                            </span>
                          </p>
                        )}
                      </div>

                      <div className="flex items-center gap-2">
                        {hasKey ? (
                          <>
                            {provider.isCustom && (
                              <Button onClick={() => handleSetupApiKey(provider.id)} variant="ghost" size="sm">
                                <Settings2 className="h-3.5 w-3.5" />
                              </Button>
                            )}
                            {!provider.isCustom && (
                              <Button
                                onClick={() => fetchModels(provider.id)}
                                variant="ghost"
                                size="sm"
                                className="text-muted-foreground"
                                disabled={isModelsLoading(provider.id)}
                                title={`Refresh ${provider.name} models`}
                              >
                                <RefreshCw className={`h-3.5 w-3.5 ${isModelsLoading(provider.id) ? "animate-spin" : ""}`} />
                              </Button>
                            )}
                            <Button
                              onClick={async () => {
                                const message = provider.isCustom
                                  ? `Remove configuration for ${provider.name}?`
                                  : `Remove API key for ${provider.name}?`;
                                const confirmed = await ask(message, {
                                  title: provider.isCustom ? "Remove Configuration" : "Remove API Key",
                                  kind: "warning",
                                });
                                if (confirmed) {
                                  handleRemoveApiKey(provider.id);
                                }
                              }}
                              variant="ghost"
                              size="sm"
                              className="text-muted-foreground hover:text-destructive"
                            >
                              <Trash2 className="h-3.5 w-3.5" />
                            </Button>
                          </>
                        ) : (
                          <>
                            {!provider.isCustom && provider.apiKeyUrl && (
                              <Button
                                onClick={() => window.open(provider.apiKeyUrl, "_blank")}
                                variant="ghost"
                                size="sm"
                                className="text-muted-foreground"
                                title={`Get ${provider.name} API Key`}
                              >
                                <ExternalLink className="h-3.5 w-3.5" />
                              </Button>
                            )}
                            <Button onClick={() => handleSetupApiKey(provider.id)} variant="outline" size="sm">
                              {provider.isCustom ? (
                                <>
                                  <Settings2 className="mr-1.5 h-3.5 w-3.5" />
                                  Configure
                                </>
                              ) : (
                                <>
                                  <Key className="mr-1.5 h-3.5 w-3.5" />
                                  Add Key
                                </>
                              )}
                            </Button>
                          </>
                        )}
                      </div>
                    </div>

                    {showModelPicker && (
                      <div className="mt-3 space-y-3 border-t border-border/50 pt-3">
                        {isModelsLoading(provider.id) && models.length === 0 && (
                          <div className="flex items-center gap-2 text-sm text-muted-foreground">
                            <Loader2 className="h-4 w-4 animate-spin" />
                            Loading models...
                          </div>
                        )}
                        {getError(provider.id) && (
                          <div className="flex items-center justify-between gap-3 rounded-md border border-destructive/30 px-3 py-2 text-sm text-destructive">
                            <span>{getError(provider.id)}</span>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => fetchModels(provider.id)}
                              disabled={isModelsLoading(provider.id)}
                            >
                              Retry
                            </Button>
                          </div>
                        )}
                        {!isModelsLoading(provider.id) && !getError(provider.id) && modelGroups.length === 0 && (
                          <p className="text-sm text-muted-foreground">No models available</p>
                        )}
                        {modelGroups.map(([label, groupModels]) => (
                          <div key={`${provider.id}-${label}`} className="space-y-1.5">
                            <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
                              {label}
                            </p>
                            <div className="grid gap-1.5 md:grid-cols-2">
                              {groupModels.map((model) => {
                                const cost = formatModelCost(model);
                                return (
                                  <Button
                                    key={model.id}
                                    type="button"
                                    variant={selectedModel === model.id ? "secondary" : "ghost"}
                                    className="h-auto justify-start px-3 py-2 text-left"
                                    onClick={() => handleSelectModel(provider.id, model.id)}
                                    disabled={!hasKey}
                                    title={hasKey ? undefined : `Add a ${provider.name} API key to select this model`}
                                  >
                                    <span className="flex min-w-0 flex-1 items-start gap-2">
                                      {model.recommended && (
                                        <Star className="mt-0.5 h-3.5 w-3.5 shrink-0 fill-amber-500 text-amber-500" />
                                      )}
                                      <span className="min-w-0">
                                        <span className="block truncate font-medium">{model.name}</span>
                                        <span className="block truncate text-xs text-muted-foreground">{model.id}</span>
                                      </span>
                                    </span>
                                    <span className="ml-2 flex shrink-0 items-center gap-1">
                                      {model.reasoning && (
                                        <Badge variant="secondary" className="h-4 px-1.5 text-[10px]">
                                          Reasoning
                                        </Badge>
                                      )}
                                      {cost && (
                                        <Badge variant="outline" className="h-4 px-1.5 text-[10px]">
                                          {cost}
                                        </Badge>
                                      )}
                                      {selectedModel === model.id && (
                                        <Check className="h-3.5 w-3.5 text-primary" />
                                      )}
                                    </span>
                                  </Button>
                                );
                              })}
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                );
              })}
            </FieldGroup>
          </FieldSet>
          </>
          )}

          <EnhancementSettings
            view={view}
            preset={enhancementOptions.preset}
            finalTextLanguage={effectiveFinalTextLanguage}
            writingSettings={writingSettings}
            aiFormattingEnabled={aiSettings.enabled}
            onPresetChange={handlePresetChange}
            onFinalTextLanguageChange={handleFinalTextLanguageChange}
            onWritingSettingsChange={handleWritingSettingsChange}
            writingSettingsDisabled={!settingsLoaded}
          />

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
