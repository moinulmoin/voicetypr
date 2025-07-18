import { AudioWaveAnimation } from "@/components/AudioWaveAnimation";
import IOSSpinner from "@/components/ios-spinner";
import { Button } from "@/components/ui/button";
import { useRecording } from "@/hooks/useRecording";
import { AppSettings } from "@/types";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AlertCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";

export function RecordingPill() {
  const recording = useRecording();
  const [audioLevel, setAudioLevel] = useState(0);
  const [feedbackMessage, setFeedbackMessage] = useState<string>("");
  const [isCompact, setIsCompact] = useState(true);
  
  // Track timer IDs for cleanup
  const feedbackTimerRef = useRef<NodeJS.Timeout | null>(null);
  const mountedRef = useRef(true);

  const isRecording = recording.state === "recording";
  const isTranscribing = recording.state === "transcribing";
  
  // Cleanup on unmount
  useEffect(() => {
    return () => {
      mountedRef.current = false;
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
    
    // Only update state if component is still mounted
    if (mountedRef.current) {
      setFeedbackMessage(message);
      
      // Set new timer
      feedbackTimerRef.current = setTimeout(() => {
        if (mountedRef.current) {
          setFeedbackMessage("");
        }
        feedbackTimerRef.current = null;
      }, timeout);
    }
  };

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
        setFeedbackWithTimeout(event.payload, 2000);
      })
    );

    // Listen for recording stopped due to silence
    unlisteners.push(
      listen("recording-stopped-silence", () => {
        setFeedbackWithTimeout("Recording stopped - no sound detected", 2000);
      })
    );

    // Listen for ESC first press from backend
    unlisteners.push(
      listen<string>("esc-first-press", (event) => {
        setFeedbackWithTimeout(event.payload, 3000);
      })
    );
    
    // Listen for recording errors
    unlisteners.push(
      listen<string>("recording-error", (event) => {
        setFeedbackWithTimeout(event.payload || "Recording error occurred", 3000);
      })
    );
    
    // Listen for transcription errors
    unlisteners.push(
      listen<string>("transcription-error", (event) => {
        setFeedbackWithTimeout(event.payload || "Transcription error occurred", 3000);
      })
    );
    
    // Listen for no models error
    unlisteners.push(
      listen<{ title: string; message: string; action: string }>("no-models-error", (event) => {
        setFeedbackWithTimeout(event.payload.message, 4000);
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