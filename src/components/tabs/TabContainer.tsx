// Direct imports for instant desktop app experience
import { AccountTab } from "./AccountTab";
import { AdvancedTab } from "./AdvancedTab";
import { EnhancementsTab } from "./EnhancementsTab";
import { HelpTab } from "./HelpTab";
import { ModelsTab } from "./ModelsTab";
import { OverviewTab } from "./OverviewTab";
import { RecordingsTab } from "./RecordingsTab";
import { SettingsTab } from "./SettingsTab";
import { ShortcutsTab } from "./ShortcutsTab";
import { NetworkSharingTab } from "./NetworkSharingTab";
import { AgentCliTab } from "./AgentCliTab";
import { AudioUploadSection } from "../sections/AudioUploadSection";
import { ReportProblemSection } from "../sections/ReportProblemSection";
import type { ScreenId } from "@/components/navigation";

interface TabContainerProps {
  activeSection: ScreenId;
}

export function TabContainer({ activeSection }: TabContainerProps) {

  const renderTabContent = () => {
    switch (activeSection) {
      case "overview":
        return <OverviewTab />;

      case "recordings":
        return <RecordingsTab />;

      case "audio":
        return <AudioUploadSection />;

      case "general":
        return <SettingsTab />;

      case "shortcuts":
        return <ShortcutsTab />;

      case "models":
        return <ModelsTab />;

      case "network":
        return <NetworkSharingTab />;

      case "agent":
        return <AgentCliTab />;

      case "advanced":
        return <AdvancedTab />;

      case "formatting":
        return <EnhancementsTab />;

      case "license":
        return <AccountTab />;

      case "help":
        return <HelpTab />;

      case "report-problem":
        return <ReportProblemSection />;


      default:
        return <OverviewTab />;
    }
  };

  return <div className="h-full min-h-0 flex flex-col">{renderTabContent()}</div>;
}
