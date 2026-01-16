// Direct imports for instant desktop app experience
import { AboutTab } from "./AboutTab";
import { AccountTab } from "./AccountTab";
import { AdvancedTab } from "./AdvancedTab";
import { EnhancementsTab } from "./EnhancementsTab";
import { HelpTab } from "./HelpTab";
import { ModelsTab } from "./ModelsTab";
import { OverviewTab } from "./OverviewTab";
import { RecordingsTab } from "./RecordingsTab";
import { SettingsTab } from "./SettingsTab";
import { AudioUploadSection } from "../sections/AudioUploadSection";
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TranscriptionHistory } from "@/types";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";

interface TabContainerProps {
  activeSection: string;
}

export function TabContainer({ activeSection }: TabContainerProps) {
  const [history, setHistory] = useState<TranscriptionHistory[]>([]);
  const [totalCount, setTotalCount] = useState<number>(0);
  const { registerEvent } = useEventCoordinator("main");

  // Load history function shared between overview and recordings tabs
  const loadHistory = useCallback(async () => {
    try {
      // Fetch both history (limited for stats calculation) and total count
      const [storedHistory, count] = await Promise.all([
        invoke<any[]>("get_transcription_history", {
          limit: 500  // Limited for monthly stats calculation
        }),
        invoke<number>("get_transcription_count")  // Get true total count
      ]);

      const formattedHistory: TranscriptionHistory[] = storedHistory.map((item) => ({
        id: item.timestamp || Date.now().toString(),
        text: item.text,
        timestamp: new Date(item.timestamp),
        model: item.model
      }));
      setHistory(formattedHistory);
      setTotalCount(count);
    } catch (error) {
      console.error("Failed to load transcription history:", error);
    }
  }, []);

  // Load history on mount and listen for updates
  useEffect(() => {
    const run = async () => {
      await loadHistory();
    };

    void run();
    
    // Listen for new transcriptions (append-only for efficiency)
    registerEvent<{text: string; model: string; timestamp: string}>("transcription-added", (data) => {
      console.log("[TabContainer] New transcription added:", data.timestamp);
      const newItem: TranscriptionHistory = {
        id: data.timestamp,
        text: data.text,
        timestamp: new Date(data.timestamp),
        model: data.model
      };
      // Prepend new item to history (newest first), avoiding duplicates
      setHistory(prev => {
        // Check if item already exists (by ID)
        if (prev.some(item => item.id === newItem.id)) {
          console.log("[TabContainer] Skipping duplicate item:", newItem.id);
          return prev;
        }
        // Only increment count when actually adding a new item
        setTotalCount(count => count + 1);
        return [newItem, ...prev];
      });
    });
    
    // Listen for history-updated only for delete/clear operations
    // (These still emit history-updated from backend)
    registerEvent("history-updated", async () => {
      console.log("[TabContainer] Full history reload (delete/clear operation)");
      await loadHistory();
    });
  }, [loadHistory, registerEvent]);

  const renderTabContent = () => {
    switch (activeSection) {
      case "overview":
        return <OverviewTab history={history} totalCount={totalCount} />;

      case "recordings":
        return <RecordingsTab />;

      case "audio":
        return <AudioUploadSection />;

      case "general":
        return <SettingsTab />;

      case "models":
        return <ModelsTab />;

      case "advanced":
        return <AdvancedTab />;

      case "formatting":
        return <EnhancementsTab />;

      case "license":
        return <AccountTab />;

      case "help":
        return <HelpTab />;

      case "about":
        return <AboutTab />;

      default:
        return <OverviewTab history={history} totalCount={totalCount} />;
    }
  };

  return <div className="h-full flex flex-col">{renderTabContent()}</div>;
}
