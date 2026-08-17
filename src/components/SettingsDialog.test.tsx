import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SettingsDialog } from "./SettingsDialog";
import { useState } from "react";

vi.mock("@/components/sections/GeneralSettings", () => ({
  GeneralSettings: () => <div>General settings content</div>,
}));

vi.mock("@/components/sections/AccountSection", () => ({
  AccountSection: () => <div>License settings content</div>,
}));

vi.mock("@/components/sections/AdvancedSection", () => ({
  AdvancedSection: () => <div>Diagnostics settings content</div>,
}));

function SettingsDialogHarness() {
  const [section, setSection] = useState<"general" | "license" | "diagnostics">(
    "general",
  );

  return (
    <SettingsDialog
      open
      section={section}
      onSectionChange={setSection}
      onOpenChange={vi.fn()}
    />
  );
}

describe("SettingsDialog", () => {
  it("opens on General and switches between utility sections", () => {
    render(<SettingsDialogHarness />);

    expect(screen.getByRole("dialog", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByText("General settings content")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "License" }));
    expect(screen.getByText("License settings content")).toBeInTheDocument();
    expect(screen.queryByText("General settings content")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Diagnostics" }));
    expect(screen.getByText("Diagnostics settings content")).toBeInTheDocument();
  });
});
