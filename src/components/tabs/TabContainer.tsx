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
  const { registerEvent } = useEventCoordinator("main");

  // Load history function shared between overview and recordings tabs
  const loadHistory = useCallback(async () => {
    try {
      const storedHistory = await invoke<any[]>("get_transcription_history", {
        limit: 500  // Increased to ensure we get enough data for monthly stats
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

  // Load history on mount and listen for updates
  useEffect(() => {
    loadHistory();
    
    // Listen for new transcriptions (append-only for efficiency)
    registerEvent<{text: string; model: string; timestamp: string}>("transcription-added", (data) => {
      console.log("[TabContainer] New transcription added:", data.timestamp);
      const newItem: TranscriptionHistory = {
        id: data.timestamp,
        text: data.text,
        timestamp: new Date(data.timestamp),
        model: data.model
      };
      // Prepend new item to history (newest first)
      setHistory(prev => [newItem, ...prev]);
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
        return <OverviewTab history={history} />;

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
        return <OverviewTab history={history} />;
    }
  };

  return <div className="h-full flex flex-col">{renderTabContent()}</div>;
}
