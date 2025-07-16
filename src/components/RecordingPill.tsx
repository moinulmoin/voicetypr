import { AudioWaveAnimation } from "@/components/AudioWaveAnimation";
import IOSSpinner from "@/components/ios-spinner";
import { Button } from "@/components/ui/button";
import { useRecording } from "@/hooks/useRecording";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

export function RecordingPill() {
  const recording = useRecording();
  const [audioLevel, setAudioLevel] = useState(0);
  const [feedbackMessage, setFeedbackMessage] = useState<string>("");

  const isRecording = recording.state === "recording";
  const isTranscribing = recording.state === "transcribing";

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

  return (
    <div className="fixed inset-0 flex items-center justify-center pointer-events-none">
      <div className="relative">
        {/* Feedback message tooltip - positioned with more padding to ensure visibility */}
        {feedbackMessage && (
          <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-3 px-4 py-2.5 bg-gray-900 text-white text-sm rounded-lg shadow-lg whitespace-nowrap transition-opacity duration-300 ease-in-out z-50 min-w-[200px] text-center">
            {feedbackMessage}
            {/* Small arrow pointing down */}
            <div className="absolute top-full left-1/2 -translate-x-1/2 -mt-px">
              <div className="w-0 h-0 border-l-[6px] border-l-transparent border-r-[6px] border-r-transparent border-t-[6px] border-t-gray-900" />
            </div>
          </div>
        )}
        
        <Button
          // onClick={handleClick}
          variant="default"
          className="rounded-xl !p-4 flex items-center justify-center gap-2"
          // aria-readonly={isTranscribing}
        >
          {isTranscribing ? (
            <>
              <IOSSpinner size={16} className="-mt-[3px]" />
              Transcribing
            </>
          ) : (
            <>
              <AudioWaveAnimation audioLevel={audioLevel} className="" />
              Listening
            </>
          )}
        </Button>
      </div>
    </div>
  );
}