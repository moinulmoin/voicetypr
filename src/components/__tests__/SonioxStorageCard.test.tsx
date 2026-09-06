import { toast } from "sonner";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SonioxStorageCard } from "../SonioxStorageCard";

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    warning: vi.fn(),
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
    vi.clearAllMocks();
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
        screen.getByText(/Stored files: 940 · Stored transcriptions: 1900/),
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
        screen.getByText(/Could not read storage usage: Soniox API key not set/),
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
          skippedUnknown: 0,
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
        expect.stringMatching(/Deleted 1940 stored records \(2 still processing\)/),
      ),
    );
    await waitFor(() => {
      expect(screen.getByText(/Stored files: 0 · Stored transcriptions: 0/)).toBeInTheDocument();
    });
  });

  it("warns on partial cleanup failure, retains all counts, and refreshes usage", async () => {
    const user = userEvent.setup();
    let cleaned = false;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_soniox_storage_counts") {
        return { filesTotal: cleaned ? 3 : 5, transcriptionsTotal: 5 };
      }
      if (cmd === "cleanup_soniox_storage") {
        cleaned = true;
        return {
          deletedTranscriptions: 0,
          deletedFiles: 2,
          skippedProcessing: 1,
          skippedUnknown: 4,
          errors: ["Deletion failed"],
        };
      }
      return null;
    });
    render(<SonioxStorageCard />);
    await user.click(screen.getByRole("button", { name: "Clean up stored files" }));
    await waitFor(() =>
      expect(toast.warning).toHaveBeenCalledWith(
        "Deleted 2 stored records (1 still processing) — 4 unrecognized records left untouched; review them in the Soniox console — 1 failed",
      ),
    );
    expect(toast.success).not.toHaveBeenCalled();
    expect(screen.getByText(/Stored files: 3 · Stored transcriptions: 5/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clean up stored files" })).not.toBeDisabled();
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
    expect(screen.getByRole("button", { name: /clean up stored files/i })).not.toBeDisabled();
  });

  it("keeps the cleanup button named during progress and explains untouched records", async () => {
    const user = userEvent.setup();
    let finishCleanup!: (value: unknown) => void;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_soniox_storage_counts") {
        return Promise.resolve({ filesTotal: 3, transcriptionsTotal: 2 });
      }
      if (cmd === "cleanup_soniox_storage") {
        return new Promise((resolve) => {
          finishCleanup = resolve;
        });
      }
      return Promise.resolve(null);
    });
    render(<SonioxStorageCard />);
    await user.click(screen.getByRole("button", { name: "Clean up stored files" }));
    expect(screen.getByRole("button", { name: "Clean up stored files" })).toBeDisabled();
    finishCleanup({
      deletedTranscriptions: 0,
      deletedFiles: 0,
      skippedProcessing: 0,
      skippedUnknown: 5,
      errors: [],
    });
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith(
        expect.stringContaining(
          "5 unrecognized records left untouched; review them in the Soniox console",
        ),
      ),
    );
    expect(screen.getByRole("button", { name: "Clean up stored files" })).not.toBeDisabled();
  });

  it("surfaces the native message when cleanup rejects with a plain string", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_soniox_storage_counts") {
        return { filesTotal: 5, transcriptionsTotal: 5 };
      }
      if (cmd === "cleanup_soniox_storage") {
        // Tauri rejects Rust Result<_, String> errors as bare strings,
        // not Error instances.
        throw "cleanup failed: soniox storage unreachable";
      }
      return null;
    });

    render(<SonioxStorageCard />);

    const button = await screen.findByRole("button", { name: /clean up stored files/i });
    await user.click(button);

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("cleanup failed: soniox storage unreachable");
    });
    expect(screen.getByRole("button", { name: /clean up stored files/i })).not.toBeDisabled();
  });
});
