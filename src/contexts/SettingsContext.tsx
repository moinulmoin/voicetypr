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
      console.error('Failed to load settings:', error);
    } finally {
      setIsLoading(false);
    }
  }, []);

  const updateSettings = useCallback(async (updates: Partial<AppSettings>) => {
    if (!settings) return;
    
    try {
      const updatedSettings = { ...settings, ...updates };
      await invoke('save_settings', { settings: updatedSettings });
      setSettings(updatedSettings);
    } catch (err) {
      console.error('Failed to update settings:', err);
      throw err;
    }
  }, [settings]);

  // Load settings on mount
  useEffect(() => {
    loadSettings();
  }, [loadSettings]);

  // Listen for settings changes from other sources (e.g., tray menu)
  useEffect(() => {
    const unlistenModel = listen('model-changed', () => {
      console.log('Model changed, refreshing settings');
      loadSettings();
    });

    const unlistenLanguage = listen('language-changed', () => {
      console.log('Language changed, refreshing settings');
      loadSettings();
    });

    const unlistenSettings = listen('settings-changed', () => {
      console.log('Settings changed, refreshing');
      loadSettings();
    });

    return () => {
      Promise.all([unlistenModel, unlistenLanguage, unlistenSettings]).then(unsubs => {
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