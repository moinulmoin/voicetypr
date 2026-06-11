/**
 * AI provider DTOs.
 * Models are fetched dynamically from provider APIs.
 */

export interface AIProviderModel {
  id: string;
  name: string;
  recommended: boolean;
}

// Mirrors Rust `crate::ai::contract::AiProvider`.
// The existing Tauri command wire DTO keeps `{ id, name }` compatibility and maps
// Rust `label` to `name`; contract fields may arrive as snake_case or camelCase.
export interface AiProvider {
  id: string;
  label?: string;
  name?: string;
  requires_api_key?: boolean;
  requiresApiKey?: boolean;
  supports_base_url?: boolean;
  supportsBaseUrl?: boolean;
  supports_reasoning?: boolean;
  supportsReasoning?: boolean;
}

export interface AIProviderConfig extends AiProvider {
  name: string;
  color: string;
  apiKeyUrl: string;
  isCustom: boolean;
}

const PROVIDER_UI_METADATA: Record<string, { color: string; apiKeyUrl: string }> = {
  openai: {
    color: "text-green-600",
    apiKeyUrl: "https://platform.openai.com/api-keys",
  },
  gemini: {
    color: "text-blue-600",
    apiKeyUrl: "https://aistudio.google.com/apikey",
  },
  anthropic: {
    color: "text-orange-600",
    apiKeyUrl: "https://console.anthropic.com/settings/keys",
  },
  custom: {
    color: "text-purple-600",
    apiKeyUrl: "",
  },
};

export function toProviderConfig(provider: AiProvider): AIProviderConfig {
  const metadata = PROVIDER_UI_METADATA[provider.id] ?? {
    color: "text-foreground",
    apiKeyUrl: "",
  };
  const supportsBaseUrl = provider.supports_base_url ?? provider.supportsBaseUrl ?? false;

  return {
    ...provider,
    name: provider.name ?? provider.label ?? provider.id,
    color: metadata.color,
    apiKeyUrl: metadata.apiKeyUrl,
    isCustom: provider.id === "custom" || supportsBaseUrl,
  };
}
