import { useEffect, useRef, type MutableRefObject } from "react";
import { createLogger } from "@/lib/logger";
import type { AISettings } from "@/types/ai";

const log = createLogger("enhancements");

export function usePolishSettingsLoad({
  settingsLoaded,
  setSettingsLoaded,
  loadAISettings,
  loadEnhancementOptionsRef,
  loadWritingSettingsRef,
}: {
  settingsLoaded: boolean;
  setSettingsLoaded: (loaded: boolean) => void;
  loadAISettings: () => Promise<AISettings | null | undefined>;
  loadEnhancementOptionsRef: MutableRefObject<(aiEnabled: boolean) => Promise<void>>;
  loadWritingSettingsRef: MutableRefObject<() => Promise<boolean>>;
}) {
  const settingsLoadStartedRef = useRef(false);

  useEffect(() => {
    if (settingsLoaded || settingsLoadStartedRef.current) {
      return;
    }

    settingsLoadStartedRef.current = true;
    const mountedRef = { current: true };
    void (async () => {
      try {
        const loadedAISettings = await loadAISettings();
        if (!mountedRef.current) {
          settingsLoadStartedRef.current = false;
          return;
        }
        await loadEnhancementOptionsRef.current(loadedAISettings?.enabled ?? false);
        if (!mountedRef.current) {
          settingsLoadStartedRef.current = false;
          return;
        }
        const writingSettingsLoaded = await loadWritingSettingsRef.current();
        if (!mountedRef.current) {
          settingsLoadStartedRef.current = false;
          return;
        }
        setSettingsLoaded(writingSettingsLoaded);
        if (!writingSettingsLoaded) {
          settingsLoadStartedRef.current = false;
        }
      } catch (error) {
        settingsLoadStartedRef.current = false;
        if (!mountedRef.current) return;
        log.error("Failed to load Polish settings:", error);
      }
    })();

    return () => {
      mountedRef.current = false;
    };
  }, [
    settingsLoaded,
    loadAISettings,
    loadEnhancementOptionsRef,
    loadWritingSettingsRef,
    setSettingsLoaded,
  ]);
}
