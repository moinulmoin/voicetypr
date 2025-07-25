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
  const [, forceUpdate] = useState({});
  
  // Track timer IDs for cleanup
  const feedbackTimerRef = useRef<NodeJS.Timeout | null>(null);
  const mountedRef = useRef(true);
  const feedbackMessageRef = useRef<string>("");
  
  // Debug re-renders
  useEffect(() => {
    console.log("RecordingPill: Component re-rendered, feedbackMessage:", feedbackMessage, "ref:", feedbackMessageRef.current);
  });

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
    console.log("RecordingPill: Setting feedback message:", message, "for", timeout, "ms", "mountedRef:", mountedRef.current);
    
    // Clear any existing timer
    if (feedbackTimerRef.current) {
      clearTimeout(feedbackTimerRef.current);
    }
    
    // Remove the mounted check - just set the state directly
    // Update ref
    feedbackMessageRef.current = message;
    
    // Force a fresh state update
    setFeedbackMessage(message);
    // Also force update to ensure re-render
    forceUpdate({});
    
    console.log("RecordingPill: Updated state and ref to:", message);
    
    // Set new timer
    feedbackTimerRef.current = setTimeout(() => {
      console.log("RecordingPill: Clearing feedback message");
      feedbackMessageRef.current = "";
      setFeedbackMessage("");
      forceUpdate({});
      feedbackTimerRef.current = null;
    }, timeout);
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
        console.log("RecordingPill: Received transcription-empty event", event.payload);
        setFeedbackWithTimeout(event.payload, 2000);
      })
    );

    // Listen for recording stopped due to silence
    unlisteners.push(
      listen("recording-stopped-silence", () => {
        console.log("RecordingPill: Received recording-stopped-silence event");
        setFeedbackWithTimeout("Recording stopped - no sound detected", 2000);
      })
    );

    // Listen for ESC first press from backend
    unlisteners.push(
      listen<string>("esc-first-press", (event) => {
        console.log("RecordingPill: Received esc-first-press event", event.payload);
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