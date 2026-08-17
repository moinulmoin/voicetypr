import { AccountSection } from "@/components/sections/AccountSection";
import { AdvancedSection } from "@/components/sections/AdvancedSection";
import { GeneralSettings } from "@/components/sections/GeneralSettings";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { Activity, Key, SlidersHorizontal, type LucideIcon } from "lucide-react";

export type SettingsSection = "general" | "license" | "diagnostics";

const SETTINGS_SECTIONS: Array<{
  id: SettingsSection;
  label: string;
  icon: LucideIcon;
}> = [
  { id: "general", label: "General", icon: SlidersHorizontal },
  { id: "license", label: "License", icon: Key },
  { id: "diagnostics", label: "Diagnostics", icon: Activity },
];

interface SettingsDialogProps {
  open: boolean;
  section: SettingsSection;
  onSectionChange: (section: SettingsSection) => void;
  onOpenChange: (open: boolean) => void;
}

export function SettingsDialog({
  open,
  section,
  onSectionChange,
  onOpenChange,
}: SettingsDialogProps) {
  const content =
    section === "license" ? (
      <AccountSection />
    ) : section === "diagnostics" ? (
      <AdvancedSection />
    ) : (
      <GeneralSettings />
    );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="overflow-hidden rounded-2xl p-0 sm:max-w-none"
        style={{
          width: "calc(100% - 6rem)",
          maxWidth: "56rem",
          height: "min(40rem, calc(100svh - 7rem))",
        }}
      >
        <DialogHeader className="sr-only">
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>
            Manage general preferences, license, and diagnostics.
          </DialogDescription>
        </DialogHeader>

        <div className="grid h-full min-h-0 grid-cols-[11.5rem_minmax(0,1fr)]">
          <aside className="flex min-h-0 flex-col border-r border-border/70 bg-muted/35 p-4">
            <div className="px-2 pb-4 pt-1">
              <p className="text-lg font-semibold tracking-tight">Settings</p>
              <p className="mt-0.5 text-xs text-muted-foreground">
                App preferences and support
              </p>
            </div>

            <nav aria-label="Settings sections" className="space-y-1">
              {SETTINGS_SECTIONS.map((settingsSection) => {
                const Icon = settingsSection.icon;
                const isActive = section === settingsSection.id;

                return (
                  <button
                    key={settingsSection.id}
                    type="button"
                    onClick={() => onSectionChange(settingsSection.id)}
                    aria-current={isActive ? "page" : undefined}
                    className={cn(
                      "flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm font-medium transition-colors",
                      isActive
                        ? "bg-background text-foreground shadow-sm ring-1 ring-border/70"
                        : "text-muted-foreground hover:bg-background/70 hover:text-foreground",
                    )}
                  >
                    <Icon className={cn("size-4", isActive && "text-sage")} />
                    <span>{settingsSection.label}</span>
                  </button>
                );
              })}
            </nav>
          </aside>

          <section className="min-h-0 min-w-0 bg-background">{content}</section>
        </div>
      </DialogContent>
    </Dialog>
  );
}
