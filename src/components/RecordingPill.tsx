import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { Mic, Loader2 } from "lucide-react";
import { useRecording } from "@/hooks/useRecording";
import { cn } from "@/lib/utils";

export function RecordingPill() {
  const recording = useRecording();
  const [audioLevel, setAudioLevel] = useState(0);
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });

  // Listen for audio level events
  useEffect(() => {
    const unlisten = listen<number>("audio-level", (event) => {
      setAudioLevel(event.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Handle dragging
  const handleMouseDown = (e: React.MouseEvent) => {
    setIsDragging(true);
    setDragStart({ x: e.clientX, y: e.clientY });
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    if (!isDragging) return;
    
    const deltaX = e.clientX - dragStart.x;
    const deltaY = e.clientY - dragStart.y;
    
    // Update window position
    invoke("update_pill_position", {
      x: window.screenX + deltaX,
      y: window.screenY + deltaY,
    });
    
    setDragStart({ x: e.clientX, y: e.clientY });
  };

  const handleMouseUp = () => {
    setIsDragging(false);
  };

  // Handle click to stop recording
  const handleClick = async () => {
    if (recording.state === "recording") {
      await recording.stopRecording();
    }
  };

  // Calculate glow intensity based on audio level
  const glowIntensity = Math.min(audioLevel * 2, 1); // Scale up for visibility
  const glowSize = glowIntensity * 20; // Max 20px glow

  const isTranscribing = recording.state === "transcribing";
  const isRecording = recording.state === "recording";

  return (
    <div
      className={cn(
        "fixed inset-0 flex items-center justify-center cursor-move select-none",
        isDragging && "cursor-grabbing"
      )}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
    >
      <button
        onClick={handleClick}
        className={cn(
          "relative w-[60px] h-[36px] rounded-full",
          "bg-black/80 backdrop-blur-md",
          "border border-white/10",
          "flex items-center justify-center",
          "transition-all duration-200",
          "hover:bg-black/90",
          isRecording && "animate-pulse"
        )}
        style={{
          boxShadow: isRecording
            ? `0 0 ${glowSize}px rgba(239, 68, 68, ${glowIntensity * 0.6})`
            : "none",
        }}
      >
        {isTranscribing ? (
          <div className="relative w-5 h-5">
            {/* macOS-style spinner */}
            <div className="absolute inset-0">
              <svg
                className="animate-spin"
                viewBox="0 0 20 20"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
              >
                <circle
                  cx="10"
                  cy="10"
                  r="8"
                  stroke="currentColor"
                  strokeWidth="2"
                  className="text-white/20"
                />
                <path
                  d="M10 2a8 8 0 0 1 8 8"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  className="text-white"
                />
              </svg>
            </div>
          </div>
        ) : (
          <Mic
            className={cn(
              "w-4 h-4",
              isRecording ? "text-red-400" : "text-white/70"
            )}
          />
        )}
      </button>
      
      {/* Error state indicator */}
      {recording.error && (
        <div className="absolute inset-0 rounded-full bg-red-500/20 animate-ping" />
      )}
    </div>
  );
}