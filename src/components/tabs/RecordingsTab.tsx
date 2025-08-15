import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { RecentRecordings } from "../sections/RecentRecordings";
import { useSettings } from "@/contexts/SettingsContext";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import { TranscriptionHistory } from "@/types";

export function RecordingsTab() {
  const { registerEvent } = useEventCoordinator("main");
  const [history, setHistory] = useState<TranscriptionHistory[]>([]);
  const { settings } = useSettings();

  // Load history function
  const loadHistory = useCallback(async () => {
    try {
      const storedHistory = await invoke<any[]>("get_transcription_history", {
        limit: 50
      });
      const formattedHistory: TranscriptionHistory[] = storedHistory.map((item) => ({
        id: item.timestamp || Date.now().toString(),
        text: item.text,
        timestamp: new Date(item.timestamp),
        model: item.model
      }));
      setHistory(formattedHistory);
    } catch (error) {
      console.error("Failed to load transcription history:", error);
    }
  }, []);

  // Initialize recordings tab
  useEffect(() => {
    const init = async () => {
      try {
        // Load initial transcription history
        await loadHistory();

        // Listen for history updates from backend
        // Backend is the single source of truth for transcription history
        registerEvent("history-updated", async () => {
          console.log("[EventCoordinator] Recordings tab: reloading history after update");
          await loadHistory();
        });

        // Listen for recording errors
        registerEvent<string>("recording-error", (errorMessage) => {
          console.error("Recording error:", errorMessage);
          
          toast.error('Recording Failed', {
            description: errorMessage || 'An error occurred during recording. Please try again.',
            action: {
              label: 'Try Again',
              onClick: () => {
                invoke('start_recording').catch(console.error);
              }
            },
            duration: 6000
          });
        });

        // Listen for transcription errors
        registerEvent<string>("transcription-error", (errorMessage) => {
          console.error("Transcription error:", errorMessage);
          
          toast.error('Transcription Failed', {
            description: errorMessage || 'An error occurred during transcription. Please try again.',
            action: {
              label: 'Try Again',
              onClick: () => {
                invoke('start_recording').catch(console.error);
              }
            },
            duration: 6000
          });
        });
      } catch (error) {
        console.error("Failed to initialize recordings tab:", error);
      }
    };

    init();
  }, [registerEvent, loadHistory]);

  return (
    <RecentRecordings
      history={history}
      hotkey={settings?.hotkey || "Cmd+Shift+Space"}
      onHistoryUpdate={loadHistory}
    />
  );
}