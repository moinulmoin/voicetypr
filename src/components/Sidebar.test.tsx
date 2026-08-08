import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Sidebar } from "./Sidebar";
import { SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar";
import { TooltipProvider } from "@/components/ui/tooltip";
const settingsState = {
  settings_mode: "recommended" as "recommended" | "advanced",
};


vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("2.0.5"),
}));

vi.mock("@/contexts/LicenseContext", () => ({
  useLicense: () => ({
    status: { status: "licensed", license_type: "pro", trial_days_left: null },
    isLoading: false,
  }),
}));

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: settingsState,
  }),
}));


vi.mock("@/services/updateService", () => ({
  updateService: { checkForUpdatesManually: vi.fn() },
}));

function renderSidebar(activeSection: Parameters<typeof Sidebar>[0]["activeSection"] = "overview") {
  const onSectionChange = vi.fn();
  render(
    <TooltipProvider>
      <SidebarProvider>
        <SidebarTrigger />
        <Sidebar activeSection={activeSection} onSectionChange={onSectionChange} />
      </SidebarProvider>
    </TooltipProvider>,
  );
  return onSectionChange;
}

beforeEach(() => {
  vi.clearAllMocks();
  settingsState.settings_mode = "recommended";
});

describe("Sidebar navigation", () => {
  it("keeps one compact navigation list visible in Default mode", () => {
    renderSidebar();

    expect(screen.getByRole("button", { name: "Models" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Account" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /report a problem/i })).toBeInTheDocument();
    expect(screen.queryByText("Workspace")).not.toBeInTheDocument();
    expect(screen.queryByText("Configure")).not.toBeInTheDocument();
    expect(screen.queryByText("Support")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /network sharing/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /agent & cli/i })).not.toBeInTheDocument();
    const accountGroup = screen
      .getByRole("button", { name: "Account" })
      .closest('[data-slot="sidebar-group"]');
    const reportGroup = screen
      .getByRole("button", { name: /report a problem/i })
      .closest('[data-slot="sidebar-group"]');
    expect(accountGroup).not.toBe(reportGroup);
  });

  it("shows power-user destinations in advanced mode", () => {
    settingsState.settings_mode = "advanced";
    renderSidebar();

    expect(screen.getByRole("button", { name: /network sharing/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /agent & cli/i })).toBeInTheDocument();
    const advancedGroup = screen
      .getByRole("button", { name: "Advanced" })
      .closest('[data-slot="sidebar-group"]');
    const reportGroup = screen
      .getByRole("button", { name: /report a problem/i })
      .closest('[data-slot="sidebar-group"]');
    expect(advancedGroup).toBe(reportGroup);
  });

  it("collapses to the icon rail from the title bar trigger", async () => {
    const user = userEvent.setup();
    renderSidebar();
    const sidebar = document.querySelector('[data-slot="sidebar"][data-state]');

    expect(sidebar).toHaveAttribute("data-state", "expanded");
    await user.click(screen.getByRole("button", { name: "Toggle Sidebar" }));
    expect(sidebar).toHaveAttribute("data-state", "collapsed");
    expect(sidebar).toHaveAttribute("data-collapsible", "icon");
    expect(screen.getByTitle("Overview")).toHaveAccessibleName("Overview");
  });
});
