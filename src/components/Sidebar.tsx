import { ReportBugDialog } from "@/components/ReportBugDialog";
import { Brandmark } from "@/components/Brandmark";
import {
  navGroups,
  footerScreens,
  sidebarActions,
  type ScreenDefinition,
  type ScreenId,
  type SidebarActionDefinition,
} from "@/components/navigation";
import { getVersion } from "@tauri-apps/api/app";
import { Button } from "@/components/ui/button";
import {
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  Sidebar as SidebarPrimitive,
} from "@/components/ui/sidebar";
import { useLicense } from "@/contexts/LicenseContext";
import type { LicenseStatus } from "@/types";
import { cn } from "@/lib/utils";
import { RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { updateService } from "@/services/updateService";

interface SidebarProps {
  activeSection: ScreenId;
  onSectionChange: (section: ScreenId) => void;
}


function getLicenseBadge(status: LicenseStatus | null, daysLeft: number) {
  if (!status || status.status === "none") {
    return {
      label: "No License",
      className: "border-border bg-muted text-muted-foreground",
    };
  }

  if (status.status === "licensed") {
    return {
      label: "Pro",
      className: "border-emerald-500/30 bg-emerald-500/10 text-emerald-700",
    };
  }

  if (status.status === "trial") {
    if (daysLeft > 1) {
      return {
        label: `Trial · ${daysLeft} days left`,
        className: "border-green-500/25 bg-green-500/10 text-green-700 dark:text-green-400",
      };
    }

    if (daysLeft === 1) {
      return {
        label: "Trial · 1 day left",
        className: "border-amber-500/25 bg-amber-500/10 text-amber-700 dark:text-amber-400",
      };
    }

    if (daysLeft === 0) {
      return {
        label: "Trial expires today",
        className: "border-amber-500/25 bg-amber-500/10 text-amber-700 dark:text-amber-400",
      };
    }

    return {
      label: "Trial",
      className: "border-green-500/25 bg-green-500/10 text-green-700 dark:text-green-400",
    };
  }

  return {
    label: "Trial expired",
    className: "border-red-500/25 bg-red-500/10 text-red-700 dark:text-red-400",
  };
}
export function Sidebar({ activeSection, onSectionChange }: SidebarProps) {
  const { status, isLoading } = useLicense();
  const [appVersion, setAppVersion] = useState("—");
  const [showReportBugDialog, setShowReportBugDialog] = useState(false);
  const licenseBadge = getLicenseBadge(status, status?.trial_days_left ?? -1);

  useEffect(() => {
    const loadVersion = async () => {
      try {
        setAppVersion(await getVersion());
      } catch {
        setAppVersion("—");
      }
    };
    void loadVersion();
  }, []);

  return (
    <>
      <SidebarPrimitive collapsible="none" className="border-sidebar-border/80 bg-sidebar/95 backdrop-blur-sm">
        <SidebarHeader className="px-4 pb-2 pt-4">
          <button
            type="button"
            onClick={() => onSectionChange("overview")}
            className="flex w-full items-center justify-between gap-3 rounded-lg border border-sidebar-border/60 bg-background/60 px-3 py-2 text-left transition-colors hover:bg-sidebar-accent"
          >
            <div className="flex min-w-0 items-center gap-2.5">
              <Brandmark className="size-6 shrink-0 text-sage" />
              <span className="truncate text-sm font-semibold tracking-tight">Voicetypr</span>
            </div>
            {!isLoading && status ? (
              <span
                className={cn(
                  "shrink-0 rounded-full border px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.12em]",
                  licenseBadge.className,
                )}
              >
                {licenseBadge.label}
              </span>
            ) : null}
          </button>
        </SidebarHeader>

        <SidebarContent className="flex flex-col px-2">
          {navGroups.map((group) => (
            <SidebarNavGroup
              key={group.label}
              label={group.label}
              items={group.screens}
              activeSection={activeSection}
              onSectionChange={onSectionChange}
            />
          ))}
          <div className="mt-auto space-y-0 pb-2">
            <SidebarNavGroup
              label={null}
              items={footerScreens}
              activeSection={activeSection}
              onSectionChange={onSectionChange}
            />
            <SidebarActionGroup
              actions={sidebarActions}
              onReportBug={() => setShowReportBugDialog(true)}
            />
          </div>
        </SidebarContent>

        <SidebarFooter className="border-t border-sidebar-border/70 px-3 py-2">
          <SidebarFooterStatus appVersion={appVersion} />
        </SidebarFooter>
      </SidebarPrimitive>

      <ReportBugDialog
        isOpen={showReportBugDialog}
        onClose={() => setShowReportBugDialog(false)}
      />
    </>
  );
}

function SidebarNavGroup({
  label,
  items,
  activeSection,
  onSectionChange,
}: {
  label: string | null;
  items: ScreenDefinition[];
  activeSection: ScreenId;
  onSectionChange: (section: ScreenId) => void;
}) {
  return (
    <SidebarGroup>
      {label ? (
        <SidebarGroupLabel className="px-3 text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
          {label}
        </SidebarGroupLabel>
      ) : null}
      <SidebarGroupContent>
        <SidebarMenu>
          {items.map((item) => (
            <SidebarNavItem
              key={item.id}
              item={item}
              isActive={activeSection === item.id}
              onSelect={onSectionChange}
            />
          ))}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  );
}

function SidebarNavItem({
  item,
  isActive,
  onSelect,
}: {
  item: ScreenDefinition;
  isActive: boolean;
  onSelect: (section: ScreenId) => void;
}) {
  const Icon = item.icon;

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        tooltip={item.description}
        isActive={isActive}
        onClick={() => onSelect(item.id)}
        className={cn(
          "rounded-xl text-sm font-medium transition-colors [&>svg]:text-muted-foreground",
          isActive
            ? "!bg-card font-semibold text-foreground shadow-sm ring-1 ring-border hover:!bg-card [&>svg]:!text-sage"
            : "text-muted-foreground hover:bg-sidebar-accent/70 hover:text-foreground",
        )}
      >
        <Icon />
        <span>{item.label}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}

function SidebarActionGroup({
  actions,
  onReportBug,
}: {
  actions: SidebarActionDefinition[];
  onReportBug: () => void;
}) {
  return (
    <SidebarGroup className="px-2 pb-2 pt-0">
      <SidebarGroupContent>
        <SidebarMenu>
          {actions.map((action) => {
            const Icon = action.icon;
            return (
              <SidebarMenuItem key={action.id}>
                <SidebarMenuButton
                  tooltip={action.description}
                  onClick={onReportBug}
                  className="rounded-xl text-sm transition-colors"
                >
                  <Icon />
                  <span>{action.label}</span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            );
          })}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  );
}

function SidebarFooterStatus({
  appVersion,
}: {
  appVersion: string;
}) {
  const [isCheckingUpdates, setIsCheckingUpdates] = useState(false);

  const checkUpdates = async () => {
    setIsCheckingUpdates(true);
    try {
      await updateService.checkForUpdatesManually();
    } finally {
      setIsCheckingUpdates(false);
    }
  };

  return (
    <div className="flex items-center justify-between gap-2">
      <span className="text-xs text-muted-foreground">v{appVersion}</span>
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        className="size-7 rounded-md text-muted-foreground"
        onClick={checkUpdates}
        disabled={isCheckingUpdates}
        title="Check for updates"
      >
        <RefreshCw className={cn("size-3.5", isCheckingUpdates && "animate-spin")} />
        <span className="sr-only">Check for updates</span>
      </Button>
    </div>
  );
}
