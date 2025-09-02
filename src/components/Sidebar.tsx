import {
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  Sidebar as SidebarPrimitive,
} from "@/components/ui/sidebar";
import { Button } from "@/components/ui/button";
import { useLicense } from "@/contexts/LicenseContext";
import { cn } from "@/lib/utils";
import {
  Clock,
  Cpu,
  HelpCircle,
  Home,
  Key,
  Layers,
  Settings2,
  Sparkles,
  VerifiedIcon,
} from "lucide-react";

interface SidebarProps {
  activeSection: string;
  onSectionChange: (section: string) => void;
}

const mainSections = [
  { id: "overview", label: "Overview", icon: Home },
  { id: "recordings", label: "History", icon: Clock },
  { id: "general", label: "Settings", icon: Settings2 },
  { id: "models", label: "Models", icon: Cpu },
  { id: "enhancements", label: "Enhancements", icon: Sparkles },
  { id: "account", label: "Account", icon: Key },
];

const bottomSections = [{ id: "advanced", label: "Advanced", icon: Layers }];

export function Sidebar({ activeSection, onSectionChange }: SidebarProps) {
  const { status, isLoading } = useLicense();

  // Show license status for all states (not just trial)
  const showLicenseInfo = !isLoading && status;
  const daysLeft = status?.trial_days_left || -1;
  
  return (
    <SidebarPrimitive className="border-r-0">
      <SidebarContent className="px-2 pt-4">
        <SidebarGroup className="flex-1">
          <SidebarMenu>
            {mainSections.map((section) => {
              const Icon = section.icon;
              const isActive = activeSection === section.id;
              return (
                <SidebarMenuItem key={section.id}>
                  <SidebarMenuButton
                    onClick={() => onSectionChange(section.id)}
                    isActive={isActive}
                    className={cn(
                      "group relative rounded-lg px-3 py-2 hover:bg-accent/50 transition-colors",
                      isActive &&
                        "bg-accent text-accent-foreground font-medium",
                    )}
                  >
                    <Icon
                      className={cn(
                        "h-4 w-4 transition-transform group-hover:scale-110",
                        isActive && "text-primary",
                      )}
                    />
                    <span className="ml-3">{section.label}</span>
                    {isActive && (
                      <div className="absolute left-0 top-1/2 -translate-y-1/2 w-1 h-6 bg-primary rounded-r-full" />
                    )}
                  </SidebarMenuButton>
                </SidebarMenuItem>
              );
            })}
          </SidebarMenu>
        </SidebarGroup>

        <SidebarGroup>
          <SidebarMenu>
            {bottomSections.map((section) => {
              const Icon = section.icon;
              const isActive = activeSection === section.id;
              return (
                <SidebarMenuItem key={section.id}>
                  <SidebarMenuButton
                    onClick={() => onSectionChange(section.id)}
                    isActive={isActive}
                    className={cn(
                      "group relative rounded-lg px-3 py-2 hover:bg-accent/50 transition-colors",
                      isActive &&
                        "bg-accent text-accent-foreground font-medium",
                    )}
                  >
                    <Icon
                      className={cn(
                        "h-4 w-4 transition-transform group-hover:scale-110",
                        isActive && "text-primary",
                      )}
                    />
                    <span className="ml-3">{section.label}</span>
                    {isActive && (
                      <div className="absolute left-0 top-1/2 -translate-y-1/2 w-1 h-6 bg-primary rounded-r-full" />
                    )}
                  </SidebarMenuButton>
                </SidebarMenuItem>
              );
            })}

            <SidebarMenuItem>
              <SidebarMenuButton
                onClick={() =>
                  window.open("https://voicetypr.com/docs", "_blank")
                }
                className="group relative rounded-lg px-3 py-2 hover:bg-accent/50 transition-colors"
              >
                <HelpCircle className="h-4 w-4 transition-transform group-hover:scale-110" />
                <span className="ml-3">Help</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter className="border-t border-border/40 p-3">
        {showLicenseInfo && (
          <div className="space-y-2">
            {status.status === "licensed" ? (
              <div className="flex items-center justify-center gap-2 px-3 py-2 rounded-md bg-green-500/10">
                <VerifiedIcon className="w-4 h-4 text-green-600 dark:text-green-400" />
                <span className="text-xs font-medium text-green-600 dark:text-green-400">
                  Pro Licensed
                </span>
              </div>
            ) : (
              <>
                <div className={cn(
                  "px-2 py-1.5 rounded-md text-center",
                  status.status === "trial" && daysLeft > 0 && "bg-accent/50"
                )}>
                  <span className={cn(
                    "text-xs font-medium",
                    (status.status === "expired" || status.status === "none" || (status.status === "trial" && daysLeft <= 0)) && "text-amber-600"
                  )}>
                    {status.status === "trial"
                      ? daysLeft > 0
                        ? `${daysLeft} days left in trial`
                        : daysLeft === 0
                          ? "Trial expires today"
                          : "Trial expired"
                      : status.status === "expired" || status.status === "none"
                        ? "Trial Expired"
                        : "No License"}
                  </span>
                </div>
                <Button
                  asChild
                  className="w-full text-sm"
                  size="sm"
                >
                  <a
                    href="https://voicetypr.com/#pricing"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    Upgrade to Pro
                  </a>
                </Button>
              </>
            )}
          </div>
        )}
      </SidebarFooter>
    </SidebarPrimitive>
  );
}
