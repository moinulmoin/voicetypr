import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
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
        model: item.model,
        recording_file: item.recording_file,
        source_recording_id: item.source_recording_id,
        status: item.status
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

        // Listen for new transcriptions (append-only for efficiency)
        registerEvent<{text: string; model: string; timestamp: string; recording_file?: string; status?: string}>("transcription-added", (data) => {
          console.log("[RecordingsTab] New transcription added:", data.timestamp);
          const newItem: TranscriptionHistory = {
            id: data.timestamp,
            text: data.text,
            timestamp: new Date(data.timestamp),
            model: data.model,
            recording_file: data.recording_file,
            status: data.status as TranscriptionHistory['status']
          };
          // Prepend new item to history (newest first), avoiding duplicates
          setHistory(prev => {
            // Check if item already exists (by ID)
            if (prev.some(item => item.id === newItem.id)) {
              console.log("[RecordingsTab] Skipping duplicate item:", newItem.id);
              return prev;
            }
            return [newItem, ...prev];
          });
        });
        
        // Listen for history-updated for delete/clear operations
        registerEvent("history-updated", async () => {
          console.log("[RecordingsTab] Full reload (delete/clear operation)");
          await loadHistory();
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
