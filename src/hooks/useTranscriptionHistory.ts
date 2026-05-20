import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import type { TranscriptionHistory } from "@/types";

interface RawTranscriptionHistoryItem {
  timestamp?: string;
  text: string;
  model: string;
  recording_file?: string;
  source_recording_id?: string;
  status?: TranscriptionHistory["status"];
}

interface TranscriptionAddedEvent {
  timestamp: string;
  text: string;
  model: string;
  recording_file?: string;
  source_recording_id?: string;
  status?: TranscriptionHistory["status"];
}

interface UseTranscriptionHistoryOptions {
  limit: number;
  includeTotalCount?: boolean;
}

interface UseTranscriptionHistoryResult {
  history: TranscriptionHistory[];
  totalCount: number;
  refreshHistory: () => Promise<void>;
}

function toHistoryItem(item: RawTranscriptionHistoryItem): TranscriptionHistory {
  const timestamp = item.timestamp ?? Date.now().toString();

  return {
    id: timestamp,
    text: item.text,
    timestamp: new Date(timestamp),
    model: item.model,
    recording_file: item.recording_file,
    source_recording_id: item.source_recording_id,
    status: item.status,
  };
}

function fromAddedEvent(item: TranscriptionAddedEvent): TranscriptionHistory {
  return {
    id: item.timestamp,
    text: item.text,
    timestamp: new Date(item.timestamp),
    model: item.model,
    recording_file: item.recording_file,
    source_recording_id: item.source_recording_id,
    status: item.status,
  };
}

export function useTranscriptionHistory({
  limit,
  includeTotalCount = false,
}: UseTranscriptionHistoryOptions): UseTranscriptionHistoryResult {
  const { registerEvent } = useEventCoordinator("main");
  const [history, setHistory] = useState<TranscriptionHistory[]>([]);
  const [totalCount, setTotalCount] = useState(0);

  const refreshHistory = useCallback(async () => {
    try {
      const historyPromise = invoke<RawTranscriptionHistoryItem[]>(
        "get_transcription_history",
        { limit },
      );
      const countPromise = includeTotalCount
        ? invoke<number>("get_transcription_count")
        : Promise.resolve<number | null>(null);

      const [storedHistory, count] = await Promise.all([
        historyPromise,
        countPromise,
      ]);

      const formattedHistory = storedHistory.map(toHistoryItem);
      setHistory(formattedHistory);
      setTotalCount(count ?? formattedHistory.length);
    } catch (error) {
      console.error("Failed to load transcription history:", error);
    }
  }, [includeTotalCount, limit]);

  useEffect(() => {
    const loadInitialHistory = async () => {
      await refreshHistory();
    };

    void loadInitialHistory();

    registerEvent<TranscriptionAddedEvent>("transcription-added", (data) => {
      const newItem = fromAddedEvent(data);

      setHistory((previous) => {
        if (previous.some((item) => item.id === newItem.id)) {
          return previous;
        }

        if (includeTotalCount) {
          setTotalCount((count) => count + 1);
        }

        return [newItem, ...previous].slice(0, limit);
      });
    });

    registerEvent("history-updated", () => {
      void refreshHistory();
    });

    registerEvent("transcription-updated", () => {
      void refreshHistory();
    });
  }, [includeTotalCount, limit, refreshHistory, registerEvent]);

  return {
    history,
    totalCount,
    refreshHistory,
  };
}
