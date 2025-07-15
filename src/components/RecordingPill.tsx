import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { useRecording } from "@/hooks/useRecording";
import { cn } from "@/lib/utils";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";

export function RecordingPill() {
  const recording = useRecording();
  const { registerEvent } = useEventCoordinator("pill");
  const [audioLevel, setAudioLevel] = useState<number>(0);

  const isRecording = recording.state === "recording";
  const isTranscribing = recording.state === "transcribing";

  // Register event handlers
  useEffect(() => {
    // Audio level updates
    registerEvent<number>("audio-level", (level) => {
      setAudioLevel(level);
    });

    // Transcription error - just log it, backend handles hiding
    registerEvent<string>("transcription-error", (error) => {
      console.error("[RecordingPill] Transcription error:", error);
    });
    
    return () => {
      // Cleanup handled by useEventCoordinator
    };
  }, [registerEvent]);

  // Handle click to stop recording
  const handleClick = async () => {
    if (isRecording) {
      await invoke("stop_recording");
    }
  };

  // Normalize audio level (0-1 range)
  const normalizedLevel = Math.min(1, Math.max(0, audioLevel));

  // Dynamic styles based on audio level
  const pillStyle = {
    boxShadow: 
      isRecording && normalizedLevel > 0.05
        ? `0 0 ${30 + normalizedLevel * 20}px rgba(255, 59, 48, ${(0.4 + normalizedLevel * 0.3).toFixed(2)})`
        : "0 4px 12px rgba(0, 0, 0, 0.1)",
    background: 
      isRecording && normalizedLevel > 0.05
        ? `linear-gradient(135deg, rgba(255, 59, 48, ${(0.1 + normalizedLevel * 0.15).toFixed(2)}) 0%, rgba(0, 0, 0, 0.95) 100%)`
        : undefined,
  };

  return (
    <div className="fixed inset-0 flex items-center justify-center pointer-events-none">
      <button
        onClick={handleClick}
        className={cn(
          "pointer-events-auto",
          "relative w-48 h-14 rounded-full", // Increased size to match window
          "bg-black/95",
          "flex items-center justify-center gap-2", // Added gap for spacing
          "transition-all duration-200 ease-out",
          "shadow-lg hover:shadow-xl", // Stronger shadow for visibility
          "border border-white/10", // Add border for better visibility
          isRecording && normalizedLevel > 0.05 && "scale-110", // More noticeable scale when speaking
        )}
        style={pillStyle}
      >
        {/* Pulsing ring when recording */}
        {isRecording && (
          <div
            className="absolute inset-0 rounded-full"
            style={{
              background:
                normalizedLevel > 0.05
                  ? `radial-gradient(ellipse at center, transparent 40%, rgba(255, 59, 48, ${(normalizedLevel * 0.3).toFixed(2)}) 100%)`
                  : "none",
              animation:
                normalizedLevel > 0.05
                  ? "pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite"
                  : "none",
            }}
          />
        )}

        {isTranscribing ? (
          <div className="w-6 h-6 relative z-10">
            {/* macOS-style spinner */}
            <svg className="animate-spin" viewBox="0 0 24 24">
              <circle
                cx="12"
                cy="12"
                r="10"
                fill="none"
                stroke="rgba(255, 255, 255, 0.2)"
                strokeWidth="2.5"
              />
              <path
                d="M12 2 A10 10 0 0 1 22 12"
                fill="none"
                stroke="white"
                strokeWidth="2.5"
                strokeLinecap="round"
              />
            </svg>
          </div>
        ) : (
          <>
            {/* Recording indicator */}
            <div
              className={cn(
                "w-3 h-3 rounded-full relative z-10", // Increased size
                isRecording ? "bg-red-500" : "bg-green-500",
                isRecording && "animate-pulse",
              )}
              style={{
                boxShadow: isRecording
                  ? "0 0 10px rgba(239, 68, 68, 0.6)"
                  : "0 0 10px rgba(34, 197, 94, 0.6)",
              }}
            />
            
            {/* Label */}
            <span className="text-white/90 text-sm font-medium relative z-10">
              {isRecording ? "Recording..." : "Ready"}
            </span>
          </>
        )}
      </button>
    </div>
  );
}