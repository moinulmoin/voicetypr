import { AudioWaveAnimation } from "@/components/AudioWaveAnimation";
import IOSSpinner from "@/components/ios-spinner";
import { Button } from "@/components/ui/button";
import { useRecording } from "@/hooks/useRecording";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

export function RecordingPill() {
  const recording = useRecording();
  const [audioLevel, setAudioLevel] = useState(0);

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
  );
}