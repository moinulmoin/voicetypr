import { AudioWaveAnimation } from "@/components/AudioWaveAnimation";
import IOSSpinner from "@/components/ios-spinner";
import { Button } from "@/components/ui/button";
import { useSetting } from "@/contexts/SettingsContext";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import { useRecording } from "@/hooks/useRecording";
import { AlertCircle, Sparkles } from "lucide-react";
import { useEffect, useRef, useState } from "react";

export function RecordingPill() {
  const recording = useRecording();
  const [audioLevel, setAudioLevel] = useState(0);
  const [feedbackMessage, setFeedbackMessage] = useState<string>("");
  const [isCompact, setIsCompact] = useState(true);
  const [isEnhancing, setIsEnhancing] = useState(false);
  const { registerEvent } = useEventCoordinator("pill");

  // Track timer IDs for cleanup
  const feedbackTimerRef = useRef<NodeJS.Timeout | null>(null);

  const isRecording = recording.state === "recording";
  const isTranscribing = recording.state === "transcribing";

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (feedbackTimerRef.current) {
        clearTimeout(feedbackTimerRef.current);
      }
    };
  }, []);

  // Helper function to set feedback message with auto-hide
  const setFeedbackWithTimeout = (message: string, timeout: number) => {
    // Clear any existing timer
    if (feedbackTimerRef.current) {
      clearTimeout(feedbackTimerRef.current);
    }

    // Update state
    setFeedbackMessage(message);

    // Set new timer
    feedbackTimerRef.current = setTimeout(() => {
      setFeedbackMessage("");
      feedbackTimerRef.current = null;
    }, timeout);
  };

  // Use settings from context
  const compactRecordingStatus = useSetting('compact_recording_status');
  const recordingMode = useSetting('recording_mode');
  const isPushToTalk = recordingMode === 'push_to_talk';

  useEffect(() => {
    setIsCompact(compactRecordingStatus !== false);
  }, [compactRecordingStatus]);

  // Listen for audio level events
  useEffect(() => {
    if (!isRecording) {
      setAudioLevel(0);
      return;
    }

    let cancelled = false;
    const setup = async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const unlisten = await listen<number>("audio-level", (event) => {
        if (!cancelled) {
          setAudioLevel(event.payload);
        }
      });
      return unlisten;
    };

    let cleanup: (() => void) | undefined;
    setup().then((unlisten) => {
      cleanup = unlisten;
    });

    return () => {
      cancelled = true;
      if (cleanup) {
        cleanup();
      }
    };
  }, [isRecording]);

  // Listen for feedback events
  useEffect(() => {
    let cancelled = false;
    const setup = async () => {
      const unsubs: Array<() => void> = [];

      // Empty transcription
      unsubs.push(
        await registerEvent<string>("transcription-empty", (message) => {
          if (!cancelled) {
            setFeedbackWithTimeout(message, 2000);
          }
        })
      );

      // Recording stopped due to silence
      unsubs.push(
        await registerEvent("recording-stopped-silence", () => {
          if (!cancelled) {
            setFeedbackWithTimeout("No sound detected", 2000);
          }
        })
      );

      // ESC first press from backend
      unsubs.push(
        await registerEvent<string>("esc-first-press", (message) => {
          if (!cancelled) {
            setFeedbackWithTimeout(message, 3000);
          }
        })
      );

      // Recording errors
      unsubs.push(
        await registerEvent<string>("recording-error", (message) => {
          if (!cancelled) {
            setFeedbackWithTimeout(message || "Recording error occurred", 3000);
          }
        })
      );

      // Transcription errors
      unsubs.push(
        await registerEvent<string>("transcription-error", (message) => {
          if (!cancelled) {
            setFeedbackWithTimeout(message || "Transcription error occurred", 3000);
          }
        })
      );

      // No models error
      unsubs.push(
        await registerEvent<{ title: string; message: string; action: string }>(
          "no-models-error",
          (payload) => {
            if (!cancelled) {
              setFeedbackWithTimeout(payload.message, 4000);
            }
          }
        )
      );

      // Enhancing events
      unsubs.push(
        await registerEvent("enhancing-started", () => {
          if (!cancelled) {
            setIsEnhancing(true);
          }
        })
      );

      unsubs.push(
        await registerEvent("enhancing-completed", () => {
          if (!cancelled) {
            setIsEnhancing(false);
          }
        })
      );

      unsubs.push(
        await registerEvent<string>("enhancing-failed", () => {
          if (!cancelled) {
            setIsEnhancing(false);
            setFeedbackWithTimeout("Formatting failed", 2000);
          }
        })
      );

      // Generic formatting error
      unsubs.push(
        await registerEvent<string>("formatting-error", () => {
          if (!cancelled) {
            setIsEnhancing(false);
            setFeedbackWithTimeout("Formatting failed", 2000);
          }
        })
      );

      // Paste errors (accessibility permission)
      unsubs.push(
        await registerEvent<string>("paste-error", (message) => {
          if (!cancelled) {
            setFeedbackWithTimeout(message, 4000);
          }
        })
      );

      return () => {
        unsubs.forEach((fn) => fn());
      };
    };

    let cleanup: (() => void) | undefined;
    setup().then((fn) => {
      cleanup = fn;
    });

    return () => {
      cancelled = true;
      if (cleanup) {
        cleanup();
      }
    };
  }, [registerEvent]);


  // Handle click to stop recording
  // const handleClick = async () => {
  //   if (isRecording) {
  //     await invoke("stop_recording");
  //   }
  // };

  // Only show pill when recording, transcribing, enhancing, or showing feedback
  if (!isRecording && !isTranscribing && !isEnhancing && !feedbackMessage) {
    return null;
  }

  return (
    <div className="fixed inset-0 flex items-end justify-center pointer-events-none">
      <div className="relative pb-4">

        {/* Feedback message as overlay */}
        {feedbackMessage && (
          <div className="absolute inset-x-0 bottom-full mb-2 flex justify-center pointer-events-none z-50">
            <div className="bg-gray-900 text-white text-sm px-4 py-2 rounded-md shadow-lg whitespace-nowrap flex items-center gap-2">
              <AlertCircle className="w-4 h-4 text-amber-400" />
              <span>{feedbackMessage}</span>
            </div>
          </div>
        )}

        {/* Show button if actively recording/transcribing/enhancing, or invisible placeholder for feedback */}
        {(isRecording || isTranscribing || isEnhancing) ? (
          <Button
            // onClick={handleClick}
            variant="default"
            className={`${
              isCompact
                ? "rounded-full !p-1 w-10 h-10 shadow-none"
                : "rounded-xl !p-4 gap-2"
            } flex items-center justify-center`}
            // aria-readonly={isTranscribing}
          >
            {isEnhancing ? (
              <>
                <Sparkles size={isCompact ? 20 : 16} className="animate-pulse" />
                {!isCompact && "Enhancing"}
              </>
            ) : isTranscribing ? (
              <>
                <IOSSpinner size={isCompact ? 20 : 16} />
                {!isCompact && "Transcribing"}
              </>
            ) : (
              <>
                <AudioWaveAnimation audioLevel={audioLevel} className={isCompact ? "scale-80" : ""} />
                {!isCompact && (isPushToTalk ? "Release to stop" : "Listening")}
              </>
            )}
          </Button>
        ) : (
          /* Invisible placeholder to maintain position for feedback messages */
          <div className={`${isCompact ? "w-10 h-10" : "h-14"} invisible bg-transparent`} />
        )}
      </div>
    </div>
  );
}
