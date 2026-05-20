import type { ScreenId } from "@/components/navigation";
import { Sidebar } from "@/components/Sidebar";
import { TabContainer } from "@/components/tabs/TabContainer";
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar";

interface AppShellProps {
  activeSection: ScreenId;
  onSectionChange: (section: ScreenId) => void;
}

export function AppShell({ activeSection, onSectionChange }: AppShellProps) {
  return (
    <SidebarProvider>
      <Sidebar activeSection={activeSection} onSectionChange={onSectionChange} />
      <SidebarInset>
        <TabContainer activeSection={activeSection} />
      </SidebarInset>
    </SidebarProvider>
  );
}
