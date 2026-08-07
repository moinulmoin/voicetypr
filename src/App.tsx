import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AppErrorBoundary } from "./components/ErrorBoundary";
import { AppContainer } from "./components/AppContainer";
import { LicenseProvider } from "./contexts/LicenseContext";
import { ReadinessProvider } from "./contexts/ReadinessContext";
import { SettingsProvider } from "./contexts/SettingsContext";
import { ModelManagementProvider } from "./contexts/ModelManagementContext";
import { ModelAvailabilityProvider } from "./contexts/ModelAvailabilityContext";

export default function App() {
  return (
    <AppErrorBoundary>
      <LicenseProvider>
        <SettingsProvider>
          <ModelAvailabilityProvider>
            <ReadinessProvider>
              <ModelManagementProvider>
              <TooltipProvider>
                <AppContainer />
                <Toaster
                  position="bottom-right"
                  closeButton
                  expand
                  visibleToasts={4}
                  toastOptions={{
                    duration: 5_000,
                    classNames: {
                      toast:
                        "border-border bg-popover text-popover-foreground shadow-2xl shadow-black/20 ring-1 ring-black/5",
                      title: "text-sm font-semibold",
                      description: "text-sm leading-relaxed text-muted-foreground",
                      closeButton:
                        "border-border bg-background text-muted-foreground shadow-sm hover:bg-accent hover:text-accent-foreground",
                      success: "border-l-4 border-l-sage",
                      error: "border-l-4 border-l-destructive",
                      warning: "border-l-4 border-l-amber-500",
                      info: "border-l-4 border-l-sky-500",
                    },
                  }}
                />
              </TooltipProvider>
            </ModelManagementProvider>
          </ReadinessProvider>
          </ModelAvailabilityProvider>
        </SettingsProvider>
      </LicenseProvider>
    </AppErrorBoundary>
  );
}
