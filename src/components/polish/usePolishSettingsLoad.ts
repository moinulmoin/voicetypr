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
    const activeRef = { current: true };
    void (async () => {
      try {
        const loadedAISettings = await loadAISettings();
        // A superseded attempt applies nothing and leaves the start guard
        // alone: the effect that replaced it owns the guard now.
        if (!activeRef.current) return;
        await loadEnhancementOptionsRef.current(loadedAISettings?.enabled ?? false);
        if (!activeRef.current) return;
        const writingSettingsLoaded = await loadWritingSettingsRef.current();
        if (!activeRef.current) return;
        setSettingsLoaded(writingSettingsLoaded);
        if (!writingSettingsLoaded) {
          settingsLoadStartedRef.current = false;
        }
      } catch (error) {
        if (!activeRef.current) return;
        settingsLoadStartedRef.current = false;
        log.error("Failed to load Polish settings:", error);
      }
    })();

    return () => {
      // Lower the guard so the replacement effect (StrictMode's
      // setup-cleanup-setup cycle) can start its own attempt instead of being
      // blocked by the canceled one.
      activeRef.current = false;
      settingsLoadStartedRef.current = false;
    };
  }, [
    settingsLoaded,
    loadAISettings,
    loadEnhancementOptionsRef,
    loadWritingSettingsRef,
    setSettingsLoaded,
  ]);
}
