import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Sidebar } from "./Sidebar";
import { SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar";
import { TooltipProvider } from "@/components/ui/tooltip";

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("2.0.5"),
}));

vi.mock("@/contexts/LicenseContext", () => ({
  useLicense: () => ({
    status: { status: "licensed", license_type: "pro", trial_days_left: null },
    isLoading: false,
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
        <Sidebar
          activeSection={activeSection}
          onSectionChange={onSectionChange}
        />
      </SidebarProvider>
    </TooltipProvider>,
  );
  return onSectionChange;
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("Sidebar navigation", () => {
  it("renders one flat ordered list of destinations", async () => {
    renderSidebar();
    await screen.findByText("v2.0.5");
    expect(
      document.querySelector('[data-slot="sidebar-container"]'),
    ).toHaveClass("group-data-[side=left]:border-r-0");

    const sources = screen.getByRole("button", { name: "Sources" });
    const polish = screen.getByRole("button", { name: "Polish" });
    expect(sources.compareDocumentPosition(polish) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByText("Workspace")).not.toBeInTheDocument();
    expect(screen.queryByText("Set up")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /network sharing/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /agent & cli/i })).toBeInTheDocument();
  });

  it("always shows the static footer block with account, diagnostics, and reporting", async () => {
    renderSidebar();
    await screen.findByText("v2.0.5");

    const footerGroup = screen.getByTestId("sidebar-footer-nav");

    expect(footerGroup).toHaveTextContent(/account/i);
    expect(footerGroup).toHaveTextContent(/diagnostics/i);
    expect(footerGroup).toHaveTextContent(/report a problem/i);
    expect(footerGroup).not.toHaveTextContent(/overview/i);
  });

  it("collapses to the icon rail through the sidebar control contract", async () => {
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
