import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ShortcutsSection } from "../ShortcutsSection";
import type { ShortcutActionDefinition, ShortcutSettings } from "@/types/shortcuts";
import { toast } from "sonner";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@/lib/platform", () => ({
  isMacOS: true,
  isWindows: false,
  isLinux: false,
}));

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
  },
}));

const actionDefinitions: ShortcutActionDefinition[] = [
  {
    action: "toggle_recording",
    label: "Toggle Recording",
    description: "Start or stop recording from anywhere.",
    section: "Recording",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "hold_to_record",
    label: "Hold to Record",
    description: "Record only while the shortcut is held.",
    section: "Recording",
    recommended_trigger: "hold",
    allows_single_key: true,
  },
  {
    action: "cancel_recording",
    label: "Cancel Recording",
    description: "Cancel the current recording.",
    section: "Recording",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "copy_last_transcription",
    label: "Copy Last Transcription",
    description: "Copy the most recent transcript to the clipboard.",
    section: "History",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "paste_last_transcription",
    label: "Paste Last Transcription",
    description: "Paste the most recent transcript.",
    section: "History",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "cycle_formatting_mode",
    label: "Cycle Formatting Mode",
    description: "Move to the next formatting mode.",
    section: "Formatting",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "set_personal_dictation",
    label: "Personal Dictation",
    description: "Switch to personal dictation.",
    section: "Formatting",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "set_clean_dictation",
    label: "Clean Dictation",
    description: "Switch to clean dictation.",
    section: "Formatting",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "set_writing",
    label: "Writing",
    description: "Switch to writing mode.",
    section: "Formatting",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "set_notes",
    label: "Notes",
    description: "Switch to notes mode.",
    section: "Formatting",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "set_message",
    label: "Message",
    description: "Switch to message mode.",
    section: "Formatting",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "set_code",
    label: "Code",
    description: "Switch to code mode.",
    section: "Formatting",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "open_dashboard",
    label: "Open Dashboard",
    description: "Show the VoiceTypr dashboard.",
    section: "App",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
];

function arrangeInvoke(
  settings: ShortcutSettings = { bindings: [] },
  onUpdate: (submittedSettings: ShortcutSettings) => ShortcutSettings | Promise<ShortcutSettings> = (submittedSettings) => submittedSettings,
  options: { rejectSettings?: Error } = {},
) {
  const invokeMock = vi.mocked(invoke);

  invokeMock.mockImplementation((command: string, args?: unknown) => {
    if (command === "list_shortcut_actions") {
      return Promise.resolve(actionDefinitions);
    }

    if (command === "get_shortcut_settings") {
      return options.rejectSettings
        ? Promise.reject(options.rejectSettings)
        : Promise.resolve(settings);
    }

    if (command === "update_shortcut_settings") {
      return Promise.resolve(onUpdate((args as { settings: ShortcutSettings }).settings)).then((updatedSettings) => {
        settings = updatedSettings;
        return settings;
      });
    }

    return Promise.resolve(undefined);
  });
}

describe("ShortcutsSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    arrangeInvoke();
  });

  it("loads action rows with empty default settings", async () => {
    render(<ShortcutsSection />);

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Recording" })).toBeInTheDocument();
      expect(screen.getByRole("heading", { name: "History" })).toBeInTheDocument();
      expect(screen.getByRole("heading", { name: "Formatting" })).toBeInTheDocument();
      expect(screen.getByRole("heading", { name: "App" })).toBeInTheDocument();
    });

    expect(screen.getByText("Toggle Recording")).toBeInTheDocument();
    expect(screen.getByText("Copy Last Transcription")).toBeInTheDocument();
    expect(screen.getByText("Open Dashboard")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Set shortcut" })).toHaveLength(actionDefinitions.length);
  });

  it("adds a copy-last shortcut and sends the full settings object", async () => {
    const user = userEvent.setup();
    render(<ShortcutsSection />);

    const copyRow = await screen.findByRole("group", { name: "Copy Last Transcription" });
    await user.click(within(copyRow).getByRole("button", { name: "Set shortcut" }));
    await user.keyboard("{Alt>}{Shift>}c{/Shift}{/Alt}");
    await user.click(within(copyRow).getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_shortcut_settings", {
        settings: {
          bindings: [
            expect.objectContaining({
              action: "copy_last_transcription",
              shortcut: "Alt+Shift+C",
              trigger: "pressed",
              enabled: true,
              allow_risky_combo: false,
            }),
          ],
        },
      });
    });
  });

  it("blocks duplicate shortcuts before saving and names the assigned action", async () => {
    const user = userEvent.setup();
    arrangeInvoke({
      bindings: [
        {
          id: "copy-binding",
          action: "copy_last_transcription",
          shortcut: "Alt+C",
          trigger: "pressed",
          enabled: true,
          allow_risky_combo: false,
        },
      ],
    });

    render(<ShortcutsSection />);

    const pasteRow = await screen.findByRole("group", { name: "Paste Last Transcription" });
    await user.click(within(pasteRow).getByRole("button", { name: "Set shortcut" }));
    await user.keyboard("{Alt>}c{/Alt}");
    await user.click(within(pasteRow).getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("Shortcut already assigned", {
        description: "Alt+C is already assigned to Copy Last Transcription.",
      });
    });
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "update_shortcut_settings")).toHaveLength(0);
  });

  it("allows reusing a shortcut from a disabled binding", async () => {
    const user = userEvent.setup();
    arrangeInvoke({
      bindings: [
        {
          id: "copy-binding",
          action: "copy_last_transcription",
          shortcut: "Alt+C",
          trigger: "pressed",
          enabled: false,
          allow_risky_combo: false,
        },
      ],
    });

    render(<ShortcutsSection />);

    const pasteRow = await screen.findByRole("group", { name: "Paste Last Transcription" });
    await user.click(within(pasteRow).getByRole("button", { name: "Set shortcut" }));
    await user.keyboard("{Alt>}c{/Alt}");
    await user.click(within(pasteRow).getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_shortcut_settings", {
        settings: {
          bindings: [
            expect.objectContaining({
              id: "copy-binding",
              enabled: false,
            }),
            expect.objectContaining({
              action: "paste_last_transcription",
              shortcut: "Alt+C",
              enabled: true,
            }),
          ],
        },
      });
    });
    expect(toast.error).not.toHaveBeenCalledWith("Shortcut already assigned", expect.anything());
  });

  it("uses the shortcut settings returned by the backend after saving", async () => {
    const user = userEvent.setup();
    arrangeInvoke({ bindings: [] }, (submittedSettings) => ({
      bindings: submittedSettings.bindings.map((binding) => ({
        ...binding,
        shortcut: "Alt+R",
      })),
    }));

    render(<ShortcutsSection />);

    const copyRow = await screen.findByRole("group", { name: "Copy Last Transcription" });
    await user.click(within(copyRow).getByRole("button", { name: "Set shortcut" }));
    await user.keyboard("{Alt>}{Shift>}c{/Shift}{/Alt}");
    await user.click(within(copyRow).getByRole("button", { name: "Save" }));

    await waitFor(() => {
      // Backend returned "Alt+R"; verify the read-mode display reflects it (not the submitted "Alt+Shift+C")
      const display = within(copyRow).getByLabelText("Copy Last Transcription shortcut");
      expect(display).toHaveTextContent("Alt+R");
      expect(display).not.toHaveTextContent("Shift");
    });
  });

  it("rejects multi-key hold-to-record shortcuts without a modifier", async () => {
    const user = userEvent.setup();
    render(<ShortcutsSection />);

    const holdRow = await screen.findByRole("group", { name: "Hold to Record" });
    await user.click(within(holdRow).getByRole("button", { name: "Set shortcut" }));

    fireEvent.keyDown(window, { key: "a", code: "KeyA" });
    await waitFor(() => {
      expect(within(holdRow).getByText("a")).toBeInTheDocument();
      expect(within(holdRow).getByRole("button", { name: "Save" })).toBeEnabled();
    });
    fireEvent.keyDown(window, { key: "b", code: "KeyB" });

    expect(within(holdRow).getByText("Multi-key shortcuts must include at least one modifier key")).toBeInTheDocument();
    expect(within(holdRow).getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("shows the simplified capture UI without a single-key toggle", async () => {
    const user = userEvent.setup();
    render(<ShortcutsSection />);

    const holdRow = await screen.findByRole("group", { name: "Hold to Record" });
    await user.click(within(holdRow).getByRole("button", { name: "Set shortcut" }));

    expect(within(holdRow).queryByRole("switch", { name: "Use a single key" })).not.toBeInTheDocument();
    expect(within(holdRow).queryByText(/Only non-typing keys are allowed/)).not.toBeInTheDocument();
    expect(within(holdRow).getByText(/won't work/)).toBeInTheDocument();
  });

  it("saves a single-key binding on Hold to Record", async () => {
    const user = userEvent.setup();
    arrangeInvoke({ bindings: [] });
    render(<ShortcutsSection />);

    const holdRow = await screen.findByRole("group", { name: "Hold to Record" });

    // Add a Hold to Record binding
    await user.click(within(holdRow).getByRole("button", { name: "Set shortcut" }));

    // Enter a single key — valid because single-key validation is now active
    fireEvent.keyDown(window, { key: "a", code: "KeyA" });
    await waitFor(() => {
      expect(within(holdRow).getByRole("button", { name: "Save" })).toBeEnabled();
    });
    await user.click(within(holdRow).getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_shortcut_settings", {
        settings: {
          bindings: [
            expect.objectContaining({
              action: "hold_to_record",
              allow_risky_combo: true,
            }),
          ],
        },
      });
    });
  });

  it("does not show a single-key toggle on a non-recording action", async () => {
    const user = userEvent.setup();
    render(<ShortcutsSection />);

    const copyRow = await screen.findByRole("group", { name: "Copy Last Transcription" });
    await user.click(within(copyRow).getByRole("button", { name: "Set shortcut" }));

    expect(within(copyRow).queryByRole("switch", { name: "Use a single key" })).not.toBeInTheDocument();
    expect(within(copyRow).queryByRole("switch", { name: "Hold to talk (push-to-talk)" })).not.toBeInTheDocument();
  });

  it("saves a single-key F1 binding on a non-recording action", async () => {
    const user = userEvent.setup();
    arrangeInvoke({ bindings: [] });
    render(<ShortcutsSection />);

    const copyRow = await screen.findByRole("group", { name: "Copy Last Transcription" });
    await user.click(within(copyRow).getByRole("button", { name: "Set shortcut" }));

    fireEvent.keyDown(window, { key: "F1", code: "F1" });
    await waitFor(() => {
      expect(within(copyRow).getByRole("button", { name: "Save" })).toBeEnabled();
    });
    await user.click(within(copyRow).getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_shortcut_settings", {
        settings: {
          bindings: [
            expect.objectContaining({
              action: "copy_last_transcription",
              shortcut: "F1",
              allow_risky_combo: true,
            }),
          ],
        },
      });
    });
  });

  it("removes an existing binding via Remove (no delete button or enable switch)", async () => {
    const user = userEvent.setup();
    arrangeInvoke({
      bindings: [
        {
          id: "copy-binding",
          action: "copy_last_transcription",
          shortcut: "Alt+C",
          trigger: "pressed",
          enabled: true,
          allow_risky_combo: false,
        },
      ],
    });
    render(<ShortcutsSection />);

    const copyRow = await screen.findByRole("group", { name: "Copy Last Transcription" });
    expect(within(copyRow).queryByRole("button", { name: /Delete/ })).not.toBeInTheDocument();
    expect(within(copyRow).queryByRole("switch", { name: /enabled/ })).not.toBeInTheDocument();

    await user.click(within(copyRow).getByRole("button", { name: "Remove" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_shortcut_settings", {
        settings: { bindings: [] },
      });
    });
  });

  it("offers only one shortcut slot per action (no add-another button once set)", async () => {
    arrangeInvoke({
      bindings: [
        {
          id: "copy-binding",
          action: "copy_last_transcription",
          shortcut: "Alt+C",
          trigger: "pressed",
          enabled: true,
          allow_risky_combo: false,
        },
      ],
    });
    render(<ShortcutsSection />);

    const copyRow = await screen.findByRole("group", { name: "Copy Last Transcription" });
    expect(within(copyRow).queryByRole("button", { name: "Set shortcut" })).not.toBeInTheDocument();
    expect(within(copyRow).queryByRole("button", { name: "Add shortcut" })).not.toBeInTheDocument();
    expect(within(copyRow).getByRole("button", { name: "Edit" })).toBeInTheDocument();
  });

  it("captures a bare modifier on Toggle Recording as isolated_tap", async () => {
    const user = userEvent.setup();
    arrangeInvoke({ bindings: [] });
    render(<ShortcutsSection />);

    const toggleRow = await screen.findByRole("group", { name: "Toggle Recording" });
    await user.click(within(toggleRow).getByRole("button", { name: "Set shortcut" }));

    // Bare Right Alt press+release → bare-modifier capture.
    fireEvent.keyDown(window, { key: "Alt", code: "AltRight", altKey: true });
    fireEvent.keyUp(window, { key: "Alt", code: "AltRight" });
    await waitFor(() => {
      expect(within(toggleRow).getByRole("button", { name: "Save" })).toBeEnabled();
    });
    await user.click(within(toggleRow).getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_shortcut_settings", {
        settings: {
          bindings: [
            expect.objectContaining({
              action: "toggle_recording",
              trigger_kind: "isolated_tap",
              trigger: "pressed",
              modifier: { modifier: "alt", side: "right" },
              shortcut: "",
            }),
          ],
        },
      });
    });
  });

  it("captures a bare modifier on Hold to Record as modifier_hold", async () => {
    const user = userEvent.setup();
    arrangeInvoke({ bindings: [] });
    render(<ShortcutsSection />);

    const holdRow = await screen.findByRole("group", { name: "Hold to Record" });
    await user.click(within(holdRow).getByRole("button", { name: "Set shortcut" }));

    fireEvent.keyDown(window, { key: "Alt", code: "AltRight", altKey: true });
    fireEvent.keyUp(window, { key: "Alt", code: "AltRight" });
    await waitFor(() => {
      expect(within(holdRow).getByRole("button", { name: "Save" })).toBeEnabled();
    });
    await user.click(within(holdRow).getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_shortcut_settings", {
        settings: {
          bindings: [
            expect.objectContaining({
              action: "hold_to_record",
              trigger_kind: "modifier_hold",
              trigger: "hold",
              modifier: { modifier: "alt", side: "right" },
              shortcut: "",
            }),
          ],
        },
      });
    });
  });

});
