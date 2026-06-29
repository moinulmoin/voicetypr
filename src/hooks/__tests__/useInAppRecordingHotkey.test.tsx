import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";

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

const mockInvoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
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

// A bare-modifier isolated-tap primary (e.g. Control alone), as returned by
// get_shortcut_settings when no combo hotkey is configured.
const bareControlBinding = {
  id: "onboarding-primary-hold",
  action: "toggle_recording",
  shortcut: "",
  trigger: "pressed", // pass-through struct field; tapToggleModifier filters on trigger_kind/action only
  enabled: true,
  allow_risky_combo: false,
  trigger_kind: "isolated_tap",
  modifier: { modifier: "control", side: "either" },
};

// Dispatch a clean lone-modifier tap (keydown then keyup, same physical key).
function fireModifierTap(target: Element, init: KeyboardEventInit = {}): void {
  const opts = { bubbles: true, cancelable: true, code: "ControlLeft", key: "Control", ...init };
  target.dispatchEvent(new KeyboardEvent("keydown", opts));
  target.dispatchEvent(new KeyboardEvent("keyup", opts));
}

// Render the hook and flush the async get_shortcut_settings load so the
// bare-modifier binding is in place before dispatching events.
async function renderWithBareModifier(): Promise<void> {
  renderHook(() => useInAppRecordingHotkey());
  await act(async () => {
    // Two microtask flushes: the invoke() promise resolves on the first, its
    // .then() (which sets the bare-modifier ref) on the second.
    await Promise.resolve();
    await Promise.resolve();
  });
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
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue({ bindings: [] });
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

  it("starts recording on a bare-modifier tap inside a text field", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    fireModifierTap(editable);

    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("stops recording on a bare-modifier tap when already recording", async () => {
    mockSettings.hotkey = "";
    mockRecording.state = "recording";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    fireModifierTap(editable);

    expect(mockRecording.stopRecording).toHaveBeenCalledTimes(1);
    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("ignores a chord (Ctrl+C) — a key pressed during the modifier hold", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    const opts = { bubbles: true, cancelable: true };
    editable.dispatchEvent(new KeyboardEvent("keydown", { ...opts, code: "ControlLeft", key: "Control", ctrlKey: true }));
    editable.dispatchEvent(new KeyboardEvent("keydown", { ...opts, code: "KeyC", key: "c", ctrlKey: true }));
    editable.dispatchEvent(new KeyboardEvent("keyup", { ...opts, code: "KeyC", key: "c", ctrlKey: true }));
    editable.dispatchEvent(new KeyboardEvent("keyup", { ...opts, code: "ControlLeft", key: "Control" }));

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("ignores a bare-modifier tap outside an editable field", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    fireModifierTap(nonEditable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("respects the configured modifier side", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({
      bindings: [{ ...bareControlBinding, modifier: { modifier: "control", side: "left" } }],
    });
    await renderWithBareModifier();

    // Right Control when only Left is configured → no toggle.
    fireModifierTap(editable, { code: "ControlRight" });

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("ignores a push-to-talk (modifier_hold) binding — tap path is toggle-only", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({
      bindings: [{ ...bareControlBinding, action: "hold_to_record", trigger_kind: "modifier_hold" }],
    });
    await renderWithBareModifier();

    fireModifierTap(editable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("bails if recording state changed between keydown and keyup", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    const opts = { bubbles: true, cancelable: true, code: "ControlLeft", key: "Control" };
    editable.dispatchEvent(new KeyboardEvent("keydown", opts));
    mockRecording.state = "recording"; // native fired in between
    editable.dispatchEvent(new KeyboardEvent("keyup", opts));

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("does not activate the bare-modifier fallback when get_shortcut_settings fails", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockRejectedValue(new Error("backend unavailable"));
    await renderWithBareModifier();

    fireModifierTap(editable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("clears a pending tap on blur (no toggle on a later keyup)", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    const opts = { bubbles: true, cancelable: true, code: "ControlLeft", key: "Control" };
    editable.dispatchEvent(new KeyboardEvent("keydown", opts));
    window.dispatchEvent(new Event("blur"));
    editable.dispatchEvent(new KeyboardEvent("keyup", opts));

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("debounces a rapid second bare-modifier tap", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    fireModifierTap(editable);
    fireModifierTap(editable);

    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
  });

  it("does not run the bare-modifier path on macOS (native engine handles it)", async () => {
    platform.isMacOS = true;
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    fireModifierTap(editable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });
});
