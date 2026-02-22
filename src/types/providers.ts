/**
 * AI Provider Configuration Types
 * Models are fetched dynamically from provider APIs
 */

export interface AIProviderModel {
  id: string;
  name: string;
  recommended: boolean;
}

export interface AIProviderConfig {
  id: string;
  name: string;
  color: string;
  apiKeyUrl: string;
  isCustom?: boolean;
}

// Provider configurations (models are fetched dynamically)
export const AI_PROVIDERS: AIProviderConfig[] = [
  {
    id: "openai",
    name: "OpenAI",
    color: "text-green-600",
    apiKeyUrl: "https://platform.openai.com/api-keys",
  },
  {
    id: "gemini",
    name: "Google Gemini",
    color: "text-blue-600",
    apiKeyUrl: "https://aistudio.google.com/apikey",
  },
  {
    id: "custom",
    name: "Custom (OpenAI-compatible)",
    color: "text-purple-600",
    apiKeyUrl: "",
    isCustom: true,
  },
];

export function getProviderById(id: string): AIProviderConfig | undefined {
  return AI_PROVIDERS.find(p => p.id === id);
}
