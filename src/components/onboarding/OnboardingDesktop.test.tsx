import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { OnboardingDesktop } from "./OnboardingDesktop";

const {
  invokeMock,
  updateSettingsMock,
  onCompleteMock,
  eventListeners,
  modelManagement,
  settingsState,
} = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  updateSettingsMock: vi.fn(),
  onCompleteMock: vi.fn(),
  eventListeners: new Map<string, Set<(event: { payload: unknown }) => void>>(),
  settingsState: {
    hotkey: "CommandOrControl+Shift+Space",
    current_model: "base.en",
    current_model_engine: "whisper",
    speech_language: "en",
    onboarding_completed: false,
  },
  modelManagement: {
    models: {
      "base.en": {
        name: "base.en",
        display_name: "Base English",
        size: 74,
        url: "",
        sha256: "",
        downloaded: true,
        speed_score: 7,
        accuracy_score: 5,
        recommended: false,
        engine: "whisper",
        kind: "local",
        requires_setup: false,
      },
    } as Record<string, any>,
    modelOrder: ["base.en"],
    downloadProgress: {},
    verifyingModels: new Set<string>(),
    loadModels: vi.fn(),
    downloadModel: vi.fn(),
    cancelDownload: vi.fn(),
    deleteModel: vi.fn(),
    sortedModels: [],
    isLoading: false,
  },
}));

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: settingsState,
    updateSettings: updateSettingsMock,
  }),
}));

vi.mock("@/hooks/useMicrophonePermission", () => ({
  useMicrophonePermission: () => ({
    hasPermission: true,
    checkPermission: vi.fn().mockResolvedValue(true),
    requestPermission: vi.fn().mockResolvedValue(true),
  }),
}));

vi.mock("@/hooks/useAccessibilityPermission", () => ({
  useAccessibilityPermission: () => ({
    hasPermission: true,
    checkPermission: vi.fn().mockResolvedValue(true),
    requestPermission: vi.fn().mockResolvedValue(true),
  }),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, handler: (event: { payload: unknown }) => void) => {
    const handlers = eventListeners.get(event) ?? new Set();
    handlers.add(handler);
    eventListeners.set(event, handlers);
    return Promise.resolve(() => handlers.delete(handler));
  }),
}));

vi.mock("@tauri-apps/plugin-shell", () => ({
  open: vi.fn().mockResolvedValue(undefined),
}));

const platformMock = vi.hoisted(() => ({ isMacOS: true, isWindows: false, isLinux: false }));
vi.mock("@/lib/platform", () => platformMock);

const renderOnboarding = () =>
  render(
    <OnboardingDesktop
      onComplete={onCompleteMock}
      modelManagement={modelManagement as never}
    />,
  );

beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(platformMock, { isMacOS: true, isWindows: false, isLinux: false });
  eventListeners.clear();
  Object.assign(settingsState, {
    hotkey: "CommandOrControl+Shift+Space",
    current_model: "base.en",
    current_model_engine: "whisper",
    speech_language: "en",
    onboarding_completed: false,
  });
  delete (settingsState as Record<string, unknown>).transcription_acceleration;
  modelManagement.models = {
    "base.en": {
      name: "base.en",
      display_name: "Base English",
      size: 74,
      url: "",
      sha256: "",
      downloaded: true,
      speed_score: 7,
      accuracy_score: 5,
      recommended: false,
      engine: "whisper",
      kind: "local",
      requires_setup: false,
    },
  };
  modelManagement.modelOrder = ["base.en"];
  updateSettingsMock.mockImplementation((updates: Partial<typeof settingsState>) => {
    Object.assign(settingsState, updates);
    return Promise.resolve();
  });
  invokeMock.mockImplementation((command: string) => {
    switch (command) {
      case "discover_remote_servers":
        return Promise.resolve([]);
      case "list_remote_servers":
        return Promise.resolve([]);
      case "get_active_remote_server":
        return Promise.resolve(null);
      case "set_active_remote_server":
      case "set_global_shortcut":
        return Promise.resolve(true);
      default:
        return Promise.resolve(null);
    }
  });
});

describe("OnboardingDesktop", () => {
  it("saves the hotkey and reaches success without a sample transcription", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    await user.click(screen.getByRole("button", { name: /start setup/i }));
    await user.click(screen.getByText("Use this device"));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /save hotkey/i }));

    await screen.findByRole("heading", { name: /you're all set/i });
    expect(screen.queryByText(/do your first transcription/i)).not.toBeInTheDocument();
    expect(eventListeners.has("transcription-added")).toBe(false);
    expect(invokeMock).toHaveBeenCalledWith("set_global_shortcut", {
      shortcut: "CommandOrControl+Shift+Space",
    });

    // Success screen (Screen A): advance to the upgrade screen.
    await user.click(screen.getByRole("button", { name: /continue/i }));
    // Upgrade screen (Screen B): completion happens via "Continue".
    await user.click(screen.getByRole("button", { name: /^continue$/i }));

    expect(updateSettingsMock).toHaveBeenCalledWith({ onboarding_completed: true });
    expect(onCompleteMock).toHaveBeenCalledTimes(1);
    expect(onCompleteMock).toHaveBeenCalledWith(undefined);
  });

  it("routes to the License tab when the user already has a license", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    await user.click(screen.getByRole("button", { name: /start setup/i }));
    await user.click(screen.getByText("Use this device"));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /save hotkey/i }));

    await screen.findByRole("heading", { name: /you're all set/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /already have a license/i }));

    expect(updateSettingsMock).toHaveBeenCalledWith({ onboarding_completed: true });
    expect(onCompleteMock).toHaveBeenCalledWith("license");
  });

  it("strips a stale onboarding hold binding when a combo hotkey is saved", async () => {
    const user = userEvent.setup();
    const userBinding = {
      id: "user-custom",
      action: "copy_last_transcription",
      shortcut: "CommandOrControl+Shift+C",
      trigger: "pressed",
      enabled: true,
      allow_risky_combo: false,
      trigger_kind: "combo",
      modifier: null,
    };
    const onboardingHold = {
      id: "onboarding-primary-hold",
      action: "hold_to_record",
      shortcut: "",
      trigger: "hold",
      enabled: true,
      allow_risky_combo: false,
      trigger_kind: "modifier_hold",
      modifier: { modifier: "alt", side: "right" },
    };
    invokeMock.mockImplementation((command: string) => {
      switch (command) {
        case "discover_remote_servers":
        case "list_remote_servers":
          return Promise.resolve([]);
        case "get_active_remote_server":
          return Promise.resolve(null);
        case "set_active_remote_server":
        case "set_global_shortcut":
          return Promise.resolve(true);
        case "get_shortcut_settings":
          return Promise.resolve({ bindings: [userBinding, onboardingHold] });
        default:
          return Promise.resolve(null);
      }
    });
    renderOnboarding();

    await user.click(screen.getByRole("button", { name: /start setup/i }));
    await user.click(screen.getByText("Use this device"));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /save hotkey/i }));

    await screen.findByRole("heading", { name: /you're all set/i });

    // Combo save registers the primary global shortcut AND removes only the
    // onboarding-created hold binding, so recording can never fire from both a
    // global shortcut and a modifier_hold trigger at once.
    expect(invokeMock).toHaveBeenCalledWith("set_global_shortcut", {
      shortcut: "CommandOrControl+Shift+Space",
    });
    expect(invokeMock).toHaveBeenCalledWith("update_shortcut_settings", {
      settings: { bindings: [userBinding] },
    });
  });

  it("requires an explicit source choice even when a local model is already saved", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    await user.click(screen.getByRole("button", { name: /start setup/i }));

    const continueButton = screen.getByRole("button", { name: /continue/i });
    expect(continueButton).toBeDisabled();
    expect(updateSettingsMock).not.toHaveBeenCalledWith(expect.objectContaining({
      current_model: "base.en",
    }));

    await user.click(screen.getByText("Use this device"));
    expect(continueButton).toBeEnabled();
  });

  it("guides users to select a downloaded local model before continuing", async () => {
    const user = userEvent.setup();
    settingsState.current_model = "";
    renderOnboarding();

    await user.click(screen.getByRole("button", { name: /start setup/i }));
    await user.click(screen.getByText("Use this device"));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    expect(screen.getByText(/select a downloaded model/i)).toBeInTheDocument();
    expect(screen.getByText(/onboarding needs one selected/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /continue/i })).toBeDisabled();
  });

  it("lets remote-first users switch to local setup from the empty remote state", async () => {
    const user = userEvent.setup();
    settingsState.current_model = "";
    renderOnboarding();

    await user.click(screen.getByRole("button", { name: /start setup/i }));
    await user.click(screen.getByText("Use another Voicetypr"));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    await user.click(await screen.findByRole("button", { name: /set up this device instead/i }));

    expect(screen.getByText("Prepare this device")).toBeInTheDocument();
    expect(screen.getByText(/select a downloaded model/i)).toBeInTheDocument();
  });

  it("allows an online remote Voicetypr source without a local model", async () => {
    const user = userEvent.setup();
    settingsState.current_model = "";
    modelManagement.models = {} as Record<string, any>;
    modelManagement.modelOrder = [];

    invokeMock.mockImplementation((command: string, args?: { serverId?: string }) => {
      switch (command) {
        case "discover_remote_servers":
          return Promise.resolve([]);
        case "list_remote_servers":
          return Promise.resolve([
            {
              id: "remote-1",
              name: "Studio Mac",
              host: "10.0.0.12",
              port: 47842,
              created_at: 1,
              model: "parakeet-tdt-0.6b-v2",
              status: "Online",
            },
          ]);
        case "get_active_remote_server":
          return Promise.resolve(null);
        case "check_remote_server_status":
          return Promise.resolve({
            id: args?.serverId ?? "remote-1",
            name: "Studio Mac",
            host: "10.0.0.12",
            port: 47842,
            created_at: 1,
            model: "parakeet-tdt-0.6b-v2",
            status: "Online",
          });
        case "set_active_remote_server":
        case "set_global_shortcut":
          return Promise.resolve(true);
        default:
          return Promise.resolve(null);
      }
    });

    renderOnboarding();

    await user.click(screen.getByRole("button", { name: /start setup/i }));
    await user.click(screen.getByText("Use another Voicetypr"));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    const useServerButton = await screen.findByRole("button", {
      name: /use this server/i,
    });
    await user.click(useServerButton);

    expect(invokeMock).toHaveBeenCalledWith("set_active_remote_server", {
      serverId: "remote-1",
    });
    expect(screen.getByRole("button", { name: /continue/i })).toBeEnabled();
  });

  it("Windows GPU toggle ON→OFF persists 'cpu' (default state: acceleration undefined → switch ON)", async () => {
    platformMock.isMacOS = false;
    platformMock.isWindows = true;
    // settingsState.transcription_acceleration is undefined → switch resolves to ON
    const user = userEvent.setup();
    renderOnboarding();

    await user.click(screen.getByRole("button", { name: /start setup/i }));
    await user.click(screen.getByText("Use this device"));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    const gpuSwitch = screen.getByRole("switch", { name: /use gpu acceleration/i });
    expect(gpuSwitch).toBeInTheDocument();
    expect(gpuSwitch).toHaveAttribute("aria-checked", "true");

    await user.click(gpuSwitch);
    expect(updateSettingsMock).toHaveBeenCalledWith({ transcription_acceleration: "cpu" });
  });

  it("Windows GPU toggle OFF→ON persists 'auto' (prior state: acceleration 'cpu' → switch OFF)", async () => {
    platformMock.isMacOS = false;
    platformMock.isWindows = true;
    (settingsState as Record<string, unknown>).transcription_acceleration = "cpu";
    const user = userEvent.setup();
    renderOnboarding();

    await user.click(screen.getByRole("button", { name: /start setup/i }));
    await user.click(screen.getByText("Use this device"));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    const gpuSwitch = screen.getByRole("switch", { name: /use gpu acceleration/i });
    expect(gpuSwitch).toHaveAttribute("aria-checked", "false");

    await user.click(gpuSwitch);
    expect(updateSettingsMock).toHaveBeenCalledWith({ transcription_acceleration: "auto" });
  });

  it("does not show the GPU toggle on macOS", async () => {
    // platformMock defaults to isMacOS:true, isWindows:false (reset by beforeEach)
    const user = userEvent.setup();
    renderOnboarding();

    // Navigate through macOS flow to readiness (welcome→source→permissions→readiness)
    await user.click(screen.getByRole("button", { name: /start setup/i }));
    await user.click(screen.getByText("Use this device"));
    await user.click(screen.getByRole("button", { name: /continue/i })); // source→permissions
    await user.click(screen.getByRole("button", { name: /continue/i })); // permissions→readiness

    expect(screen.queryByText(/use gpu acceleration/i)).not.toBeInTheDocument();
  });

  it("saves isolated_tap binding when bare modifier captured with Hold to talk OFF", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    // Navigate to hotkey step (macOS: welcome→source→permissions→readiness→hotkey)
    await user.click(screen.getByRole("button", { name: /start setup/i }));
    await user.click(screen.getByText("Use this device"));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    // Enter HotkeyInput edit mode
    await user.click(screen.getByTitle("Change hotkey"));

    // Simulate Right Alt bare modifier press + release; waitFor ensures the
    // keydown listener is attached and pendingBareModifier is set before clicking Save.
    fireEvent.keyDown(window, { key: "Alt", code: "AltRight", altKey: true });
    fireEvent.keyUp(window, { key: "Alt", code: "AltRight" });
    await waitFor(() => expect(screen.getByTitle("Save hotkey")).not.toBeDisabled());

    // Save within HotkeyInput (the checkmark icon button)
    await user.click(screen.getByTitle("Save hotkey"));

    // Hold to talk is OFF by default; save the step
    await user.click(screen.getByRole("button", { name: /save hotkey/i }));

    await screen.findByRole("heading", { name: /you're all set/i });

    // isolated_tap / toggle_recording / pressed
    expect(invokeMock).toHaveBeenCalledWith("update_shortcut_settings", {
      settings: {
        bindings: [
          expect.objectContaining({
            id: "onboarding-primary-hold",
            trigger_kind: "isolated_tap",
            action: "toggle_recording",
            trigger: "pressed",
            modifier: { modifier: "alt", side: "right" },
            shortcut: "",
          }),
        ],
      },
    });
    expect(updateSettingsMock).toHaveBeenCalledWith(
      expect.objectContaining({ recording_mode: "toggle" }),
    );
  });

  it("saves modifier_hold binding when bare modifier captured with Hold to talk ON", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    // Navigate to hotkey step
    await user.click(screen.getByRole("button", { name: /start setup/i }));
    await user.click(screen.getByText("Use this device"));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    // Enter HotkeyInput edit mode
    await user.click(screen.getByTitle("Change hotkey"));

    // Simulate Right Alt bare modifier press + release
    fireEvent.keyDown(window, { key: "Alt", code: "AltRight", altKey: true });
    fireEvent.keyUp(window, { key: "Alt", code: "AltRight" });
    await waitFor(() => expect(screen.getByTitle("Save hotkey")).not.toBeDisabled());

    // Save within HotkeyInput
    await user.click(screen.getByTitle("Save hotkey"));

    // Toggle "Hold to talk" ON
    await user.click(screen.getByRole("switch", { name: /hold to talk/i }));

    // Save the step
    await user.click(screen.getByRole("button", { name: /save hotkey/i }));

    await screen.findByRole("heading", { name: /you're all set/i });

    // modifier_hold / hold_to_record / hold
    expect(invokeMock).toHaveBeenCalledWith("update_shortcut_settings", {
      settings: {
        bindings: [
          expect.objectContaining({
            id: "onboarding-primary-hold",
            trigger_kind: "modifier_hold",
            action: "hold_to_record",
            trigger: "hold",
            modifier: { modifier: "alt", side: "right" },
            shortcut: "",
          }),
        ],
      },
    });
    expect(updateSettingsMock).toHaveBeenCalledWith(
      expect.objectContaining({ recording_mode: "push_to_talk" }),
    );
  });

  it("defaults telemetry to ON and persists consent=true on completion", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    // Navigate to the success step (welcome→source→permissions→readiness→hotkey).
    await user.click(screen.getByRole("button", { name: /start setup/i }));
    await user.click(screen.getByText("Use this device"));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /save hotkey/i }));

    await screen.findByRole("heading", { name: /you're all set/i });

    // Anonymous error tracking is opt-out: the checkbox is checked by default.
    const telemetryCheckbox = screen.getByRole("checkbox", { name: /send anonymous error reports/i });
    expect(telemetryCheckbox).toBeChecked();

    // Accepting the default and finishing must enable diagnostics.
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_telemetry_consent", { enabled: true }),
    );
  });

  it("persists telemetry consent=false when the success-step checkbox is unchecked", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    await user.click(screen.getByRole("button", { name: /start setup/i }));
    await user.click(screen.getByText("Use this device"));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /save hotkey/i }));

    await screen.findByRole("heading", { name: /you're all set/i });

    // Uncheck the default-on consent box before finishing.
    await user.click(screen.getByRole("checkbox", { name: /send anonymous error reports/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_telemetry_consent", { enabled: false }),
    );
  });
});
