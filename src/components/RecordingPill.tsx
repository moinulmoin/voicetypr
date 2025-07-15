import { Button } from "@/components/ui/button";
import { useRecording } from "@/hooks/useRecording";
import { invoke } from "@tauri-apps/api/core";
import { AudioLines, Loader2 } from "lucide-react";

export function RecordingPill() {
  const recording = useRecording();

  const isRecording = recording.state === "recording";
  const isTranscribing = recording.state === "transcribing";

  // Handle click to stop recording
  const handleClick = async () => {
    if (isRecording) {
      await invoke("stop_recording");
    }
  };

  return (
    <div className="fixed inset-0 flex items-center justify-center pointer-events-none">
      <Button
        // onClick={handleClick}
        variant="default"
        className="rounded-xl !p-4"
        // aria-readonly={isTranscribing}
      >
        {isTranscribing ? (
          <>
            <Loader2 className="animate-spin" />
            Transcribing
          </>
        ) : (
          <>
            <AudioLines />
            Listening
          </>
        )}
      </Button>
    </div>
  );
}