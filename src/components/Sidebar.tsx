import { Clock, Cpu, Info, Settings2 } from "lucide-react";
import {
  Sidebar as SidebarPrimitive,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";

interface SidebarProps {
  activeSection: string;
  onSectionChange: (section: string) => void;
}

const sections = [
  { id: "recordings", label: "Recent Recordings", icon: Clock },
  { id: "general", label: "General", icon: Settings2 },
  { id: "models", label: "Models", icon: Cpu },
  { id: "about", label: "About", icon: Info },
];

export function Sidebar({ activeSection, onSectionChange }: SidebarProps) {
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
        <p className="text-xs text-muted-foreground px-2">v0.1.0</p>
      </SidebarFooter>
    </SidebarPrimitive>
  );
}