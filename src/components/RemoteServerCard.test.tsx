import { render, screen, waitFor, fireEvent, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock Tauri invoke
const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

import { RemoteServerCard, SavedConnection, StatusResponse } from "./RemoteServerCard";

// Test data factory
function createMockServer(overrides: Partial<SavedConnection> = {}): SavedConnection {
  return {
    id: "server-1",
    host: "192.168.1.100",
    port: 47842,
    password: null,
    name: null,
    created_at: Date.now(),
    model: null,
    ...overrides,
  };
}

function createMockStatusResponse(overrides: Partial<StatusResponse> = {}): StatusResponse {
  return {
    status: "ok",
    version: "1.0.0",
    model: "large-v3-turbo",
    name: "Test Server",
    machine_id: "remote-machine-123",
    ...overrides,
  };
}

describe("RemoteServerCard", () => {
  const mockOnSelect = vi.fn();
  const mockOnRemove = vi.fn();
  const mockOnEdit = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();

    // Default: server is online
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_local_machine_id") {
        return Promise.resolve("local-machine-456");
      }
      if (command === "test_remote_server") {
        return Promise.resolve(createMockStatusResponse());
      }
      return Promise.reject(new Error(`Unknown command: ${command}`));
    });
  });

  // ============================================================================
  // Basic Rendering Tests
  // ============================================================================

  describe("basic rendering", () => {
    it("renders server display name when no custom name is set", async () => {
      const server = createMockServer({ name: null });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      // The host:port should be shown as the display name
      expect(screen.getByText("192.168.1.100:47842")).toBeInTheDocument();
    });

    it("renders custom server name when set", async () => {
      const server = createMockServer({ name: "My Home Server" });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      expect(screen.getByText("My Home Server")).toBeInTheDocument();
    });

    it("renders Edit and Remove buttons", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      expect(screen.getByTitle("Edit server")).toBeInTheDocument();
      expect(screen.getByTitle("Remove server")).toBeInTheDocument();
    });
  });

  // ============================================================================
  // Status Display Tests
  // ============================================================================

  describe("status display", () => {
    it("shows Online status when server responds successfully", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Online")).toBeInTheDocument();
      });
    });

    it("shows model name when server is online", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      await waitFor(() => {
        // Model is displayed with a bullet prefix "• large-v3-turbo"
        expect(screen.getByText(/large-v3-turbo/)).toBeInTheDocument();
      });
    });

    it("shows Offline status when server fails to respond", async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === "get_local_machine_id") {
          return Promise.resolve("local-machine-456");
        }
        if (command === "test_remote_server") {
          return Promise.reject(new Error("Connection refused"));
        }
        return Promise.reject(new Error(`Unknown command: ${command}`));
      });

      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Offline")).toBeInTheDocument();
      });
    });

    it("shows Auth Failed status when authentication fails", async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === "get_local_machine_id") {
          return Promise.resolve("local-machine-456");
        }
        if (command === "test_remote_server") {
          return Promise.reject("Authentication failed: Invalid password");
        }
        return Promise.reject(new Error(`Unknown command: ${command}`));
      });

      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Auth Failed")).toBeInTheDocument();
      });
    });

    it("shows This Machine status for self-connection", async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === "get_local_machine_id") {
          return Promise.resolve("same-machine-id");
        }
        if (command === "test_remote_server") {
          return Promise.resolve(createMockStatusResponse({ machine_id: "same-machine-id" }));
        }
        return Promise.reject(new Error(`Unknown command: ${command}`));
      });

      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      await waitFor(() => {
        expect(screen.getByText("This Machine")).toBeInTheDocument();
      });

      // "Cannot use self" is displayed with a bullet prefix
      expect(screen.getByText(/Cannot use self/)).toBeInTheDocument();
    });
  });

  // ============================================================================
  // Active State Tests
  // ============================================================================

  describe("active state", () => {
    it("shows Active badge when isActive is true and server is online", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={true}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Active")).toBeInTheDocument();
      });
    });

    it("does not show Active badge when isActive is false", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Online")).toBeInTheDocument();
      });

      expect(screen.queryByText("Active")).not.toBeInTheDocument();
    });
  });

  // ============================================================================
  // Interaction Tests
  // ============================================================================

  describe("interactions", () => {
    it("calls onSelect when clicking card while server is online", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Online")).toBeInTheDocument();
      });

      const card = screen.getByText("192.168.1.100:47842").closest("[class*='px-4']");
      await act(async () => {
        fireEvent.click(card!);
      });

      expect(mockOnSelect).toHaveBeenCalledWith("server-1");
    });

    it("does not call onSelect when clicking card while server is offline", async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === "get_local_machine_id") {
          return Promise.resolve("local-machine-456");
        }
        if (command === "test_remote_server") {
          return Promise.reject(new Error("Connection refused"));
        }
        return Promise.reject(new Error(`Unknown command: ${command}`));
      });

      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Offline")).toBeInTheDocument();
      });

      const card = screen.getByText("192.168.1.100:47842").closest("[class*='px-4']");
      await act(async () => {
        fireEvent.click(card!);
      });

      expect(mockOnSelect).not.toHaveBeenCalled();
    });

    it("calls onEdit when clicking Edit button", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      const editButton = screen.getByTitle("Edit server");
      await act(async () => {
        fireEvent.click(editButton);
      });

      expect(mockOnEdit).toHaveBeenCalledWith(server);
    });

    it("calls onRemove when clicking Remove button", async () => {
      mockOnRemove.mockResolvedValue(undefined);

      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      const removeButton = screen.getByTitle("Remove server");
      await act(async () => {
        fireEvent.click(removeButton);
      });

      expect(mockOnRemove).toHaveBeenCalledWith("server-1");
    });

    it("Edit button click does not propagate to card selection", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Online")).toBeInTheDocument();
      });

      const editButton = screen.getByTitle("Edit server");
      await act(async () => {
        fireEvent.click(editButton);
      });

      expect(mockOnEdit).toHaveBeenCalled();
      expect(mockOnSelect).not.toHaveBeenCalled();
    });

    it("Remove button click does not propagate to card selection", async () => {
      mockOnRemove.mockResolvedValue(undefined);

      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Online")).toBeInTheDocument();
      });

      const removeButton = screen.getByTitle("Remove server");
      await act(async () => {
        fireEvent.click(removeButton);
      });

      expect(mockOnRemove).toHaveBeenCalled();
      expect(mockOnSelect).not.toHaveBeenCalled();
    });
  });

  // ============================================================================
  // Self-Connection Tests
  // ============================================================================

  describe("self-connection handling", () => {
    it("does not allow selection when server is self", async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === "get_local_machine_id") {
          return Promise.resolve("same-machine-id");
        }
        if (command === "test_remote_server") {
          return Promise.resolve(createMockStatusResponse({ machine_id: "same-machine-id" }));
        }
        return Promise.reject(new Error(`Unknown command: ${command}`));
      });

      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      await waitFor(() => {
        expect(screen.getByText("This Machine")).toBeInTheDocument();
      });

      const card = screen.getByText("192.168.1.100:47842").closest("[class*='px-4']");
      await act(async () => {
        fireEvent.click(card!);
      });

      expect(mockOnSelect).not.toHaveBeenCalled();
    });
  });

  // ============================================================================
  // Edge Cases
  // ============================================================================

  describe("edge cases", () => {
    it("handles different port numbers correctly", async () => {
      const server = createMockServer({ host: "10.0.0.5", port: 8080 });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      expect(screen.getByText("10.0.0.5:8080")).toBeInTheDocument();
    });

    it("handles server with password", async () => {
      const server = createMockServer({ password: "secret123" });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Online")).toBeInTheDocument();
      });
    });
  });
});
