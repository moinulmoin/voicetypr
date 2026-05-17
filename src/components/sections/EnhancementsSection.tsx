import { ApiKeyModal } from "@/components/ApiKeyModal";
import { EnhancementSettings } from "@/components/EnhancementSettings";
import { OpenAICompatConfigModal } from "@/components/OpenAICompatConfigModal";
import { ProviderCard } from "@/components/ProviderCard";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import type { EnhancementOptions } from "@/types/ai";
import { fromBackendOptions, toBackendOptions } from "@/types/ai";
import type { WritingSettings } from "@/types/writing";
import { defaultWritingSettings } from "@/types/writing";
import { AI_PROVIDERS } from "@/types/providers";
import { useAllProviderModels } from "@/hooks/useProviderModels";
import { hasApiKey, removeApiKey, saveApiKey, getApiKey } from "@/utils/keyring";
import { getErrorMessage } from "@/utils/error";
import { useReadinessState } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState, useCallback, useRef } from "react";
import { toast } from "sonner";
import { Info } from "lucide-react";

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
    preset: "Default" | "Prompts" | "Email" | "Commit";
  }>({
    preset: "Default",
  });
  const [writingSettings, setWritingSettings] =
    useState<WritingSettings>(defaultWritingSettings);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const writingSaveGeneration = useRef(0);

  const loadEnhancementOptions = async () => {
    try {
      const options = await invoke<EnhancementOptions>("get_enhancement_options");
      setEnhancementOptions(fromBackendOptions(options));
    } catch (error) {
      console.error("Failed to load enhancement options:", error);
    }
  };

  const loadWritingSettings = async () => {
    try {
      const nextSettings = await invoke<WritingSettings>("get_writing_settings");
      setWritingSettings(nextSettings);
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
    } catch (error) {
      console.error("Failed to load AI settings:", error);
    }
  }, [readiness, fetchModels]);

  useEffect(() => {
    if (!settingsLoaded) {
      loadAISettings();
      loadEnhancementOptions();
      loadWritingSettings();
      setSettingsLoaded(true);
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
  }, [settingsLoaded, aiSettings.provider, clearModels]);

  const handlePresetChange = async (preset: typeof enhancementOptions.preset) => {
    const nextOptions = { preset };
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

  const handleWritingSettingsChange = async (nextSettings: WritingSettings) => {
    const previousSettings = writingSettings;
    const saveGeneration = writingSaveGeneration.current + 1;
    writingSaveGeneration.current = saveGeneration;
    setWritingSettings(nextSettings);
    try {
      await invoke("update_writing_settings", { settings: nextSettings });
    } catch (error) {
      if (writingSaveGeneration.current === saveGeneration) {
        setWritingSettings(previousSettings);
      }
      const message = getErrorMessage(error, "Failed to save writing settings");
      toast.error(message);
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
      toast.success(enabled ? "AI formatting enabled" : "AI formatting disabled");
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

  const activeModelName = isUsingCustomProvider
    ? customModelName
    : getModels(aiSettings.provider).find((model) => model.id === aiSettings.model)?.name ||
      aiSettings.model;

  return (
    <div className="h-full min-h-0 flex flex-col">
      <div className="shrink-0 px-6 py-4 border-b border-border/40">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold">Formatting</h1>
            <p className="text-sm text-muted-foreground mt-1">
              AI-powered text formatting and enhancement
            </p>
          </div>
          <div className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-card border border-border/50">
            <Label htmlFor="ai-formatting" className="text-sm font-medium cursor-pointer">
              AI Formatting
            </Label>
            <Switch
              id="ai-formatting"
              checked={aiSettings.enabled}
              onCheckedChange={handleToggleEnabled}
              disabled={!hasAnyValidConfig || !hasSelectedModel}
            />
          </div>
        </div>
      </div>

      <ScrollArea className="flex-1 min-h-0">
        <div className="p-6 space-y-6">
          <div className="space-y-4">
            <div className="flex items-center gap-2">
              <h2 className="text-base font-semibold">AI Providers</h2>
              <div className="h-px bg-border/50 flex-1" />
              {activeModelName && aiSettings.enabled && (
                <span className="text-sm text-muted-foreground">
                  Active: <span className="text-amber-600 dark:text-amber-500">{activeModelName}</span>
                </span>
              )}
            </div>

            <div className="grid gap-3">
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
            </div>
          </div>

          <div className="space-y-4">
            <div className="flex items-center gap-2">
              <h2 className="text-base font-semibold">Formatting Options</h2>
              <div className="h-px bg-border/50 flex-1" />
            </div>

            <div className="space-y-3">
              {!aiSettings.enabled &&
                settings?.final_text_language &&
                settings.final_text_language !== "same_as_transcript" && (
                  <div className="rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-300">
                    Final text language changes beyond the raw transcript require AI formatting to be enabled.
                  </div>
                )}
              <EnhancementSettings
                preset={enhancementOptions.preset}
                finalTextLanguage={settings?.final_text_language ?? "same_as_transcript"}
                writingSettings={writingSettings}
                onPresetChange={handlePresetChange}
                onFinalTextLanguageChange={handleFinalTextLanguageChange}
                onWritingSettingsChange={handleWritingSettingsChange}
              />
            </div>
          </div>

          {!aiSettings.enabled && (
            <div className="rounded-lg border border-border/50 bg-card p-4">
              <div className="flex items-start gap-3">
                <div className="p-1.5 rounded-md bg-amber-500/10">
                  <Info className="h-4 w-4 text-amber-500" />
                </div>
                <div className="space-y-2 flex-1">
                  <h3 className="font-medium text-sm">Quick Setup</h3>
                  <ol className="text-sm text-muted-foreground space-y-1.5 list-decimal list-inside">
                    <li>Choose a provider above (OpenAI, Anthropic, or Google)</li>
                    <li>Click "Add Key" and enter your API key</li>
                    <li>Select a model from the dropdown</li>
                    <li>Toggle "AI Formatting" on to enable</li>
                  </ol>
                  <p className="text-xs text-muted-foreground mt-3">
                    AI formatting improves punctuation, grammar, style, language conversion, and context-aware cleanup. Replacements, dictionary words, and snippets still save even if AI is off.
                  </p>
                </div>
              </div>
            </div>
          )}
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

            const nextEnabled = aiSettings.enabled || !aiSettings.model;
            await invoke("update_ai_settings", {
              enabled: nextEnabled,
              provider: "custom",
              model: trimmedModel,
            });

            setCustomModelName(trimmedModel);
            setOpenAIDefaultBaseUrl(trimmedBase);
            setAISettings((prev) => ({
              ...prev,
              enabled: nextEnabled,
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
