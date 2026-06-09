import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { OnboardingDesktop } from "./OnboardingDesktop";

const {
  invokeMock,
  updateSettingsMock,
  onCompleteMock,
  startRecordingMock,
  stopRecordingMock,
  eventListeners,
  modelManagement,
  settingsState,
  recordingState,
} = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  updateSettingsMock: vi.fn(),
  onCompleteMock: vi.fn(),
  startRecordingMock: vi.fn(),
  stopRecordingMock: vi.fn(),
  eventListeners: new Map<string, Set<(event: { payload: unknown }) => void>>(),
  settingsState: {
    hotkey: "CommandOrControl+Shift+Space",
    current_model: "base.en",
    current_model_engine: "whisper",
    speech_language: "en",
    onboarding_completed: false,
  },
  recordingState: {
    state: "idle",
    error: null as string | null,
    isActive: false,
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

vi.mock("@/hooks/useRecording", () => ({
  useRecording: () => ({
    state: recordingState.state,
    error: recordingState.error,
    startRecording: startRecordingMock,
    stopRecording: stopRecordingMock,
    isActive: recordingState.isActive,
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

const emit = (event: string, payload: unknown) => {
  eventListeners.get(event)?.forEach((handler) => handler({ payload }));
};

const renderOnboarding = () =>
  render(
    <OnboardingDesktop
      onComplete={onCompleteMock}
      modelManagement={modelManagement as never}
    />,
  );

beforeEach(() => {
  vi.clearAllMocks();
  eventListeners.clear();
  Object.assign(settingsState, {
    hotkey: "CommandOrControl+Shift+Space",
    current_model: "base.en",
    current_model_engine: "whisper",
    speech_language: "en",
    onboarding_completed: false,
  });
  Object.assign(recordingState, {
    state: "idle",
    error: null,
    isActive: false,
  });
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
  updateSettingsMock.mockResolvedValue(undefined);
  startRecordingMock.mockResolvedValue(undefined);
  stopRecordingMock.mockResolvedValue(undefined);
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
  it("requires a successful first transcription before completing onboarding", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    await user.click(screen.getByRole("button", { name: /start setup/i }));
    await user.click(screen.getByText("Use this device"));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /save hotkey/i }));

    const reviewButton = await screen.findByRole("button", { name: /review result/i });

    expect(screen.queryByText(/current state:/i)).not.toBeInTheDocument();
    expect(screen.getByText(/start a short sample/i)).toBeInTheDocument();
    expect(reviewButton).toBeDisabled();

    await user.click(screen.getByRole("button", { name: /start sample/i }));
    expect(startRecordingMock).toHaveBeenCalledTimes(1);

    emit("transcription-added", {
      text: "Hello from VoiceTypr onboarding.",
      model: "base.en",
      timestamp: "2026-05-18T00:00:00Z",
    });

    await waitFor(() => expect(reviewButton).toBeEnabled());
    expect(screen.getByText("Hello from VoiceTypr onboarding.")).toBeInTheDocument();

    await user.click(reviewButton);
    await user.click(screen.getByRole("button", { name: /go to dashboard/i }));

    expect(updateSettingsMock).toHaveBeenCalledWith({ onboarding_completed: true });
    expect(onCompleteMock).toHaveBeenCalledTimes(1);
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
    await user.click(screen.getByText("Use another VoiceTypr"));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    await user.click(await screen.findByRole("button", { name: /set up this device instead/i }));

    expect(screen.getByText("Prepare this device")).toBeInTheDocument();
    expect(screen.getByText(/select a downloaded model/i)).toBeInTheDocument();
  });

  it("allows an online remote VoiceTypr source without a local model", async () => {
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
    await user.click(screen.getByText("Use another VoiceTypr"));
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
});
