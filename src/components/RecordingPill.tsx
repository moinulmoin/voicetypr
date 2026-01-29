import { AudioDots } from "@/components/AudioDots";
import { useSetting } from "@/contexts/SettingsContext";
import { useRecording } from "@/hooks/useRecording";
import { PillIndicatorMode } from "@/types";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { motion } from "framer-motion";

type PillState = "idle" | "listening" | "transcribing" | "formatting";

export function RecordingPill() {
  const recording = useRecording();
  const [audioLevel, setAudioLevel] = useState(0);
  const [isFormatting, setIsFormatting] = useState(false);

  // Setting: pill indicator mode (default: "when_recording")
  const pillIndicatorMode: PillIndicatorMode = useSetting("pill_indicator_mode") ?? "when_recording";

  // Determine pill state
  const getPillState = (): PillState => {
    if (isFormatting) return "formatting";
    if (recording.state === "recording") return "listening";
    if (recording.state === "transcribing" || recording.state === "stopping")
      return "transcribing";
    return "idle";
  };

  const pillState = getPillState();
  const isListening = pillState === "listening";
  const isActive = pillState !== "idle";

  // Listen for audio level events
  useEffect(() => {
    if (isListening) {
      let isMounted = true;
      let unlistenFn: (() => void) | undefined;

      listen<number>("audio-level", (event) => {
        if (isMounted) setAudioLevel(event.payload);
      }).then((unlisten) => {
        if (!isMounted) {
          unlisten();
          return;
        }
        unlistenFn = unlisten;
      });

      return () => {
        isMounted = false;
        if (unlistenFn) unlistenFn();
        setAudioLevel(0);
      };
    } else {
      const timeoutId = setTimeout(() => setAudioLevel(0), 0);
      return () => clearTimeout(timeoutId);
    }
  }, [isListening]);

  // Listen for formatting/enhancement events (global events from backend)
  useEffect(() => {
    let isMounted = true;
    const unlistenFns: (() => void)[] = [];

    const events = [
      { name: "enhancing-started", handler: () => {
        if (isMounted) setIsFormatting(true);
      }},
      { name: "enhancing-completed", handler: () => {
        if (isMounted) setIsFormatting(false);
      }},
      { name: "enhancing-failed", handler: () => {
        if (isMounted) setIsFormatting(false);
      }},
    ];

    events.forEach(({ name, handler }) => {
      listen(name, handler).then((unlisten) => {
        if (!isMounted) {
          unlisten();
          return;
        }
        unlistenFns.push(unlisten);
      });
    });

    return () => {
      isMounted = false;
      unlistenFns.forEach((fn) => fn());
    };
  }, []);

  // Determine if pill should be hidden based on mode and state
  // "never" → always hide
  // "always" → never hide (always show)
  // "when_recording" → hide when idle
  const shouldHide =
    pillIndicatorMode === "never" ||
    (pillIndicatorMode === "when_recording" && pillState === "idle");

  if (shouldHide) {
    return null;
  }

  return (
    <div className="fixed inset-0 flex items-center justify-center">
      {/* Solid black pill - grows when active */}
      <motion.div
        className="flex items-center justify-center rounded-full select-none bg-black shadow-lg"
        animate={{
          // ~1.4x growth from idle to active
          paddingLeft: isActive ? 14 : 10,
          paddingRight: isActive ? 14 : 10,
          paddingTop: isActive ? 7 : 5,
          paddingBottom: isActive ? 7 : 5,
        }}
        transition={{
          duration: 0.25,
          ease: "easeOut",
        }}
      >
        <AudioDots state={pillState} audioLevel={audioLevel} />
      </motion.div>
    </div>
  );
}
