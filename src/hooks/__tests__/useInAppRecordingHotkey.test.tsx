import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";

const platform = vi.hoisted(() => ({ isMacOS: false }));
vi.mock("@/lib/platform", () => ({
  get isMacOS() {
    return platform.isMacOS;
  },
  get isWindows() {
    return !platform.isMacOS;
  },
}));

const mockRecording = vi.hoisted(() => ({
  state: "idle" as string,
  error: null as string | null,
  isActive: false,
  startRecording: vi.fn(),
  stopRecording: vi.fn(),
}));
vi.mock("@/hooks/useRecording", () => ({
  useRecording: () => mockRecording,
}));

const mockSettings = vi.hoisted(() => ({ hotkey: "CommandOrControl+Space" }) as Record<string, unknown>);
vi.mock("@/contexts/SettingsContext", () => ({
  useSetting: (key: string) => mockSettings[key],
}));

import { useInAppRecordingHotkey } from "@/hooks/useInAppRecordingHotkey";

function fireHotkey(target: Element, init: KeyboardEventInit = {}): void {
  target.dispatchEvent(
    new KeyboardEvent("keydown", {
      bubbles: true,
      cancelable: true,
      ctrlKey: true,
      code: "Space",
      key: " ",
      ...init,
    }),
  );
}

describe("useInAppRecordingHotkey", () => {
  let editable: HTMLTextAreaElement;
  let nonEditable: HTMLDivElement;

  beforeEach(() => {
    platform.isMacOS = false;
    mockSettings.hotkey = "CommandOrControl+Space";
    mockRecording.isActive = false;
    mockRecording.state = "idle";
    mockRecording.startRecording.mockReset();
    mockRecording.stopRecording.mockReset();
    editable = document.createElement("textarea");
    nonEditable = document.createElement("div");
    document.body.append(editable, nonEditable);
  });

  afterEach(() => {
    editable.remove();
    nonEditable.remove();
  });

  it("starts recording when the hotkey fires inside a text field", () => {
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable);

    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("stops recording when it is already recording", () => {
    mockRecording.state = "recording";
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable);

    expect(mockRecording.stopRecording).toHaveBeenCalledTimes(1);
    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("ignores the hotkey during transitional states (transcribing)", () => {
    mockRecording.state = "transcribing";
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("ignores the hotkey when focus is not in an editable field", () => {
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(nonEditable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("ignores a combo that does not match the configured hotkey", () => {
    renderHook(() => useInAppRecordingHotkey());

    // Missing the Control modifier → plain Space.
    fireHotkey(editable, { ctrlKey: false });

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("debounces a rapid second press within the window", () => {
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable);
    fireHotkey(editable);

    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
  });

  it("ignores auto-repeat keydowns", () => {
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable, { repeat: true });

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("ignores IME composition events", () => {
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable, { isComposing: true });

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("does nothing when no hotkey is configured", () => {
    mockSettings.hotkey = "";
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("leaves a bare-key hotkey to the field so typing still works", () => {
    mockSettings.hotkey = "Space";
    renderHook(() => useInAppRecordingHotkey());

    const event = new KeyboardEvent("keydown", {
      bubbles: true,
      cancelable: true,
      code: "Space",
      key: " ",
    });
    const preventDefault = vi.spyOn(event, "preventDefault");
    editable.dispatchEvent(event);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
    expect(preventDefault).not.toHaveBeenCalled();
  });

  it("ignores Shift-only combos that can produce typed characters", () => {
    mockSettings.hotkey = "Shift+Space";
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable, { ctrlKey: false, shiftKey: true });

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("ignores AltGr keystrokes so typing isn't stolen (Ctrl+Alt + AltGraph)", () => {
    mockSettings.hotkey = "CommandOrControl+Alt+Q";
    renderHook(() => useInAppRecordingHotkey());

    const event = new KeyboardEvent("keydown", {
      bubbles: true,
      cancelable: true,
      ctrlKey: true,
      altKey: true,
      code: "KeyQ",
      key: "@",
    });
    // jsdom doesn't honor modifierAltGraph init reliably; force AltGraph on.
    Object.defineProperty(event, "getModifierState", {
      value: (key: string) => key === "AltGraph",
    });
    const preventDefault = vi.spyOn(event, "preventDefault");
    editable.dispatchEvent(event);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
    expect(preventDefault).not.toHaveBeenCalled();
  });
});
