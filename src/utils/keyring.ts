import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';

/**
 * Save a value to the OS keyring
 * @param key The key to store the value under
 * @param value The value to store
 */
export const keyringSet = async (key: string, value: string): Promise<void> => {
  await invoke('keyring_set', { key, value });
};

/**
 * Get a value from the OS keyring
 * @param key The key to retrieve
 * @returns The value if found, null otherwise
 */
export const keyringGet = async (key: string): Promise<string | null> => {
  return await invoke<string | null>('keyring_get', { key });
};

/**
 * Delete a value from the OS keyring
 * @param key The key to delete
 */
export const keyringDelete = async (key: string): Promise<void> => {
  await invoke('keyring_delete', { key });
};

/**
 * Check if a key exists in the OS keyring
 * @param key The key to check
 * @returns true if the key exists, false otherwise
 */
export const keyringHas = async (key: string): Promise<boolean> => {
  return await invoke<boolean>('keyring_has', { key });
};

// API Key specific helpers
export const saveApiKey = async (provider: string, apiKey: string): Promise<void> => {
  const key = `ai_api_key_${provider}`;
  await keyringSet(key, apiKey);
  
  // Cache or validate depending on provider
  if (provider === 'openai') {
    // OpenAI-compatible requires validation (may include no-auth path via separate modal)
    await invoke('validate_and_cache_api_key', { provider, apiKey });
  } else {
    // For Groq/Gemini, just cache the key; validation happens during usage
    await invoke('cache_ai_api_key', { provider, apiKey });
  }
  
  console.log(`[Keyring] API key saved and validated for ${provider}`);
  
  // Emit event to notify that API key was saved
  await emit('api-key-saved', { provider });
};

export const getApiKey = async (provider: string): Promise<string | null> => {
  const key = `ai_api_key_${provider}`;
  return await keyringGet(key);
};

export const hasApiKey = async (provider: string): Promise<boolean> => {
  const key = `ai_api_key_${provider}`;
  return await keyringHas(key);
};

export const removeApiKey = async (provider: string): Promise<void> => {
  const key = `ai_api_key_${provider}`;
  await keyringDelete(key);
  
  // Clear backend cache
  await invoke('clear_ai_api_key_cache', { provider });
  
  console.log(`[Keyring] API key removed for ${provider}`);
  
  // Emit event to notify that API key was removed
  await emit('api-key-removed', { provider });
};

// Load all API keys to backend cache (for app startup)
export const loadApiKeysToCache = async (): Promise<void> => {
  const providers = ['groq', 'gemini', 'openai'];
  
  for (const provider of providers) {
    try {
      const apiKey = await getApiKey(provider);
      if (apiKey) {
        await invoke('cache_ai_api_key', { provider, apiKey });
        console.log(`[Keyring] Loaded ${provider} API key from keyring to cache`);
      }
    } catch (error) {
      console.error(`Failed to load API key for ${provider}:`, error);
    }
  }
};

// OpenAI-compatible configuration helpers
export const setOpenAIConfig = async (baseUrl: string, noAuth: boolean): Promise<void> => {
  await invoke('set_openai_config', { base_url: baseUrl, no_auth: noAuth });
};

export const saveOpenAIKeyWithConfig = async (
  apiKey: string,
  baseUrl: string,
  model: string,
  noAuth: boolean
): Promise<void> => {
  const provider = 'openai';
  const key = `ai_api_key_${provider}`;
  if (apiKey) {
    await keyringSet(key, apiKey);
  }

  await invoke('validate_and_cache_api_key', {
    provider,
    // Standardize to snake_case for Tauri commands
    api_key: apiKey || undefined,
    base_url: baseUrl,
    model,
    no_auth: noAuth || !apiKey?.trim(),
  });

  // Persist provider + model selection
  await invoke('update_ai_settings', { enabled: false, provider, model });

  console.log(`[Keyring] OpenAI-compatible config saved (noAuth=${noAuth})`);
  await emit('api-key-saved', { provider });
};

// STT (Speech-to-Text) cloud provider keys
// Soniox support
const STT_SONIOX_KEY = 'stt_api_key_soniox';

export const saveSttApiKeySoniox = async (apiKey: string): Promise<void> => {
  // Validate first; only persist to keyring on success
  await invoke('validate_and_cache_soniox_key', { api_key: apiKey, apiKey });
  await keyringSet(STT_SONIOX_KEY, apiKey);
  await emit('stt-key-saved', { provider: 'soniox' });
};

export const getSttApiKeySoniox = async (): Promise<string | null> => {
  return keyringGet(STT_SONIOX_KEY);
};

export const hasSttApiKeySoniox = async (): Promise<boolean> => {
  return keyringHas(STT_SONIOX_KEY);
};

export const removeSttApiKeySoniox = async (): Promise<void> => {
  await keyringDelete(STT_SONIOX_KEY);
  // Clear any backend cache if added in future; call is optional
  try {
    await invoke('clear_soniox_key_cache');
  } catch (_) {
    // best-effort; command may not exist in older builds
  }
  await emit('stt-key-removed', { provider: 'soniox' });
};