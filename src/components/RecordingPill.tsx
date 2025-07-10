import { useEffect, useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Mic } from "lucide-react";
import { useRecording } from "@/hooks/useRecording";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import { cn } from "@/lib/utils";
import "../globals.css"; // Ensure CSS is imported

export function RecordingPill() {
  const recording = useRecording();
  const [audioLevel, setAudioLevel] = useState(0);
  const [autoInsert, setAutoInsert] = useState(true);
  const { registerEvent } = useEventCoordinator("pill");
  
  // Use ref to always have latest autoInsert value in event handlers
  const autoInsertRef = useRef(autoInsert);
  useEffect(() => {
    autoInsertRef.current = autoInsert;
  }, [autoInsert]);
  
  // Prevent duplicate processing
  const processingRef = useRef(false);

  // Load auto-insert setting on mount
  useEffect(() => {
    console.log("RecordingPill mounted");
    invoke("get_settings").then((settings: any) => {
      setAutoInsert(settings?.auto_insert ?? true);
    }).catch(console.error);

    return () => {
      console.log("RecordingPill unmounted");
    };
  }, []);

  // Register event handlers through coordinator - only once!
  useEffect(() => {
    console.log("[RecordingPill] Registering event handlers...");
    
    // Audio level events
    registerEvent<number>("audio-level", (level) => {
      setAudioLevel(level);
      console.log("[EventCoordinator] Audio level:", level);
    });

    // Transcription complete events
    registerEvent<{ text: string; model: string }>("transcription-complete", async ({ text }) => {
      // Guard against duplicate processing
      if (processingRef.current) {
        console.warn("[RecordingPill] Already processing transcription, skipping duplicate");
        return;
      }
      
      processingRef.current = true;
      
      console.log("[EventCoordinator] Pill window received transcription:", { 
        text: text.substring(0, 50) + "...",
        timestamp: new Date().toISOString()
      });
      
      try {
        // Always copy to clipboard first
        try {
          await navigator.clipboard.writeText(text);
          console.log("Text copied to clipboard successfully");
        } catch (e) {
          console.error("Failed to copy to clipboard:", e);
        }

        // Try to insert at cursor if auto-insert is enabled
        // Use ref to get latest value
        if (autoInsertRef.current) {
          console.log("Auto-insert is enabled, inserting text at cursor...");
          try {
            await invoke("insert_text", { text });
            console.log("Text inserted at cursor successfully");
          } catch (e) {
            console.error("Failed to insert text:", e);
          }
        } else {
          console.log("Auto-insert is disabled, skipping cursor insertion");
        }
      } finally {
        // Reset processing flag after a delay
        setTimeout(() => {
          processingRef.current = false;
        }, 1000);
      }

      // Close the pill widget after handling transcription
      setTimeout(async () => {
        try {
          // First ensure the pill window loses focus
          await invoke("hide_pill_widget");
          console.log("Pill widget hidden");

          // Small delay before closing
          setTimeout(async () => {
            await invoke("close_pill_widget");
            console.log("Pill widget closed after transcription");
          }, 100);
        } catch (e) {
          console.error("Failed to close pill widget:", e);
        }
      }, 1000); // Increased delay to ensure paste completes
    });
  }, [registerEvent]); // Only depend on registerEvent which is memoized

  // Handle click to stop recording
  const handleClick = async () => {
    if (recording.state === "recording") {
      console.log("Stopping recording from pill");
      await recording.stopRecording();
    }
  };

  // Calculate glow effect based on audio level
  // Make it MUCH more sensitive and visible
  const normalizedLevel = Math.min(audioLevel * 20, 1); // 20x amplification for better sensitivity
  const glowRadius = normalizedLevel > 0.05 ? 20 + (normalizedLevel * 40) : 0; // 20-60px range
  const glowOpacity = normalizedLevel > 0.05 ? 0.7 + (normalizedLevel * 0.3) : 0; // 0.7-1.0 opacity

  // Debug logging
  console.log("RecordingPill Debug:", {
    audioLevel,
    normalizedLevel,
    glowRadius,
    glowOpacity,
    recordingState: recording.state
  });

  const isTranscribing = recording.state === "transcribing";
  const isRecording = recording.state === "recording";

  // Build style objects to ensure they're valid
  const pillStyle = {
    boxShadow: "0 4px 12px rgba(0, 0, 0, 0.3)", // Default shadow
    background: "rgba(0, 0, 0, 0.95)" // Default background
  };

  if (isRecording && normalizedLevel > 0.05) {
    // Apply glow effect when recording and speaking
    const shadowString = `0 0 ${Math.round(glowRadius)}px rgba(255, 59, 48, ${glowOpacity.toFixed(2)}), 0 4px 12px rgba(0, 0, 0, 0.4)`;
    pillStyle.boxShadow = shadowString;

    const bgString = `radial-gradient(ellipse at center, rgba(255, 59, 48, ${(normalizedLevel * 0.15).toFixed(2)}) 0%, rgba(0, 0, 0, 0.95) 70%)`;
    pillStyle.background = bgString;

    console.log("Pill style applied:", { shadowString, bgString });
  }

  return (
    <div className="fixed inset-0 flex items-center justify-center pointer-events-none">
      <button
        onClick={handleClick}
        className={cn(
          "pointer-events-auto",
          "relative w-16 h-10 rounded-full",
          "bg-black/95",
          "flex items-center justify-center",
          "transition-all duration-200 ease-out",
          "shadow-md hover:shadow-lg",
          isRecording && normalizedLevel > 0.05 && "scale-110" // More noticeable scale when speaking
        )}
        style={pillStyle}
      >
        {/* Pulsing ring when recording */}
        {isRecording && (
          <div
            className="absolute inset-0 rounded-full"
            style={{
              background: normalizedLevel > 0.05
                ? `radial-gradient(ellipse at center, transparent 40%, rgba(255, 59, 48, ${(normalizedLevel * 0.3).toFixed(2)}) 100%)`
                : "none",
              animation: normalizedLevel > 0.05 ? "pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite" : "none"
            }}
          />
        )}

        {isTranscribing ? (
          <div className="w-4 h-4 relative z-10">
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
          <Mic
            className={cn(
              "w-4 h-4 relative z-10",
              isRecording ? "text-white" : "text-white/70"
            )}
            style={{
              filter: isRecording && normalizedLevel > 0.05
                ? `drop-shadow(0 0 ${normalizedLevel * 8}px rgba(255, 255, 255, 0.8))`
                : "none"
            }}
          />
        )}
      </button>

      {/* Debug info - remove in production */}
      {process.env.NODE_ENV === "development" && (
        <div className="absolute bottom-2 left-2 text-[10px] text-white/70 pointer-events-none bg-black/50 px-2 py-1 rounded">
          <div>State: {recording.state}</div>
          <div>Audio: {(audioLevel * 100).toFixed(1)}%</div>
          <div>Glow: {glowRadius.toFixed(0)}px</div>
        </div>
      )}
    </div>
  );
}