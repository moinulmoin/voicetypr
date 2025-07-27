import { ApiKeyModal } from "@/components/ApiKeyModal";
import { EnhancementModelCard } from "@/components/EnhancementModelCard";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Switch } from "@/components/ui/switch";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
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

  // Available models - flat list
  const models: AIModel[] = [
    {
      id: "llama-3.1-8b-instant",
      name: "Llama 3.1 8B Instant",
      provider: "groq",
      description: "Fast and efficient model for instant responses"
    }
  ];

  useEffect(() => {
    loadAISettings();
  }, []);

  const loadAISettings = async () => {
    try {
      const settings = await invoke<AISettings>("get_ai_settings");
      setAISettings(settings);

      // Check API keys for all unique providers
      const providers = [...new Set(models.map(m => m.provider))];
      const keyStatus: Record<string, boolean> = {};

      for (const provider of providers) {
        const providerSettings = await invoke<AISettings>("get_ai_settings_for_provider", { provider });
        keyStatus[provider] = providerSettings.hasApiKey;
      }

      setProviderApiKeys(keyStatus);
    } catch (error) {
      console.error("Failed to load AI settings:", error);
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
      await invoke("save_ai_api_key", {
        provider: selectedProvider,
        apiKey: apiKey.trim()
      });

      // Reload settings to get updated hasApiKey status
      await loadAISettings();
      setShowApiKeyModal(false);
      toast.success("API key saved successfully");
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
      await invoke("update_ai_settings", {
        enabled: aiSettings.enabled && providerApiKeys[provider],
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

  const getProviderDisplayName = (provider: string) => {
    const names: Record<string, string> = {
      groq: "Groq"
    };
    return names[provider] || provider;
  };

  const hasAnyApiKey = Object.values(providerApiKeys).some(v => v);
  const hasSelectedModel = Boolean(aiSettings.model);

  return (
    <div className="h-full flex flex-col p-6">
      <div className="flex-shrink-0 space-y-6">
        <h2 className="text-lg font-semibold">Enhancements</h2>

        {/* AI Enhancement Toggle - Minimal Style */}
        <div className="flex items-center justify-between gap-4">
          <div className="space-y-1">
            <Label htmlFor="ai-enhancement" className="text-sm font-medium">AI Enhancement</Label>
            <p className="text-xs text-muted-foreground">
              {!hasAnyApiKey
                ? "Add an API key below to enable"
                : !hasSelectedModel
                ? "Select a model below to enable"
                : "Improve transcriptions with AI"}
            </p>
          </div>
          <Switch
            id="ai-enhancement"
            checked={aiSettings.enabled}
            onCheckedChange={handleToggleEnabled}
            disabled={!hasAnyApiKey || !hasSelectedModel}
          />
        </div>

        {/* Models Section */}
        <div className="space-y-2">
          <Label className="text-sm font-medium">AI Models</Label>
          <p className="text-xs text-muted-foreground mb-3">
            Choose an AI model for post-processing
          </p>
        </div>
      </div>

      <ScrollArea className="flex-1 min-h-0">
        <div className="space-y-3">
          {models.map((model) => (
            <EnhancementModelCard
              key={model.id}
              model={model}
              hasApiKey={providerApiKeys[model.provider] || false}
              isSelected={aiSettings.model === model.id && providerApiKeys[model.provider]}
              onSetupApiKey={() => handleSetupApiKey(model.provider)}
              onSelect={() => handleModelSelect(model.id, model.provider)}
            />
          ))}
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