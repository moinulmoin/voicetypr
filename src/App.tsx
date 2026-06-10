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
                <Toaster
                  position="top-center"
                  closeButton
                  toastOptions={{
                    classNames: {
                      toast:
                        "border-border/60 bg-card/95 text-card-foreground shadow-xl shadow-black/10 backdrop-blur supports-[backdrop-filter]:bg-card/90",
                      title: "text-sm font-semibold",
                      description: "text-sm text-muted-foreground",
                      closeButton:
                        "border-border/60 bg-background text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                      success:
                        "border-green-500/25 bg-green-500/10 text-green-900 dark:text-green-100",
                      error:
                        "border-red-500/25 bg-red-500/10 text-red-900 dark:text-red-100",
                      warning:
                        "border-amber-500/30 bg-amber-50/80 text-amber-800 dark:border-amber-400/25 dark:bg-amber-950/20 dark:text-amber-200",
                      info:
                        "border-blue-500/25 bg-blue-500/10 text-blue-900 dark:text-blue-100",
                    },
                  }}
                />
              </TooltipProvider>
            </ModelManagementProvider>
          </ReadinessProvider>
        </SettingsProvider>
      </LicenseProvider>
    </AppErrorBoundary>
  );
}
