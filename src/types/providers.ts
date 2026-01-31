/**
 * AI Provider Configuration Types
 * Defines the structure for major AI providers and their models
 */

export interface AIProviderModel {
  id: string;
  name: string;
  description?: string;
}

export interface AIProviderConfig {
  id: string;
  name: string;
  color: string;
  models: AIProviderModel[];
  apiKeyUrl: string;
  // If true, this is a custom/OpenAI-compatible provider
  isCustom?: boolean;
}

// Major providers configuration (January 2026)
export const AI_PROVIDERS: AIProviderConfig[] = [
  {
    id: "openai",
    name: "OpenAI",
    color: "text-green-600",
    apiKeyUrl: "https://platform.openai.com/api-keys",
    models: [
      { id: "gpt-5-nano", name: "GPT-5 Nano", description: "Fastest, most cost-efficient" },
      { id: "gpt-5-mini", name: "GPT-5 Mini", description: "Fast, cost-effective" },
    ],
  },
  {
    id: "anthropic",
    name: "Anthropic",
    color: "text-amber-600",
    apiKeyUrl: "https://console.anthropic.com/settings/keys",
    models: [
      { id: "claude-haiku-4-5", name: "Claude Haiku 4.5", description: "Fast, cost-efficient" },
      { id: "claude-sonnet-4-5", name: "Claude Sonnet 4.5", description: "Balanced performance" },
    ],
  },
  {
    id: "gemini",
    name: "Google Gemini",
    color: "text-blue-600",
    apiKeyUrl: "https://aistudio.google.com/apikey",
    models: [
      { id: "gemini-2.0-flash", name: "Gemini 2.0 Flash", description: "Fast, free tier available" },
      { id: "gemini-2.5-flash-lite", name: "Gemini 2.5 Flash Lite", description: "Ultra cost-efficient" },
      { id: "gemini-3-flash", name: "Gemini 3 Flash", description: "Latest & fastest" },
    ],
  },
  {
    id: "custom",
    name: "Custom (OpenAI-compatible)",
    color: "text-purple-600",
    apiKeyUrl: "",
    isCustom: true,
    models: [], // Models are user-defined via configuration
  },
];

export function getProviderById(id: string): AIProviderConfig | undefined {
  return AI_PROVIDERS.find(p => p.id === id);
}

export function getModelById(providerId: string, modelId: string): AIProviderModel | undefined {
  const provider = getProviderById(providerId);
  return provider?.models.find(m => m.id === modelId);
}
