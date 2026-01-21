import { createContext, useContext, ReactNode, useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { AppSettings } from '@/types';

interface SettingsContextType {
  settings: AppSettings | null;
  isLoading: boolean;
  error: Error | null;
  refreshSettings: () => Promise<void>;
  updateSettings: (updates: Partial<AppSettings>) => Promise<void>;
}

const SettingsContext = createContext<SettingsContextType | null>(null);

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const loadSettings = useCallback(async () => {
    try {
      setIsLoading(true);
      setError(null);
      const appSettings = await invoke<AppSettings>('get_settings');
      setSettings(appSettings);
    } catch (err) {
      const error = err instanceof Error ? err : new Error('Failed to load settings');
      setError(error);
      console.error('[SettingsContext] Failed to load settings:', error);
    } finally {
      setIsLoading(false);
    }
  }, []);

  const updateSettings = useCallback(async (updates: Partial<AppSettings>) => {
    if (!settings) {
      return;
    }

    const updatedSettings = { ...settings, ...updates };
    const previousSettings = settings;

    // Optimistic update - update state immediately so UI responds instantly
    setSettings(updatedSettings);

    try {
      await invoke('save_settings', { settings: updatedSettings });
    } catch (err) {
      // Rollback on error
      setSettings(previousSettings);
      console.error('[SettingsContext] Failed to update settings:', err);
      throw err;
    }
  }, [settings]);

  // Load settings on mount
  useEffect(() => {
    loadSettings();
  }, [loadSettings]);

  // Listen for settings changes from other sources (e.g., tray menu)
  // Note: We don't listen to 'settings-changed' here because it causes race conditions
  // with the optimistic updates in updateSettings. The frontend already updates state
  // after save_settings completes, so we don't need to reload.
  useEffect(() => {
    const unlistenModel = listen('model-changed', () => {
      loadSettings();
    });

    const unlistenLanguage = listen('language-changed', () => {
      loadSettings();
    });

    const unlistenAudioDevice = listen('audio-device-changed', () => {
      loadSettings();
    });

    return () => {
      Promise.all([unlistenModel, unlistenLanguage, unlistenAudioDevice]).then(unsubs => {
        unsubs.forEach(unsub => unsub());
      });
    };
  }, [loadSettings]);

  return (
    <SettingsContext.Provider
      value={{
        settings,
        isLoading,
        error,
        refreshSettings: loadSettings,
        updateSettings,
      }}
    >
      {children}
    </SettingsContext.Provider>
  );
}

export function useSettings() {
  const context = useContext(SettingsContext);
  if (!context) {
    throw new Error('useSettings must be used within a SettingsProvider');
  }
  return context;
}

// Helper hook for components that only need specific settings
export function useSetting<K extends keyof AppSettings>(
  key: K
): AppSettings[K] | undefined {
  const { settings } = useSettings();
  return settings?.[key];
}
