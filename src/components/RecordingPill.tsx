import { AudioWaveAnimation } from "@/components/AudioWaveAnimation";
import IOSSpinner from "@/components/ios-spinner";
import { Button } from "@/components/ui/button";
import { useRecording } from "@/hooks/useRecording";
import { AppSettings } from "@/types";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AlertCircle } from "lucide-react";
import { useEffect, useState } from "react";

export function RecordingPill() {
  const recording = useRecording();
  const [audioLevel, setAudioLevel] = useState(0);
  const [feedbackMessage, setFeedbackMessage] = useState<string>("");
  const [isCompact, setIsCompact] = useState(true);

  const isRecording = recording.state === "recording";
  const isTranscribing = recording.state === "transcribing";

  // Fetch settings on mount
  useEffect(() => {
    invoke<AppSettings>("get_settings").then((settings) => {
      setIsCompact(settings.compact_recording_status !== false);
    }).catch(console.error);
  }, []);

  // Listen for audio level events
  useEffect(() => {
    if (!isRecording) {
      setAudioLevel(0);
      return;
    }

    const unlisten = listen<number>("audio-level", (event) => {
      setAudioLevel(event.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [isRecording]);

  // Listen for feedback events
  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [];

    // Listen for empty transcription
    unlisteners.push(
      listen<string>("transcription-empty", (event) => {
        setFeedbackMessage(event.payload);
        // Auto-hide after 2 seconds
        setTimeout(() => setFeedbackMessage(""), 2000);
      })
    );

    // Listen for recording stopped due to silence
    unlisteners.push(
      listen("recording-stopped-silence", () => {
        setFeedbackMessage("Recording stopped - no sound detected");
        // Auto-hide after 2 seconds
        setTimeout(() => setFeedbackMessage(""), 2000);
      })
    );

    // Listen for ESC first press from backend
    unlisteners.push(
      listen<string>("esc-first-press", (event) => {
        setFeedbackMessage(event.payload);
        // Auto-hide after 3 seconds
        setTimeout(() => setFeedbackMessage(""), 3000);
      })
    );

    return () => {
      unlisteners.forEach((unlisten) => unlisten.then((fn) => fn()));
    };
  }, []);


  // Handle click to stop recording
  // const handleClick = async () => {
  //   if (isRecording) {
  //     await invoke("stop_recording");
  //   }
  // };

  // // Only show pill when recording or transcribing
  // if (!isRecording && !isTranscribing) {
  //   return null;
  // }

  return (
    <div className="fixed inset-0 flex items-end justify-center pointer-events-none">
      <div className="relative">
        {/* Feedback message - white background with alert icon */}
        {feedbackMessage && (
          <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-4 pointer-events-auto animate-in fade-in slide-in-from-bottom-2 duration-200">
            <div className="bg-white border border-gray-200 rounded-lg shadow-md overflow-hidden whitespace-nowrap">
              <div className="flex items-center gap-3 px-4 py-3">
                <AlertCircle className="size-4 text-amber-500 flex-shrink-0" />
                <span className="text-sm text-gray-700 font-medium whitespace-nowrap">{feedbackMessage}</span>
              </div>
            </div>
          </div>
        )}

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
          {isTranscribing ? (
            <>
              <IOSSpinner size={isCompact ? 20 : 16} />
              {!isCompact && "Transcribing"}
            </>
          ) : (
            <>
              <AudioWaveAnimation audioLevel={audioLevel} className={isCompact ? "scale-80" : ""} />
              {!isCompact && "Listening"}
            </>
          )}
        </Button>
      </div>
    </div>
  );
}