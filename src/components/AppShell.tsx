import { useEffect, useState, type CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";
import { CircleAlert } from "lucide-react";
import { toast } from "sonner";
import type { ScreenId } from "@/components/navigation";
import { Sidebar } from "@/components/Sidebar";
import { TabContainer } from "@/components/tabs/TabContainer";
import {
  Alert,
  AlertAction,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { Spinner } from "@/components/ui/spinner";
import { createLogger } from "@/lib/logger";
import { isMacOS } from "@/lib/platform";
import { getTrayStatus, retryTrayCreation, type TrayStatus } from "@/lib/tray";

const log = createLogger("app-shell");

interface AppShellProps {
  activeSection: ScreenId;
  onSectionChange: (section: ScreenId) => void;
}

export function AppShell({ activeSection, onSectionChange }: AppShellProps) {
  const [trayStatus, setTrayStatus] = useState<TrayStatus | null>(null);
  const [isRetryingTray, setIsRetryingTray] = useState(false);
  useEffect(() => {
    let isMounted = true;
    let unlisten: (() => void) | undefined;

    void getTrayStatus()
      .then((status) => {
        if (isMounted) setTrayStatus(status);
      })
      .catch((error) => {
        log.warn("Failed to read tray status:", error);
      });

    void listen<TrayStatus>("tray-status-changed", (event) => {
      if (isMounted) setTrayStatus(event.payload);
    }).then((nextUnlisten) => {
      if (!isMounted) {
        nextUnlisten();
        return;
      }
      unlisten = nextUnlisten;
    });

    return () => {
      isMounted = false;
      unlisten?.();
    };
  }, []);

  const handleRetryTray = async () => {
    setIsRetryingTray(true);
    try {
      const status = await retryTrayCreation();
      setTrayStatus(status);
      if (status.available) {
        toast.success("Menu-bar icon restored");
      } else {
        toast.error(
          "Menu-bar icon is still unavailable. Keep this window open and report the issue.",
        );
      }
    } catch (error) {
      log.error("Failed to retry tray creation:", error);
      toast.error(
        "Could not retry the menu-bar icon. Keep this window open and report the issue.",
      );
    } finally {
      setIsRetryingTray(false);
    }
  };

  const trayUnavailable =
    trayStatus !== null && !trayStatus.available && trayStatus.attempts > 0;

  return (
    <SidebarProvider
      className="bg-background"
      style={
        {
          "--sidebar-width": "14rem",
          "--sidebar": "var(--background)",
        } as CSSProperties
      }
    >
      <header
        data-tauri-drag-region
        className={`fixed inset-x-0 top-0 z-50 flex h-9 items-center bg-sidebar pr-3 ${
          isMacOS ? "pl-[4.75rem]" : "pl-3"
        }`}
      >
        <SidebarTrigger
          className="size-7 -translate-y-2 text-muted-foreground"
          title="Toggle sidebar"
        />
      </header>
      <Sidebar
        activeSection={activeSection}
        onSectionChange={onSectionChange}
      />
      <SidebarInset className="mb-2 mr-2 mt-9 h-[calc(100svh-2.75rem)] min-h-0 rounded-2xl border border-l-0 border-border/70 bg-background">
        {trayUnavailable ? (
          <Alert variant="destructive" className="mx-4 mt-4">
            <CircleAlert />
            <AlertTitle>Menu-bar icon unavailable</AlertTitle>
            <AlertDescription>
              Voicetypr could not create its menu-bar icon after{" "}
              {trayStatus.attempts} attempts. Keep this window open, then retry
              or submit a bug report with the included diagnostic.
            </AlertDescription>
            <AlertAction>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={isRetryingTray}
                onClick={handleRetryTray}
              >
                {isRetryingTray ? <Spinner data-icon="inline-start" /> : null}
                Retry icon
              </Button>
            </AlertAction>
          </Alert>
        ) : null}
        <div className="min-h-0 flex-1">
          <TabContainer activeSection={activeSection} />
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
