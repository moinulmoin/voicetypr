import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useSettings } from '@/contexts/SettingsContext';
import type { ModelInfo } from '@/types';

interface ModelStatusResponse {
  models: ModelInfo[];
}

export function useModelAvailability() {
  const { settings } = useSettings();
  const [hasModels, setHasModels] = useState<boolean | null>(null);
  const [selectedModelAvailable, setSelectedModelAvailable] = useState<boolean | null>(null);
  const [isChecking, setIsChecking] = useState(false);

  const checkModels = useCallback(async () => {
    setIsChecking(true);
    try {
      const status = await invoke<ModelStatusResponse>('get_model_status');
      const readyModels = status.models.filter(
        (model) => model.downloaded && !model.requires_setup
      );
      setHasModels(readyModels.length > 0);

      const selectedModel = settings?.current_model;
      if (selectedModel) {
        const match = status.models.find((model) => model.name === selectedModel);
        const isAvailable =
          !!match && match.downloaded && !match.requires_setup;
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

    const unlistenCloudSaved = listen('stt-key-saved', () => {
      console.log('[useModelAvailability] Cloud provider connected');
      checkModels();
    });

    const unlistenCloudRemoved = listen('stt-key-removed', () => {
      console.log('[useModelAvailability] Cloud provider disconnected');
      checkModels();
    });

    return () => {
      Promise.all([
        unlistenDownloaded,
        unlistenDeleted,
        unlistenModelChanged,
        unlistenCloudSaved,
        unlistenCloudRemoved
      ]).then(unsubs => {
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
