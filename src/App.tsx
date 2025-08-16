import { Toaster } from "sonner";
import { AppErrorBoundary } from "./components/ErrorBoundary";
import { AppContainer } from "./components/AppContainer";
import { LicenseProvider } from "./contexts/LicenseContext";
import { ReadinessProvider } from "./contexts/ReadinessContext";
import { SettingsProvider } from "./contexts/SettingsContext";

export default function App() {
  return (
    <AppErrorBoundary>
      <LicenseProvider>
        <SettingsProvider>
          <ReadinessProvider>
            <AppContainer />
            <Toaster position="top-center" />
          </ReadinessProvider>
        </SettingsProvider>
      </LicenseProvider>
    </AppErrorBoundary>
  );
}
