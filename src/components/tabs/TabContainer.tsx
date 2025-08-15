import { lazy, Suspense } from "react";
import { Skeleton } from "../ui/skeleton";

// Lazy load tab components for better performance
const RecordingsTab = lazy(() => import("./RecordingsTab").then(m => ({ default: m.RecordingsTab })));
const ModelsTab = lazy(() => import("./ModelsTab").then(m => ({ default: m.ModelsTab })));
const SettingsTab = lazy(() => import("./SettingsTab").then(m => ({ default: m.SettingsTab })));
const EnhancementsTab = lazy(() => import("./EnhancementsTab").then(m => ({ default: m.EnhancementsTab })));
const AdvancedTab = lazy(() => import("./AdvancedTab").then(m => ({ default: m.AdvancedTab })));
const AccountTab = lazy(() => import("./AccountTab").then(m => ({ default: m.AccountTab })));

interface TabContainerProps {
  activeSection: string;
}

// Loading skeleton for tab content
function TabSkeleton() {
  return (
    <div className="h-full flex flex-col p-6">
      <div className="flex-shrink-0 mb-4 space-y-3">
        <Skeleton className="h-6 w-48" />
        <Skeleton className="h-4 w-96" />
      </div>
      <div className="flex-1 space-y-3">
        <Skeleton className="h-20 w-full" />
        <Skeleton className="h-20 w-full" />
        <Skeleton className="h-20 w-full" />
      </div>
    </div>
  );
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
    <Suspense fallback={<TabSkeleton />}>
      <div className="h-full flex flex-col">
        {renderTabContent()}
      </div>
    </Suspense>
  );
}