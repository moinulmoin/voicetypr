import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AIProviderModel } from "@/types/providers";

interface UseProviderModelsReturn {
  models: AIProviderModel[];
  loading: boolean;
  error: string | null;
  fetchModels: () => Promise<void>;
  clearModels: () => void;
}

/**
 * Hook for fetching and managing models for a specific provider.
 * Models are fetched on demand (when fetchModels is called).
 * Results are cached in component state.
 */
export function useProviderModels(providerId: string): UseProviderModelsReturn {
  const [models, setModels] = useState<AIProviderModel[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchModels = useCallback(async () => {
    // Don't fetch for custom provider (user defines model in config)
    if (providerId === "custom") {
      return;
    }

    // Don't refetch if already loading
    if (loading) {
      return;
    }

    setLoading(true);
    setError(null);

    try {
      const fetchedModels = await invoke<AIProviderModel[]>("list_provider_models", {
        provider: providerId,
      });
      setModels(fetchedModels);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      setError(errorMessage);
      console.error(`Failed to fetch models for ${providerId}:`, err);
    } finally {
      setLoading(false);
    }
  }, [providerId, loading]);

  const clearModels = useCallback(() => {
    setModels([]);
    setError(null);
  }, []);

  return {
    models,
    loading,
    error,
    fetchModels,
    clearModels,
  };
}

/**
 * Hook for managing models across all providers.
 * Provides a centralized way to fetch and cache models.
 */
export function useAllProviderModels() {
  const [modelsMap, setModelsMap] = useState<Record<string, AIProviderModel[]>>({});
  const [loadingMap, setLoadingMap] = useState<Record<string, boolean>>({});
  const [errorMap, setErrorMap] = useState<Record<string, string | null>>({});

  const fetchModels = useCallback(async (providerId: string) => {
    // Don't fetch for custom provider
    if (providerId === "custom") {
      return;
    }

    // Don't refetch if already loading
    if (loadingMap[providerId]) {
      return;
    }

    setLoadingMap(prev => ({ ...prev, [providerId]: true }));
    setErrorMap(prev => ({ ...prev, [providerId]: null }));

    try {
      const fetchedModels = await invoke<AIProviderModel[]>("list_provider_models", {
        provider: providerId,
      });
      setModelsMap(prev => ({ ...prev, [providerId]: fetchedModels }));
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      setErrorMap(prev => ({ ...prev, [providerId]: errorMessage }));
      console.error(`Failed to fetch models for ${providerId}:`, err);
    } finally {
      setLoadingMap(prev => ({ ...prev, [providerId]: false }));
    }
  }, [loadingMap]);

  const getModels = useCallback((providerId: string): AIProviderModel[] => {
    return modelsMap[providerId] || [];
  }, [modelsMap]);

  const isLoading = useCallback((providerId: string): boolean => {
    return loadingMap[providerId] || false;
  }, [loadingMap]);

  const getError = useCallback((providerId: string): string | null => {
    return errorMap[providerId] || null;
  }, [errorMap]);

  const clearModels = useCallback((providerId: string) => {
    setModelsMap(prev => {
      const next = { ...prev };
      delete next[providerId];
      return next;
    });
    setErrorMap(prev => {
      const next = { ...prev };
      delete next[providerId];
      return next;
    });
  }, []);

  return {
    fetchModels,
    getModels,
    isLoading,
    getError,
    clearModels,
  };
}
