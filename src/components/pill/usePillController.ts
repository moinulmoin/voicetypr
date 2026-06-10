import { useSetting } from "@/contexts/SettingsContext";
import { useRecording } from "@/hooks/useRecording";
import type { PillIndicatorMode } from "@/types";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useState } from "react";

export type PillState = "idle" | "listening" | "transcribing" | "formatting";

const FORMATTING_EVENTS = [
  ["enhancing-started", true],
  ["enhancing-completed", false],
  ["enhancing-failed", false],
] as const;

export interface PillControllerState {
  audioLevel: number;
  isActive: boolean;
  isVisible: boolean;
  pillState: PillState;
}

export function usePillController(): PillControllerState {
  const recording = useRecording();
  const pillIndicatorMode: PillIndicatorMode =
    useSetting("pill_indicator_mode") ?? "when_recording";
  const [audioLevel, setAudioLevel] = useState(0);
  const [isFormatting, setIsFormatting] = useState(false);

  const pillState = useMemo<PillState>(() => {
    if (isFormatting) return "formatting";
    if (recording.state === "recording") return "listening";
    if (recording.state === "transcribing" || recording.state === "stopping") {
      return "transcribing";
    }
    return "idle";
  }, [isFormatting, recording.state]);

  const isListening = pillState === "listening";

  useEffect(() => {
    if (!isListening) {
      return;
    }

    let isMounted = true;
    let unlisten: (() => void) | undefined;

    void listen<number>("audio-level", (event) => {
      if (isMounted) setAudioLevel(event.payload);
    }).then((nextUnlisten) => {
      if (!isMounted) {
        nextUnlisten();
        return;
      }
      unlisten = nextUnlisten;
    });

    return () => {
      isMounted = false;
      unlisten?.();
      setAudioLevel(0);
    };
  }, [isListening]);

  useEffect(() => {
    let isMounted = true;
    const unlistenFns: Array<() => void> = [];

    FORMATTING_EVENTS.forEach(([name, isActive]) => {
      void listen(name, () => {
        if (isMounted) setIsFormatting(isActive);
      }).then((unlisten) => {
        if (!isMounted) {
          unlisten();
          return;
        }
        unlistenFns.push(unlisten);
      });
    });

    return () => {
      isMounted = false;
      unlistenFns.forEach((unlisten) => unlisten());
    };
  }, []);

  const isActive = pillState !== "idle";
  const isVisible =
    pillIndicatorMode === "always" ||
    (pillIndicatorMode === "when_recording" && isActive);

  return {
    audioLevel: isListening ? audioLevel : 0,
    isActive,
    isVisible,
    pillState,
  };
}
