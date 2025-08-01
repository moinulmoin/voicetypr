import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useSettings } from '@/contexts/SettingsContext';

interface ModelStatusResponse {
  models: Array<{
    name: string;
    info: {
      downloaded: boolean;
      // other fields we don't need
    };
  }>;
}

export function useModelAvailability() {
  const { settings } = useSettings();
  const [hasModels, setHasModels] = useState<boolean | null>(null);
  const [selectedModelAvailable, setSelectedModelAvailable] = useState<boolean | null>(null);
  const [isChecking, setIsChecking] = useState(false);

  const checkModels = useCallback(async () => {
    setIsChecking(true);
    try {
      // Check which models are downloaded
      const status = await invoke<ModelStatusResponse>('get_model_status');
      const downloadedModels = status.models.filter(m => m.info.downloaded);
      setHasModels(downloadedModels.length > 0);

      // Check if selected model is available using settings from context
      const selectedModel = settings?.current_model;
      
      if (selectedModel) {
        const isAvailable = downloadedModels.some(m => m.name === selectedModel);
        setSelectedModelAvailable(isAvailable);
      } else {
        setSelectedModelAvailable(false);
      }
    } catch (error) {
      console.error('Failed to check model availability:', error);
      setHasModels(false);
      setSelectedModelAvailable(false);
    } finally {
      setIsChecking(false);
    }
  }, [settings]);

  // Check on mount and when settings change
  useEffect(() => {
    checkModels();
  }, [checkModels, settings]);

  // Listen for model events
  useEffect(() => {
    const unlistenDownloaded = listen('model-downloaded', () => {
      console.log('[useModelAvailability] Model downloaded event received');
      checkModels();
    });

    const unlistenDeleted = listen('model-deleted', () => {
      console.log('[useModelAvailability] Model deleted event received');
      checkModels();
    });

    const unlistenModelChanged = listen('model-changed', () => {
      console.log('[useModelAvailability] Model changed event received');
      checkModels();
    });

    return () => {
      Promise.all([unlistenDownloaded, unlistenDeleted, unlistenModelChanged]).then(unsubs => {
        unsubs.forEach(unsub => unsub());
      });
    };
  }, [checkModels]);

  return {
    hasModels,
    selectedModelAvailable,
    isChecking,
    checkModels
  };
}