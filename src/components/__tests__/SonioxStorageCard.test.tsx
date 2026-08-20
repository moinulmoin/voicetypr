import { toast } from "sonner";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SonioxStorageCard } from "../SonioxStorageCard";

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

const invokeMock = vi.hoisted(() => vi.fn());
const listenMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

const unlisten = vi.fn();
listenMock.mockResolvedValue(unlisten);

describe("SonioxStorageCard", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_soniox_storage_counts") {
        return { filesTotal: 940, transcriptionsTotal: 1900 };
      }
      return null;
    });
  });

  it("renders stored-file and transcription counts from the backend", async () => {
    render(<SonioxStorageCard />);

    await waitFor(() => {
      expect(
        screen.getByText(/Stored files: 940 · Stored transcriptions: 1900/)
      ).toBeInTheDocument();
    });
    expect(invokeMock).toHaveBeenCalledWith("get_soniox_storage_counts");
  });

  it("shows the count error instead of usage when the read fails", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_soniox_storage_counts") {
        throw new Error("Soniox API key not set");
      }
      return null;
    });

    render(<SonioxStorageCard />);

    await waitFor(() => {
      expect(
        screen.getByText(/Could not read storage usage: Soniox API key not set/)
      ).toBeInTheDocument();
    });
  });

  it("runs cleanup on click, toasts the drained total, and refreshes counts", async () => {
    const user = userEvent.setup();
    let cleanupCalls = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_soniox_storage_counts") {
        return cleanupCalls === 0
          ? { filesTotal: 940, transcriptionsTotal: 1900 }
          : { filesTotal: 0, transcriptionsTotal: 0 };
      }
      if (cmd === "cleanup_soniox_storage") {
        cleanupCalls += 1;
        return {
          deletedTranscriptions: 1900,
          deletedFiles: 40,
          skippedProcessing: 2,
          errors: [],
        };
      }
      return null;
    });

    render(<SonioxStorageCard />);

    const button = await screen.findByRole("button", { name: /clean up stored files/i });
    await user.click(button);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("cleanup_soniox_storage");
    });
    // Drained total in the toast, then refreshed zero counts on screen.
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith(
        expect.stringMatching(/Deleted 1940 stored records \(2 still processing\)/)
      )
    );
    await waitFor(() => {
      expect(
        screen.getByText(/Stored files: 0 · Stored transcriptions: 0/)
      ).toBeInTheDocument();
    });
  });

  it("keeps the button usable and surfaces an error toast when cleanup fails", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_soniox_storage_counts") {
        return { filesTotal: 5, transcriptionsTotal: 5 };
      }
      if (cmd === "cleanup_soniox_storage") {
        throw new Error("Soniox API key not set");
      }
      return null;
    });

    render(<SonioxStorageCard />);

    const button = await screen.findByRole("button", { name: /clean up stored files/i });
    await user.click(button);

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("Soniox API key not set");
    });
    expect(
      screen.getByRole("button", { name: /clean up stored files/i })
    ).not.toBeDisabled();
  });
});
