import IOSSpinner from "@/components/ios-spinner";
import { Button } from "@/components/ui/button";
import { useRecording } from "@/hooks/useRecording";
import { AudioLines } from "lucide-react";

export function RecordingPill() {
  const recording = useRecording();

  // const isRecording = recording.state === "recording";
  const isTranscribing = recording.state === "transcribing";

  // Handle click to stop recording
  // const handleClick = async () => {
  //   if (isRecording) {
  //     await invoke("stop_recording");
  //   }
  // };

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
            <IOSSpinner size={16} className="mr-2" />
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