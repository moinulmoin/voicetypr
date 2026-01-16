import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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

// Mock sonner toast - using inline object to avoid hoisting issues
vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

// Import toast after mocking to get the mocked version
import { toast } from "sonner";

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
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({ firewall_enabled: false, app_allowed: true, may_be_blocked: false });
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
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({ firewall_enabled: false, app_allowed: true, may_be_blocked: false });
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
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({ firewall_enabled: false, app_allowed: true, may_be_blocked: false });
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
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({ firewall_enabled: false, app_allowed: true, may_be_blocked: false });
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
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({ firewall_enabled: false, app_allowed: true, may_be_blocked: false });
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

  describe("when using a remote server", () => {
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
          case "get_active_remote_server":
            return Promise.resolve("remote-server-1");
          case "get_firewall_status":
            return Promise.resolve({ firewall_enabled: false, app_allowed: true, may_be_blocked: false });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows warning when using remote VoiceTypr", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Using remote VoiceTypr")).toBeInTheDocument();
      });

      expect(
        screen.getByText(/Network sharing is unavailable while using a remote VoiceTypr/)
      ).toBeInTheDocument();
    });

    it("disables the toggle when using a remote server", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        const toggle = screen.getByRole("switch");
        expect(toggle).toBeDisabled();
      });
    });
  });

  describe("firewall warning", () => {
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
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: true,
              app_allowed: false,
              may_be_blocked: true,
            });
          case "open_firewall_settings":
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows firewall warning when may_be_blocked is true", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Firewall may block connections")).toBeInTheDocument();
      });

      expect(screen.getByText(/Your macOS firewall is enabled/)).toBeInTheDocument();
    });

    it("has a link to open system settings", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Open System Settings")).toBeInTheDocument();
      });
    });

    it("can trigger check again for firewall status", async () => {
      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Check again")).toBeInTheDocument();
      });

      await user.click(screen.getByText("Check again"));

      await waitFor(() => {
        expect(toast.info).toHaveBeenCalledWith("Checking firewall status...");
      });
    });
  });

  describe("toggle sharing functionality", () => {
    beforeEach(() => {
      vi.clearAllMocks();
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
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({ firewall_enabled: false, app_allowed: true, may_be_blocked: false });
          case "start_sharing":
            return Promise.resolve();
          case "stop_sharing":
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("calls start_sharing when toggle is turned on", async () => {
      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByRole("switch")).not.toBeDisabled();
      });

      await user.click(screen.getByRole("switch"));

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("start_sharing", expect.objectContaining({
          port: 47842,
          serverName: null,
        }));
      });
    });

    it("shows success toast when sharing is enabled", async () => {
      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByRole("switch")).not.toBeDisabled();
      });

      await user.click(screen.getByRole("switch"));

      await waitFor(() => {
        expect(toast.success).toHaveBeenCalledWith("Network sharing enabled");
      });
    });

    it("shows error toast when start_sharing fails", async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === "start_sharing") {
          return Promise.reject(new Error("Port already in use"));
        }
        if (command === "get_settings") {
          return Promise.resolve({
            current_model: "large-v3-turbo",
            auto_insert: true,
            launch_at_startup: false,
          });
        }
        if (command === "get_sharing_status") {
          return Promise.resolve({
            enabled: false,
            port: null,
            model_name: null,
            server_name: null,
            active_connections: 0,
          });
        }
        if (command === "get_local_ips") {
          return Promise.resolve(["192.168.1.100 (eth0)"]);
        }
        if (command === "get_model_status") {
          return Promise.resolve({
            models: [
              { name: "large-v3-turbo", display_name: "Large v3 Turbo", downloaded: true },
            ],
          });
        }
        if (command === "get_active_remote_server") {
          return Promise.resolve(null);
        }
        if (command === "get_firewall_status") {
          return Promise.resolve({ firewall_enabled: false, app_allowed: true, may_be_blocked: false });
        }
        return Promise.reject(new Error(`Unknown command: ${command}`));
      });

      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByRole("switch")).not.toBeDisabled();
      });

      await user.click(screen.getByRole("switch"));

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith("Port already in use");
      });
    });
  });

  describe("copy address functionality", () => {
    beforeEach(() => {
      vi.clearAllMocks();
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
              sharing_port: 47842,
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
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({ firewall_enabled: false, app_allowed: true, may_be_blocked: false });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows success toast when copy button is clicked", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Sharing Active")).toBeInTheDocument();
      });

      // Find and click the copy button
      const copyButton = screen.getByTitle("Copy address");
      fireEvent.click(copyButton);

      // Verify the toast is shown (the clipboard call is a browser API side effect)
      await waitFor(() => {
        expect(toast.success).toHaveBeenCalledWith("Address copied to clipboard");
      });
    });

    it("has a copy button for each IP address", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Sharing Active")).toBeInTheDocument();
      });

      // Should have at least one copy button
      const copyButtons = screen.getAllByTitle("Copy address");
      expect(copyButtons.length).toBeGreaterThan(0);
    });
  });

  describe("port configuration", () => {
    beforeEach(() => {
      vi.clearAllMocks();
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
              sharing_port: 47842,
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
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({ firewall_enabled: false, app_allowed: true, may_be_blocked: false });
          case "stop_sharing":
            return Promise.resolve();
          case "start_sharing":
            return Promise.resolve();
          case "save_settings":
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows port input field when sharing is enabled", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByLabelText("Port")).toBeInTheDocument();
      });
    });

    it("shows save button when port is changed", async () => {
      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByLabelText("Port")).toBeInTheDocument();
      });

      const portInput = screen.getByLabelText("Port");
      await user.clear(portInput);
      await user.type(portInput, "8080");

      await waitFor(() => {
        expect(screen.getByTitle("Save and restart server")).toBeInTheDocument();
      });
    });
  });

  describe("password configuration", () => {
    beforeEach(() => {
      vi.clearAllMocks();
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
              sharing_port: 47842,
              sharing_password: "",
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
              password: null,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [
                { name: "large-v3-turbo", display_name: "Large v3 Turbo", downloaded: true },
              ],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({ firewall_enabled: false, app_allowed: true, may_be_blocked: false });
          case "stop_sharing":
            return Promise.resolve();
          case "start_sharing":
            return Promise.resolve();
          case "save_settings":
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows password input field when sharing is enabled", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByLabelText("Password (Optional)")).toBeInTheDocument();
      });
    });

    it("password field starts as hidden (type=password)", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        const passwordInput = screen.getByLabelText("Password (Optional)");
        expect(passwordInput).toHaveAttribute("type", "password");
      });
    });

    it("toggles password visibility when eye icon is clicked", async () => {
      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByLabelText("Password (Optional)")).toBeInTheDocument();
      });

      const passwordInput = screen.getByLabelText("Password (Optional)");
      expect(passwordInput).toHaveAttribute("type", "password");

      // Find and click the visibility toggle (the Eye icon button)
      const toggleButtons = screen.getAllByRole("button");
      const visibilityToggle = toggleButtons.find(btn => btn.getAttribute("tabindex") === "-1");

      if (visibilityToggle) {
        await user.click(visibilityToggle);

        await waitFor(() => {
          expect(passwordInput).toHaveAttribute("type", "text");
        });
      }
    });

    it("shows save button when password is changed", async () => {
      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByLabelText("Password (Optional)")).toBeInTheDocument();
      });

      const passwordInput = screen.getByLabelText("Password (Optional)");
      await user.type(passwordInput, "secret123");

      await waitFor(() => {
        expect(screen.getByTitle("Save password")).toBeInTheDocument();
      });
    });
  });

  describe("server configuration section", () => {
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
            return Promise.resolve(["192.168.1.100 (eth0)", "10.0.0.5 (WiFi)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [
                { name: "large-v3-turbo", display_name: "Large v3 Turbo", downloaded: true },
              ],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({ firewall_enabled: false, app_allowed: true, may_be_blocked: false });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("displays multiple IP addresses when available", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText(/192\.168\.1\.100:47842/)).toBeInTheDocument();
        expect(screen.getByText(/10\.0\.0\.5:47842/)).toBeInTheDocument();
      });
    });

    it("shows interface names for each IP", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("(eth0)")).toBeInTheDocument();
        expect(screen.getByText("(WiFi)")).toBeInTheDocument();
      });
    });

    it("shows Connect Using label", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Connect Using")).toBeInTheDocument();
      });
    });

    it("shows Server Configuration section", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Server Configuration")).toBeInTheDocument();
      });
    });
  });
});
