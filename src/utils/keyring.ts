import { invoke } from '@tauri-apps/api/core';

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
  
  // Also cache in backend for fast access during transcription
  await invoke('cache_ai_api_key', { provider, apiKey });
  
  console.log(`[Keyring] API key saved for ${provider}`);
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
};

// Load all API keys to backend cache (for app startup)
export const loadApiKeysToCache = async (): Promise<void> => {
  const providers = ['groq', 'gemini']; // Add more providers as needed
  
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