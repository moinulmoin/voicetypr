import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { ReactNode } from "react";

// Mock Tauri invoke
const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// Mock Tauri events
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

// Mock sonner toast
vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

import { NetworkSharingCard } from "../NetworkSharingCard";
import { SettingsProvider } from "@/contexts/SettingsContext";

// Wrapper component that provides SettingsContext
function TestWrapper({ children }: { children: ReactNode }) {
  return <SettingsProvider>{children}</SettingsProvider>;
}

// Helper to render with providers
function renderWithProviders(ui: React.ReactElement) {
  return render(ui, { wrapper: TestWrapper });
}

describe("NetworkSharingCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("when no model is downloaded", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: null,
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: false,
              port: null,
              model_name: null,
              server_name: null,
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [
                { name: "large-v3-turbo", display_name: "Large v3 Turbo", downloaded: false },
                { name: "base.en", display_name: "Base (English)", downloaded: false },
              ],
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows warning when no model is downloaded", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("No model downloaded")).toBeInTheDocument();
      });

      expect(
        screen.getByText(/Download a transcription model in the Models tab/)
      ).toBeInTheDocument();
    });

    it("disables the toggle switch when no model is downloaded", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        const toggle = screen.getByRole("switch");
        expect(toggle).toBeDisabled();
      });
    });
  });

  describe("when a model is downloaded", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: false,
              port: null,
              model_name: null,
              server_name: null,
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)", "10.0.0.5 (WiFi)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [
                { name: "large-v3-turbo", display_name: "Large v3 Turbo", downloaded: true },
                { name: "base.en", display_name: "Base (English)", downloaded: false },
              ],
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows which model will be shared", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Large v3 Turbo")).toBeInTheDocument();
      });

      expect(
        screen.getByText(/other VoiceTypr instances on your network can use your/)
      ).toBeInTheDocument();
    });

    it("enables the toggle switch when a model is available", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        const toggle = screen.getByRole("switch");
        expect(toggle).not.toBeDisabled();
      });
    });

    it("does not show the no model warning", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.queryByText("No model downloaded")).not.toBeInTheDocument();
      });
    });
  });

  describe("when sharing is enabled", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [
                { name: "large-v3-turbo", display_name: "Large v3 Turbo", downloaded: true },
              ],
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows Sharing Active status", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Sharing Active")).toBeInTheDocument();
      });
    });

    it("shows the model being shared with friendly name", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Model: Large v3 Turbo")).toBeInTheDocument();
      });
    });

    it("displays IP addresses with port", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText(/192\.168\.1\.100:47842/)).toBeInTheDocument();
      });
    });
  });

  describe("UI messaging", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: false,
              port: null,
              model_name: null,
              server_name: null,
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [
                { name: "large-v3-turbo", display_name: "Large v3 Turbo", downloaded: true },
              ],
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows clear description about sharing the model", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(
          screen.getByText("Share your transcription model with other devices")
        ).toBeInTheDocument();
      });
    });
  });

  describe("when model selection changes while sharing", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            // User has selected a different model than what's being shared
            return Promise.resolve({
              current_model: "base.en",
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            // Server is sharing large-v3-turbo
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [
                { name: "large-v3-turbo", display_name: "Large v3 Turbo", downloaded: true },
                { name: "base.en", display_name: "Base (English)", downloaded: true },
              ],
            });
          case "stop_sharing":
            return Promise.resolve();
          case "start_sharing":
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("automatically restarts sharing when model changes", async () => {
      renderWithProviders(<NetworkSharingCard />);

      // Wait for the auto-restart to be triggered
      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("stop_sharing");
        expect(mockInvoke).toHaveBeenCalledWith("start_sharing", expect.any(Object));
      });
    });

    it("does not show manual Update button", async () => {
      renderWithProviders(<NetworkSharingCard />);

      // Wait for component to render
      await waitFor(() => {
        expect(screen.getByText("Sharing Active")).toBeInTheDocument();
      });

      // Should NOT show Update button - restart is automatic
      expect(screen.queryByRole("button", { name: /Update/i })).not.toBeInTheDocument();
    });
  });
});
