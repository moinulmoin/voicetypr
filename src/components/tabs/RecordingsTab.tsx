import { useSettings } from "@/contexts/SettingsContext";
import { useTranscriptionHistory } from "@/hooks/useTranscriptionHistory";
import { RecentRecordings } from "../sections/RecentRecordings";

export function RecordingsTab() {
  const { settings } = useSettings();
  const { history, refreshHistory } = useTranscriptionHistory({ limit: 50 });

  return (
    <RecentRecordings
      history={history}
      hotkey={settings?.hotkey || "Cmd+Shift+Space"}
      onHistoryUpdate={refreshHistory}
    />
  );
}
