import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TelemetrySection } from "../TelemetrySection";

const mockInvoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock("sonner", () => ({
  toast: {
    info: vi.fn(),
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
  },
}));

describe("TelemetrySection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_telemetry_status") {
        return Promise.resolve({ enabled: true, available: true });
      }
      if (command === "get_product_analytics_status") {
        return Promise.resolve({
          enabled: false,
          available: true,
          consent_required: false,
        });
      }
      return Promise.resolve(undefined);
    });
  });

  it("renders independent crash and usage analytics controls", async () => {
    render(<TelemetrySection />);

    const diagnostics = await screen.findByRole("switch", {
      name: "Enable crash and error reporting",
    });
    const analytics = screen.getByRole("switch", {
      name: "Enable usage analytics",
    });

    expect(diagnostics).toBeChecked();
    expect(analytics).not.toBeChecked();
    expect(screen.getByText("Crash & error reporting")).toBeInTheDocument();
    expect(screen.getByText("Usage analytics")).toBeInTheDocument();
  });

  it("updates product analytics without changing crash consent", async () => {
    render(<TelemetrySection />);

    const analytics = await screen.findByRole("switch", {
      name: "Enable usage analytics",
    });
    await act(async () => {
      fireEvent.click(analytics);
    });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_product_analytics_consent", {
        enabled: true,
      });
    });
    expect(mockInvoke).not.toHaveBeenCalledWith("set_telemetry_consent", expect.anything());
  });

  it("allows opting out of an unavailable category", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_telemetry_status") {
        return Promise.resolve({ enabled: true, available: false });
      }
      if (command === "get_product_analytics_status") {
        return Promise.resolve({
          enabled: true,
          available: false,
          consent_required: false,
        });
      }
      return Promise.resolve(undefined);
    });

    render(<TelemetrySection />);

    const diagnostics = await screen.findByRole("switch", {
      name: "Enable crash and error reporting",
    });
    expect(diagnostics).toBeEnabled();

    await act(async () => {
      fireEvent.click(diagnostics);
    });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_telemetry_consent", {
        enabled: false,
      });
    });
  });
});
