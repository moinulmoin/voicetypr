import { ApiKeyModal } from "@/components/ApiKeyModal";
import { EnhancementModelCard } from "@/components/EnhancementModelCard";
import { EnhancementSettings } from "@/components/EnhancementSettings";
import { OpenAICompatConfigModal } from "@/components/OpenAICompatConfigModal";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import type { EnhancementOptions } from "@/types/ai";
import { fromBackendOptions, toBackendOptions } from "@/types/ai";
import { hasApiKey, removeApiKey, saveApiKey, getApiKey, saveOpenAIKeyWithConfig } from "@/utils/keyring";
import { useReadinessState } from "@/contexts/ReadinessContext";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState, useCallback } from "react";
import { toast } from "sonner";
import { Info } from "lucide-react";

interface AIModel {
  id: string;
  name: string;
  provider: string;
  description?: string;
}

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
    provider: "groq",
    model: "",  // Empty by default until user selects
    hasApiKey: false,
  });

  const [showApiKeyModal, setShowApiKeyModal] = useState(false);
  const [showOpenAIConfig, setShowOpenAIConfig] = useState(false);
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

  // Groq, Gemini, and OpenAI-compatible
  const models: AIModel[] = [
    {
      id: "llama-3.1-8b-instant",
      name: "Llama 3.1 8B Instant",
      provider: "groq",
      description: "Fast and efficient model for instant responses"
    },
    {
      id: "gemini-2.5-flash-lite",
      name: "Gemini 2.5 Flash Lite",
      provider: "gemini",
      description: "Google's lightweight flash model for quick processing"
    },
    {
      id: "openai-compatible",
      name: "OpenAI Compatible",
      provider: "openai",
      description: "Configure any OpenAI-compatible endpoint"
    }
  ];

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
      // First, check and cache API keys if not already loaded
      let currentKeyStatus = providerApiKeys;
      if (Object.keys(providerApiKeys).length === 0) {
        const providers = [...new Set(models.map(m => m.provider))];
        const keyStatus: Record<string, boolean> = {};

        // Batch check API keys and cache them to backend
        await Promise.all(providers.map(async (provider) => {
          const hasKey = await hasApiKey(provider);
          keyStatus[provider] = hasKey;
          if (hasKey) {
            console.log(`[AI Settings] Found ${provider} API key in keyring, caching to backend`);
            // Load the key from keyring and cache it to backend
            try {
              const apiKey = await getApiKey(provider);
              if (apiKey) {
                await invoke('cache_ai_api_key', { provider, apiKey });
              }
            } catch (error) {
              console.error(`Failed to cache ${provider} API key:`, error);
            }
          }
        }));

        setProviderApiKeys(keyStatus);
        currentKeyStatus = keyStatus;
      }

      // Now load settings after API keys are cached
      const settings = await invoke<AISettings>("get_ai_settings");
      setAISettings(settings);

      // Use readiness state for quick check
      if (readiness?.ai_ready) {
        // AI is ready, so current provider has API key
        const currentProvider = settings.provider;
        setProviderApiKeys(prev => ({ ...prev, [currentProvider]: true }));
      }

      // Auto-select model if only one has API key and no model is currently selected
      const modelsWithApiKey = models.filter(m => currentKeyStatus[m.provider]);
      if (!settings.model && modelsWithApiKey.length === 1) {
        const autoSelectModel = modelsWithApiKey[0];
        console.log(`[AI Settings] Auto-selecting model: ${autoSelectModel.name}`);

        // Update backend settings with auto-selected model
        // Preserve the enabled state from settings
        await invoke("update_ai_settings", {
          enabled: settings.enabled, // Preserve the existing enabled state
          provider: autoSelectModel.provider,
          model: autoSelectModel.id
        });

        // Update local state
        setAISettings({
          ...settings,
          enabled: settings.enabled, // Preserve the existing enabled state
          provider: autoSelectModel.provider,
          model: autoSelectModel.id,
          hasApiKey: true
        });
      } else {
        // Respect backend hasApiKey for OpenAI (no-auth). For others, derive from keyring status.
        const currentModelProvider = settings.provider === 'openai'
          ? 'openai'
          : (settings.model ? models.find(m => m.id === settings.model)?.provider || null : null);
        const currentModelHasKey = currentModelProvider ? currentKeyStatus[currentModelProvider] : false;
        const derivedHasKey = settings.provider === 'openai' ? settings.hasApiKey : currentModelHasKey;

        setAISettings({
          ...settings,
          hasApiKey: derivedHasKey,
        });
      }
    } catch (error) {
      console.error("Failed to load AI settings:", error);
    }
  }, [readiness, providerApiKeys, models]);

  // Load settings only once when component becomes visible
  useEffect(() => {
    if (!settingsLoaded) {
      loadAISettings();
      loadEnhancementOptions();
      setSettingsLoaded(true);
    }
  }, [settingsLoaded, loadAISettings]);

  // Listen for AI readiness changes from backend
  useEffect(() => {
    const unlistenReady = listen('ai-ready', async () => {
      // Only reload if settings were already loaded
      if (settingsLoaded) {
        await loadAISettings();
      }
    });

    // Listen for API key save events
    const unlistenApiKey = listen('api-key-saved', async (event) => {
      console.log('[AI Settings] API key saved event received:', event.payload);
      // Only reload settings, not API keys check
      const settings = await invoke<AISettings>("get_ai_settings");
      setAISettings(settings);
      
      // Update provider key status for the specific provider
      const provider = (event.payload as any).provider;
      if (provider) {
        setProviderApiKeys(prev => ({ ...prev, [provider]: true }));
      }
    });

    // Listen for API key remove events
    const unlistenApiKeyRemoved = listen<{ provider: string }>('api-key-removed', async (event) => {
      console.log('[AI Settings] API key removed for provider:', event.payload.provider);
      
      // Update local state immediately
      setProviderApiKeys(prev => ({ ...prev, [event.payload.provider]: false }));
      
      // Check if the removed key was for the currently selected model
      const selectedModel = models.find(m => m.id === aiSettings.model);
      if (selectedModel && selectedModel.provider === event.payload.provider) {
        console.log('[AI Settings] Clearing model selection for removed API key');
        // Clear the model selection and disable AI
        setAISettings(prev => ({
          ...prev,
          enabled: false,
          provider: "",
          model: "",
          hasApiKey: false
        }));
        
        // Update backend to clear the selection
        try {
          await invoke("update_ai_settings", {
            enabled: false,
            provider: "",
            model: ""
          });
        } catch (error) {
          console.error('Failed to update backend settings:', error);
        }
      }
    });

    // Listen for formatting failures from backend and show a toast
    const unlistenFormattingError = listen<string>('formatting-error', async (event) => {
      const msg = (event.payload as any) || 'Formatting failed';
      toast.error(typeof msg === 'string' ? msg : 'Formatting failed');
    });

    return () => {
      Promise.all([unlistenReady, unlistenApiKey, unlistenApiKeyRemoved, unlistenFormattingError]).then(fns => {
        fns.forEach(fn => fn());
      });
    };
  }, [settingsLoaded]);

  const handleEnhancementOptionsChange = async (newOptions: typeof enhancementOptions) => {
    setEnhancementOptions(newOptions);
    try {
      await invoke("update_enhancement_options", {
        options: toBackendOptions(newOptions)
      });
    } catch (error) {
      toast.error(`Failed to save enhancement options: ${error}`);
    }
  };

  const handleToggleEnabled = async (enabled: boolean) => {
    if (enabled && (!aiSettings.hasApiKey || !aiSettings.model)) {
      toast.error("Please select a model and add an API key first");
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
      toast.error(`Failed to update settings: ${error}`);
    }
  };

  const handleApiKeySubmit = async (apiKey: string) => {
    setIsLoading(true);
    try {
      // Save API key using Stronghold
      await saveApiKey(selectedProvider, apiKey.trim());

      // Close modal first to give feedback
      setShowApiKeyModal(false);
      toast.success("API key saved securely");

      // No need for delay - the event listener will handle the reload
    } catch (error) {
      toast.error(`Failed to save API key: ${error}`);
    } finally {
      setIsLoading(false);
    }
  };

  const handleSetupApiKey = (provider: string) => {
    setSelectedProvider(provider);
    if (provider === 'openai') {
      setShowOpenAIConfig(true);
    } else {
      setShowApiKeyModal(true);
    }
  };

  const handleModelSelect = async (modelId: string, provider: string) => {
    try {
      // When selecting a model, preserve the enabled state unless the provider has no API key
      const shouldBeEnabled = providerApiKeys[provider] ? aiSettings.enabled : false;
      
      await invoke("update_ai_settings", {
        enabled: shouldBeEnabled,
        provider: provider,
        model: modelId
      });

      setAISettings(prev => ({
        ...prev,
        provider: provider,
        model: modelId,
        hasApiKey: providerApiKeys[provider] || false
      }));

      toast.success("Model selected");
    } catch (error) {
      toast.error(`Failed to select model: ${error}`);
    }
  };

  const handleRemoveApiKey = async (provider: string) => {
    try {
      console.log(`[AI Settings] Removing API key for provider: ${provider}`);
      
      // Remove the key (this also clears backend cache and emits the event)
      // The 'api-key-removed' event listener will handle all UI updates
      await removeApiKey(provider);
      
      toast.success("API key removed");
    } catch (error) {
      console.error(`[AI Settings] Failed to remove API key:`, error);
      toast.error(`Failed to remove API key: ${error}`);
    }
  };

  const getProviderDisplayName = (provider: string) => {
    const names: Record<string, string> = {
      groq: "Groq",
      gemini: "Gemini",
      openai: "OpenAI Compatible"
    };
    return names[provider] || provider;
  };

  // Valid config is either a cached key in keyring OR backend-validated config (OpenAI no-auth)
  const hasAnyValidConfig = aiSettings.hasApiKey || Object.values(providerApiKeys).some(v => v);
  const selectedModelProvider = models.find(m => m.id === aiSettings.model)?.provider;
  const hasSelectedModel = aiSettings.provider === 'openai'
    ? Boolean(aiSettings.model)
    : Boolean(aiSettings.model && selectedModelProvider && providerApiKeys[selectedModelProvider]);
  const selectedModelName = models.find(m => m.id === aiSettings.model)?.name;

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
          {/* AI Models Section */}
          <div className="space-y-4">
            <div className="flex items-center gap-2">
              <h2 className="text-base font-semibold">AI Models</h2>
              <div className="h-px bg-border/50 flex-1" />
              {selectedModelName && (
                <span className="text-sm text-muted-foreground">
                  Active: <span className="text-amber-600 dark:text-amber-500">{selectedModelName}</span>
                </span>
              )}
            </div>
            
            <div className="grid gap-3">
              {models.map((model) => {
                const hasKey = model.provider === 'openai'
                  ? (aiSettings.hasApiKey || providerApiKeys[model.provider] || false)
                  : (providerApiKeys[model.provider] || false);
                const isSelected = model.provider === 'openai'
                  ? (aiSettings.provider === 'openai' && hasKey)
                  : (aiSettings.model === model.id && providerApiKeys[model.provider]);
                return (
                  <EnhancementModelCard
                    key={model.id}
                    model={model}
                    hasApiKey={hasKey}
                    isSelected={isSelected}
                    onSetupApiKey={() => handleSetupApiKey(model.provider)}
                    onSelect={() => (model.provider === 'openai' ? handleModelSelect(aiSettings.model, model.provider) : handleModelSelect(model.id, model.provider))}
                    onRemoveApiKey={() => handleRemoveApiKey(model.provider)}
                  />
                );
              })}
            </div>
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
                    <li>Click "Add Key" on a model above</li>
                    <li>Get your API key from the provider's website</li>
                    <li>Paste the API key and save</li>
                    <li>Select the model you want to use</li>
                    <li>Toggle AI Formatting on to enable</li>
                  </ol>
                  <p className="text-xs text-muted-foreground mt-3">
                    AI formatting automatically improves your transcribed text with proper punctuation, capitalization, and style.
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
        defaultBaseUrl="https://api.openai.com"
        defaultModel={aiSettings.model}
        onClose={() => setShowOpenAIConfig(false)}
        onSubmit={async ({ baseUrl, model, apiKey, noAuth }) => {
          try {
            setIsLoading(true);
            await saveOpenAIKeyWithConfig(apiKey?.trim() || '', baseUrl.trim(), model.trim(), !!noAuth);
            setAISettings(prev => ({ ...prev, provider: 'openai', model: model.trim(), hasApiKey: true }));
            toast.success('Configuration saved');
            setShowOpenAIConfig(false);
          } catch (error) {
            toast.error(`Failed to save configuration: ${error}`);
          } finally {
            setIsLoading(false);
          }
        }}
      />
    </div>
  );
}