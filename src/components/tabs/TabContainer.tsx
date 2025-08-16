// Direct imports for instant desktop app experience
import { RecordingsTab } from "./RecordingsTab";
import { ModelsTab } from "./ModelsTab";
import { SettingsTab } from "./SettingsTab";
import { EnhancementsTab } from "./EnhancementsTab";
import { AdvancedTab } from "./AdvancedTab";
import { AccountTab } from "./AccountTab";

interface TabContainerProps {
  activeSection: string;
}

export function TabContainer({ activeSection }: TabContainerProps) {
  const renderTabContent = () => {
    switch (activeSection) {
      case "recordings":
        return <RecordingsTab />;

      case "general":
        return <SettingsTab />;

      case "models":
        return <ModelsTab />;

      case "advanced":
        return <AdvancedTab />;

      case "enhancements":
        return <EnhancementsTab />;

      case "account":
      case "about":
      case "license":
        return <AccountTab />;

      default:
        return <RecordingsTab />;
    }
  };

  return (
    <div className="h-full flex flex-col">
      {renderTabContent()}
    </div>
  );
}