import {
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  Sidebar as SidebarPrimitive,
} from "@/components/ui/sidebar";
import { useLicense } from "@/contexts/LicenseContext";
import { cn } from "@/lib/utils";
import { Clock, Cpu, Key, Layers, Settings2, Sparkles, VerifiedIcon } from "lucide-react";

interface SidebarProps {
  activeSection: string;
  onSectionChange: (section: string) => void;
}

const sections = [
  { id: "recordings", label: "History", icon: Clock },
  { id: "general", label: "General", icon: Settings2 },
  { id: "models", label: "Models", icon: Cpu },
  { id: "enhancements", label: "AI Enhancement", icon: Sparkles },
  { id: "account", label: "Account", icon: Key },
  { id: "advanced", label: "Advanced", icon: Layers },
];

export function Sidebar({ activeSection, onSectionChange }: SidebarProps) {
  const { status, isLoading } = useLicense();

  // Show license status for all states (not just trial)
  const showLicenseInfo = !isLoading && status;
  const daysLeft = status?.trial_days_left || -1;

  return (
    <SidebarPrimitive>
      <SidebarContent>
        <SidebarGroup>
          <SidebarMenu>
            {sections.map((section) => {
              const Icon = section.icon;
              return (
                <SidebarMenuItem key={section.id}>
                  <SidebarMenuButton
                    onClick={() => onSectionChange(section.id)}
                    isActive={activeSection === section.id}
                  >
                    <Icon className="h-4 w-4" />
                    <span>{section.label}</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              );
            })}
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter>
        {showLicenseInfo && (
          <div className="px-3 py-2 flex items-center justify-between text-xs">
            <span className={cn(
              "text-muted-foreground flex items-center gap-2",
              status.status === 'licensed' && 'text-primary font-bold',
            )}>
              {
                status.status === 'licensed' && <VerifiedIcon className="w-4 h-4 text-primary" />
              }
              {status.status === 'licensed' ? (
                'Licensed'
              ) : status.status === 'trial' ? (
                daysLeft > 0 ? `Trial: ${daysLeft} days left` : daysLeft === 0 ? 'Trial expires today' : 'Trial expired'
              ) : status.status === 'expired' ? (
                'Trial Expired'
              ) : (
                'No License'
              )}
            </span>
            {status.status !== 'licensed' && (
              <a
                href="https://voicetypr.com/#pricing"
                target="_blank"
                rel="noopener noreferrer"
                className="font-semibold text-foreground hover:underline"
              >
                Upgrade
              </a>
            )}
          </div>
        )}
      </SidebarFooter>
    </SidebarPrimitive>
  );
}