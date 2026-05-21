import { Toaster } from "sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AppErrorBoundary } from "./components/ErrorBoundary";
import { AppContainer } from "./components/AppContainer";
import { LicenseProvider } from "./contexts/LicenseContext";
import { ReadinessProvider } from "./contexts/ReadinessContext";
import { SettingsProvider } from "./contexts/SettingsContext";
import { ModelManagementProvider } from "./contexts/ModelManagementContext";

export default function App() {
  return (
    <AppErrorBoundary>
      <LicenseProvider>
        <SettingsProvider>
          <ReadinessProvider>
            <ModelManagementProvider>
              <TooltipProvider>
                <AppContainer />
                <Toaster position="top-center" />
              </TooltipProvider>
            </ModelManagementProvider>
          </ReadinessProvider>
        </SettingsProvider>
      </LicenseProvider>
    </AppErrorBoundary>
  );
}
