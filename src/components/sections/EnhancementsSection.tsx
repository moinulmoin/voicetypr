import { ApiKeyModal } from "@/components/ApiKeyModal";
import { EnhancementSettings } from "@/components/EnhancementSettings";
import { OpenAICompatConfigModal } from "@/components/OpenAICompatConfigModal";
import { ProviderCard } from "@/components/ProviderCard";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import type { EnhancementOptions } from "@/types/ai";
import { fromBackendOptions, toBackendOptions } from "@/types/ai";
import { AI_PROVIDERS, CUSTOM_PROVIDER } from "@/types/providers";
import { hasApiKey, removeApiKey, saveApiKey, getApiKey, keyringSet } from "@/utils/keyring";
import { getErrorMessage } from "@/utils/error";
import { useReadinessState } from "@/contexts/ReadinessContext";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState, useCallback } from "react";
import { toast } from "sonner";
import { Info, Settings2 } from "lucide-react";

interface AISettings {
  enabled: boolean;
  provider: string;
  model: string;
  hasApiKey: boolean;
}

export function EnhancementsSection() {
  const readiness = useReadinessState();
  
  const [aiSettings, setAISettings] = useState<AISettings>({
    enabled: false,
    provider: "",
    model: "",
    hasApiKey: false,
  });

  const [showApiKeyModal, setShowApiKeyModal] = useState(false);
  const [showOpenAIConfig, setShowOpenAIConfig] = useState(false);
  const [openAIDefaultBaseUrl, setOpenAIDefaultBaseUrl] = useState("https://api.openai.com/v1");
  const [selectedProvider, setSelectedProvider] = useState<string>("");
  const [isLoading, setIsLoading] = useState(false);
  const [providerApiKeys, setProviderApiKeys] = useState<Record<string, boolean>>({});
  const [enhancementOptions, setEnhancementOptions] = useState<{
    preset: "Default" | "Prompts" | "Email" | "Commit";
    customVocabulary: string[];
  }>({
    preset: 'Default',
    customVocabulary: [],
  });
  const [settingsLoaded, setSettingsLoaded] = useState(false);

  const loadEnhancementOptions = async () => {
    try {
      const options = await invoke<EnhancementOptions>("get_enhancement_options");
      setEnhancementOptions(fromBackendOptions(options));
    } catch (error) {
      console.error("Failed to load enhancement options:", error);
    }
  };

  const loadAISettings = useCallback(async () => {
    try {
      // Check and cache API keys for all providers
      const allProviders = [...AI_PROVIDERS.map(p => p.id), "custom"];
      const keyStatus: Record<string, boolean> = {};

      await Promise.all(allProviders.map(async (providerId) => {
        const hasKey = await hasApiKey(providerId);
        keyStatus[providerId] = hasKey;
        if (hasKey) {
          console.log(`[AI Settings] Found ${providerId} API key in keyring, caching to backend`);
          try {
            const apiKey = await getApiKey(providerId);
            if (apiKey) {
              await invoke('cache_ai_api_key', { args: { provider: providerId, apiKey } });
            }
          } catch (error) {
            console.error(`Failed to cache ${providerId} API key:`, error);
          }
        }
      }));

      setProviderApiKeys(keyStatus);

      // Load AI settings from backend
      const settings = await invoke<AISettings>("get_ai_settings");
      setAISettings(settings);

      // If readiness state shows AI is ready, update the provider key status
      if (readiness?.ai_ready && settings.provider) {
        setProviderApiKeys(prev => ({ ...prev, [settings.provider]: true }));
      }
    } catch (error) {
      console.error("Failed to load AI settings:", error);
    }
  }, [readiness]);

  // Load settings when component mounts
  useEffect(() => {
    if (!settingsLoaded) {
      loadAISettings();
      loadEnhancementOptions();
      setSettingsLoaded(true);
    }
  }, [settingsLoaded, loadAISettings]);

  // Listen for AI events
  useEffect(() => {
    const unlistenReady = listen('ai-ready', async () => {
      if (settingsLoaded) {
        await loadAISettings();
      }
    });

    const unlistenApiKey = listen('api-key-saved', async (event) => {
      console.log('[AI Settings] API key saved:', event.payload);
      const settings = await invoke<AISettings>("get_ai_settings");
      setAISettings(settings);
      
      const provider = (event.payload as any).provider;
      if (provider) {
        setProviderApiKeys(prev => ({ ...prev, [provider]: true }));
      }
    });

    const unlistenApiKeyRemoved = listen<{ provider: string }>('api-key-removed', async (event) => {
      console.log('[AI Settings] API key removed:', event.payload.provider);
      setProviderApiKeys(prev => ({ ...prev, [event.payload.provider]: false }));
      
      // If removed provider is currently selected, clear selection
      if (aiSettings.provider === event.payload.provider) {
        setAISettings(prev => ({
          ...prev,
          enabled: false,
          provider: "",
          model: "",
          hasApiKey: false
        }));
        
        await invoke("update_ai_settings", {
          enabled: false,
          provider: "",
          model: ""
        });
      }
    });

    const unlistenFormattingError = listen<string>('formatting-error', async (event) => {
      const msg = (event.payload as any) || 'Formatting failed';
      toast.error(typeof msg === 'string' ? msg : 'Formatting failed');
    });

    return () => {
      Promise.all([unlistenReady, unlistenApiKey, unlistenApiKeyRemoved, unlistenFormattingError]).then(fns => {
        fns.forEach(fn => fn());
      });
    };
  }, [settingsLoaded, aiSettings.provider]);

  const handleEnhancementOptionsChange = async (newOptions: typeof enhancementOptions) => {
    setEnhancementOptions(newOptions);
    try {
      await invoke("update_enhancement_options", {
        options: toBackendOptions(newOptions)
      });
    } catch (error) {
      const message = getErrorMessage(error, "Failed to save enhancement options");
      toast.error(message);
    }
  };

  const handleToggleEnabled = async (enabled: boolean) => {
    if (enabled && (!providerApiKeys[aiSettings.provider] || !aiSettings.model)) {
      toast.error("Please select a provider, add an API key, and select a model first");
      return;
    }

    try {
      await invoke("update_ai_settings", {
        enabled,
        provider: aiSettings.provider,
        model: aiSettings.model
      });

      setAISettings(prev => ({ ...prev, enabled }));
      toast.success(enabled ? "AI formatting enabled" : "AI formatting disabled");
    } catch (error) {
      const message = getErrorMessage(error, "Failed to update AI settings");
      toast.error(message);
    }
  };

  const handleSetupApiKey = async (providerId: string) => {
    setSelectedProvider(providerId);
    
    if (providerId === "custom") {
      // Custom provider - show OpenAI config modal
      try {
        const savedConfig = await invoke<{ baseUrl: string }>('get_openai_config');
        setOpenAIDefaultBaseUrl(savedConfig.baseUrl || "https://api.openai.com/v1");
      } catch (error) {
        console.error('Failed to load custom config:', error);
      }
      setShowOpenAIConfig(true);
    } else {
      // Standard provider - show API key modal
      setShowApiKeyModal(true);
    }
  };

  const handleApiKeySubmit = async (apiKey: string) => {
    setIsLoading(true);
    try {
      const trimmedKey = apiKey.trim();
      await saveApiKey(selectedProvider, trimmedKey);

      // Update provider key status
      setProviderApiKeys(prev => ({ ...prev, [selectedProvider]: true }));

      // Find the provider's first model to auto-select
      const provider = AI_PROVIDERS.find(p => p.id === selectedProvider);
      const firstModel = provider?.models[0];

      if (firstModel) {
        // Auto-select first model and enable if not already set
        const shouldEnable = aiSettings.enabled || !aiSettings.model;
        
        await invoke("update_ai_settings", {
          enabled: shouldEnable,
          provider: selectedProvider,
          model: firstModel.id
        });

        setAISettings(prev => ({
          ...prev,
          enabled: shouldEnable,
          provider: selectedProvider,
          model: firstModel.id,
          hasApiKey: true
        }));
      }

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
        model: modelId
      });

      setAISettings(prev => ({
        ...prev,
        enabled: shouldEnable,
        provider: providerId,
        model: modelId,
        hasApiKey: hasKey
      }));

      toast.success("Model selected");
    } catch (error) {
      const message = getErrorMessage(error, "Failed to select model");
      toast.error(message);
    }
  };

  const getProviderDisplayName = (providerId: string) => {
    const provider = AI_PROVIDERS.find(p => p.id === providerId);
    return provider?.name || providerId;
  };

  // Check if any provider has a valid API key
  const hasAnyValidConfig = Object.values(providerApiKeys).some(v => v);
  const hasSelectedModel = Boolean(aiSettings.provider && aiSettings.model && providerApiKeys[aiSettings.provider]);

  // Get active model name for display
  const activeProvider = AI_PROVIDERS.find(p => p.id === aiSettings.provider);
  const activeModel = activeProvider?.models.find(m => m.id === aiSettings.model);

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="px-6 py-4 border-b border-border/40">
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

      <ScrollArea className="flex-1">
        <div className="p-6 space-y-6">
          {/* AI Providers Section */}
          <div className="space-y-4">
            <div className="flex items-center gap-2">
              <h2 className="text-base font-semibold">AI Providers</h2>
              <div className="h-px bg-border/50 flex-1" />
              {activeModel && (
                <span className="text-sm text-muted-foreground">
                  Active: <span className="text-amber-600 dark:text-amber-500">{activeModel.name}</span>
                </span>
              )}
            </div>
            
            <div className="grid gap-3">
              {AI_PROVIDERS.map((provider) => (
                <ProviderCard
                  key={provider.id}
                  provider={provider}
                  hasApiKey={providerApiKeys[provider.id] || false}
                  isActive={aiSettings.provider === provider.id && aiSettings.enabled}
                  selectedModel={aiSettings.provider === provider.id ? aiSettings.model : null}
                  onSetupApiKey={() => handleSetupApiKey(provider.id)}
                  onRemoveApiKey={() => handleRemoveApiKey(provider.id)}
                  onSelectModel={(modelId) => handleSelectModel(provider.id, modelId)}
                />
              ))}
            </div>
          </div>

          {/* Custom Provider Section */}
          <div className="space-y-4">
            <div className="flex items-center gap-2">
              <h2 className="text-base font-semibold">Custom Provider</h2>
              <div className="h-px bg-border/50 flex-1" />
            </div>
            
            <Card className="p-4">
              <div className="flex items-center justify-between gap-4">
                <div className="flex-1">
                  <h3 className={`font-semibold ${CUSTOM_PROVIDER.color}`}>{CUSTOM_PROVIDER.name}</h3>
                  <p className="text-sm text-muted-foreground mt-1">
                    Configure any OpenAI-compatible API endpoint (Ollama, LM Studio, etc.)
                  </p>
                </div>
                <Button
                  onClick={() => handleSetupApiKey("custom")}
                  variant="outline"
                  size="sm"
                >
                  <Settings2 className="w-3.5 h-3.5 mr-1.5" />
                  Configure
                </Button>
              </div>
            </Card>
          </div>

          {/* Formatting Options */}
          <div className="space-y-4">
            <div className="flex items-center gap-2">
              <h2 className="text-base font-semibold">Formatting Options</h2>
              <div className="h-px bg-border/50 flex-1" />
            </div>

            <div className={!aiSettings.enabled ? "opacity-50 pointer-events-none" : ""}>
              <EnhancementSettings
                settings={enhancementOptions}
                onSettingsChange={handleEnhancementOptionsChange}
              />
            </div>
          </div>

          {/* Setup Guide */}
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
                    AI formatting automatically improves your transcribed text with proper punctuation, grammar, and style in your selected language.
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
        providerName={getProviderDisplayName(selectedProvider)}
        isLoading={isLoading}
      />
      
      <OpenAICompatConfigModal
        isOpen={showOpenAIConfig}
        defaultBaseUrl={openAIDefaultBaseUrl}
        defaultModel={aiSettings.provider === 'openai' || aiSettings.provider === 'custom' ? aiSettings.model : ''}
        onClose={() => setShowOpenAIConfig(false)}
        onSubmit={async ({ baseUrl, model, apiKey }) => {
          try {
            setIsLoading(true);
            const trimmedBase = baseUrl.trim();
            const trimmedModel = model.trim();
            const trimmedKey = apiKey?.trim() || '';

            // Save OpenAI config
            await invoke('set_openai_config', { args: { baseUrl: trimmedBase } });

            // Save API key if provided
            if (trimmedKey) {
              await keyringSet('ai_api_key_openai', trimmedKey);
              await invoke('cache_ai_api_key', { args: { provider: 'openai', apiKey: trimmedKey } });
            }

            // Update settings
            const nextEnabled = aiSettings.enabled || !aiSettings.model;
            await invoke('update_ai_settings', { enabled: nextEnabled, provider: 'openai', model: trimmedModel });

            setAISettings(prev => ({
              ...prev,
              enabled: nextEnabled,
              provider: 'openai',
              model: trimmedModel,
              hasApiKey: true
            }));

            setProviderApiKeys(prev => ({ ...prev, openai: true }));

            toast.success('Configuration saved');
            setShowOpenAIConfig(false);
          } catch (error) {
            const message = getErrorMessage(error, 'Failed to save configuration');
            toast.error(message);
          } finally {
            setIsLoading(false);
          }
        }}
      />
    </div>
  );
}
