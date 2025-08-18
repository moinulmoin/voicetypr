import { ApiKeyModal } from "@/components/ApiKeyModal";
import { EnhancementModelCard } from "@/components/EnhancementModelCard";
import { EnhancementSettings } from "@/components/EnhancementSettings";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Switch } from "@/components/ui/switch";
import type { EnhancementOptions } from "@/types/ai";
import { fromBackendOptions, toBackendOptions } from "@/types/ai";
import { hasApiKey, removeApiKey, saveApiKey, getApiKey } from "@/utils/keyring";
import { useReadinessState } from "@/contexts/ReadinessContext";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState, useCallback } from "react";
import { toast } from "sonner";

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

  // Available models - flat list
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
        // Update settings with correct hasApiKey status from Stronghold
        // The enabled state is already persisted in the backend
        const currentModelProvider = settings.model ? models.find(m => m.id === settings.model)?.provider : null;
        const currentModelHasKey = currentModelProvider ? currentKeyStatus[currentModelProvider] : false;

        // Update the hasApiKey status based on current key status
        // Don't clear the model or disable AI enhancement just because the key isn't cached yet
        setAISettings({
          ...settings,
          hasApiKey: currentModelHasKey
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

    return () => {
      Promise.all([unlistenReady, unlistenApiKey, unlistenApiKeyRemoved]).then(fns => {
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
      toast.success(enabled ? "AI enhancement enabled" : "AI enhancement disabled");
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
    setShowApiKeyModal(true);
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
      gemini: "Gemini"
    };
    return names[provider] || provider;
  };

  const hasAnyApiKey = Object.values(providerApiKeys).some(v => v);
  const selectedModelProvider = models.find(m => m.id === aiSettings.model)?.provider;
  const hasSelectedModel = Boolean(aiSettings.model && selectedModelProvider && providerApiKeys[selectedModelProvider]);

  return (
    <div className="h-full flex flex-col p-6">
      <div className="flex-shrink-0 space-y-4 mb-4">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">AI Enhancement</h2>
          <Switch
            id="ai-enhancement"
            checked={aiSettings.enabled}
            onCheckedChange={handleToggleEnabled}
            disabled={!hasAnyApiKey || !hasSelectedModel}
          />
        </div>
      </div>

      <ScrollArea className="flex-1 min-h-0">
        <div className="space-y-4">
          {/* Models */}
          <div className="space-y-3">
            {models.map((model) => (
              <EnhancementModelCard
                key={model.id}
                model={model}
                hasApiKey={providerApiKeys[model.provider] || false}
                isSelected={aiSettings.model === model.id && providerApiKeys[model.provider]}
                onSetupApiKey={() => handleSetupApiKey(model.provider)}
                onSelect={() => handleModelSelect(model.id, model.provider)}
                onRemoveApiKey={() => handleRemoveApiKey(model.provider)}
              />
            ))}
          </div>

          {/* Simple setup guide when AI is disabled */}
          {!aiSettings.enabled && (
            <div className="bg-muted/50 rounded-lg p-4 space-y-3 mt-4">
              <h3 className="font-medium text-sm">Quick Setup Guide</h3>
              <ol className="text-sm text-muted-foreground space-y-2 list-decimal list-inside">
                <li>Click "Add Key" on a model above</li>
                <li>Visit the provider's website to get your API key</li>
                <li>Paste the API key and submit</li>
                <li>Select the model you want to use</li>
                <li>Toggle the switch above to enable AI enhancement</li>
              </ol>
            </div>
          )}

          {/* Enhancement Settings - Always visible when enabled */}
          {aiSettings.enabled && (
            <div className="mt-4 pt-4">
              <EnhancementSettings
                settings={enhancementOptions}
                onSettingsChange={handleEnhancementOptionsChange}
              />
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
    </div>
  );
}