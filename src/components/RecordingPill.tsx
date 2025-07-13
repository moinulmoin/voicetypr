import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { useRecording } from "@/hooks/useRecording";
import { cn } from "@/lib/utils";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";

export function RecordingPill() {
  const recording = useRecording();
  const { registerEvent, getDebugInfo } = useEventCoordinator("pill");
  const [audioLevel, setAudioLevel] = useState<number>(0);
  const autoInsertRef = useRef<boolean>(true);
  
  // Keep a ref to prevent duplicate processing
  const processingRef = useRef(false);

  const isRecording = recording.state === "recording";
  const isTranscribing = recording.state === "transcribing";
  
  // Log state changes for debugging
  useEffect(() => {
    console.log("[DEBUG] RecordingPill state changed:", recording.state);
  }, [recording.state]);
  
  // Log when component mounts
  useEffect(() => {
    console.log("[DEBUG] RecordingPill component mounted");
    console.log("[DEBUG] EventCoordinator info:", getDebugInfo());
    
    return () => {
      console.log("[DEBUG] RecordingPill component unmounting");
    };
  }, []);

  // Load auto-insert setting
  useEffect(() => {
    invoke<{ auto_insert: boolean }>("get_settings")
      .then((settings) => {
        autoInsertRef.current = settings.auto_insert;
      })
      .catch(console.error);

    return () => {
      // Cleanup
    };
  }, []);

  // Register event handlers
  useEffect(() => {
    console.log("[DEBUG] RecordingPill mounting, registering event handlers");
    
    // Test event handler for debugging
    registerEvent("test-event", (payload) => {
      console.log("[DEBUG] RecordingPill received test-event:", payload);
    });
    
    // Audio level updates
    registerEvent<number>("audio-level", (level) => {
      setAudioLevel(level);
    });

    // Transcription error - hide pill and reset
    registerEvent<string>("transcription-error", async (error) => {
      console.error("[DEBUG] Transcription error received:", error);
      
      try {
        await invoke("hide_pill_widget");
      } catch (e) {
        console.error("Failed to hide pill widget on error:", e);
      }
      
      // Reset processing flag
      processingRef.current = false;
    });
    
    // Transcription complete - handle clipboard and auto-insert
    console.log("[DEBUG] Registering transcription-complete handler");
    registerEvent<{ text: string; model: string }>(
      "transcription-complete",
      async ({ text, model }) => {
        console.log("[DEBUG] transcription-complete event received:", {
          timestamp: Date.now(),
          textLength: text.length,
          processing: processingRef.current,
          eventCoordinatorInfo: getDebugInfo()
        });
        
        // Guard against duplicate processing
        if (processingRef.current) {
          console.warn("[DEBUG] Skipping duplicate transcription-complete event");
          return;
        }
        
        processingRef.current = true;
        console.log("[DEBUG] Processing transcription...");
        
        try {
          // Hide the pill FIRST to ensure clean text insertion
          try {
            await invoke("hide_pill_widget");
          } catch (e) {
            console.error("Failed to hide pill widget:", e);
          }
          
          // Longer delay to ensure window is fully hidden and focus is properly restored
          // This prevents text duplication issues on macOS
          await new Promise(resolve => setTimeout(resolve, 200));
          
          // Now insert at cursor if auto-insert is enabled
          if (autoInsertRef.current) {
            console.log("[DEBUG] Calling insert_text with", text.length, "characters");
            try {
              await invoke("insert_text", { text });
              console.log("[DEBUG] insert_text completed successfully");
            } catch (e) {
              console.error("Failed to insert text:", e);
            }
          } else {
            // If auto-insert is disabled, just copy to clipboard
            console.log("[DEBUG] Auto-insert disabled, copying to clipboard only");
            try {
              await navigator.clipboard.writeText(text);
            } catch (e) {
              console.error("Failed to copy to clipboard:", e);
            }
          }

          // Save transcription to backend AFTER paste/clipboard operations
          try {
            await invoke("save_transcription", { text, model });
          } catch (e) {
            console.error("Failed to save transcription:", e);
          }
          
          // Notify backend that pill has finished processing
          // This allows backend to transition to Idle state
          console.log("[DEBUG] Notifying backend that transcription processing is complete");
          try {
            await invoke("transcription_processed");
            console.log("[DEBUG] Backend notified successfully");
          } catch (e) {
            console.error("Failed to notify backend of processing completion:", e);
          }
        } finally {
          // Reset processing flag after a delay
          setTimeout(() => {
            processingRef.current = false;
          }, 1000);
        }
      },
    );
    
    return () => {
      // Cleanup handled by useEventCoordinator
    };
  }, [registerEvent]); // Only depend on registerEvent which is memoized

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