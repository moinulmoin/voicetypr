// Direct imports for instant desktop app experience
import { AccountTab } from "./AccountTab";
import { AdvancedTab } from "./AdvancedTab";
import { EnhancementsTab } from "./EnhancementsTab";
import { ModelsTab } from "./ModelsTab";
import { OverviewTab } from "./OverviewTab";
import { RecordingsTab } from "./RecordingsTab";
import { SettingsTab } from "./SettingsTab";
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TranscriptionHistory } from "@/types";

interface TabContainerProps {
  activeSection: string;
}

export function TabContainer({ activeSection }: TabContainerProps) {
  const [history, setHistory] = useState<TranscriptionHistory[]>([]);

  // Load history function shared between overview and recordings tabs
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

  // Load history on mount
  useEffect(() => {
    loadHistory();
  }, [loadHistory]);

  const renderTabContent = () => {
    switch (activeSection) {
      case "overview":
        return <OverviewTab history={history} />;
        
      case "recordings":
        return <RecordingsTab />;

      case "general":
        return <SettingsTab />;

      case "models":
        return <ModelsTab />;

      case "advanced":
        return <AdvancedTab />;

      case "formatting":
        return <EnhancementsTab />;

      case "account":
      case "about":
      case "license":
        return <AccountTab />;

      default:
        return <OverviewTab history={history} />;
    }
  };

  return <div className="h-full flex flex-col">{renderTabContent()}</div>;
}
