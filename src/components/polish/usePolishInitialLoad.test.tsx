import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { useAiProviderSettings } from "./useAiProviderSettings";
import { usePolishSectionSettings } from "./usePolishSectionSettings";
import { usePolishSettingsLoad } from "./usePolishSettingsLoad";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@/utils/keyring", () => ({
  hasApiKey: vi.fn(async () => false),
  getApiKey: vi.fn(async () => null),
  saveApiKey: vi.fn(),
  removeApiKey: vi.fn(),
}));

const noop = async () => {};
function useInitialLoad(ready: boolean) {
  const section = usePolishSectionSettings({ settings: null, updateSettings: noop });
  const provider = useAiProviderSettings({
    readinessAiReady: ready,
    settingsLoaded: section.settingsLoaded,
    onPolishEnabled: noop,
    onModelSelected: noop,
    onEnabledModelSelected: noop,
    onPolishToggled: noop,
    onActiveProviderCleared: noop,
  });
  usePolishSettingsLoad({
    settingsLoaded: section.settingsLoaded,
    setSettingsLoaded: section.setSettingsLoaded,
    loadAISettings: provider.loadAISettings,
    loadEnhancementOptionsRef: section.loadEnhancementOptionsRef,
    loadWritingSettingsRef: section.loadWritingSettingsRef,
  });
  return { section, provider };
}

function response(command: string, old: boolean): unknown {
  switch (command) {
    case "list_ai_providers":
      return [{ id: "custom", name: old ? "Old" : "New", status: "production" }];
    case "get_ai_settings":
      return {
        enabled: true,
        provider: "custom",
        model: old ? "old-model" : "new-model",
        hasApiKey: true,
        modelsByProvider: { custom: old ? "old-model" : "new-model" },
        aiModelNeedsReselection: old,
      };
    case "get_ai_settings_for_provider":
      return { hasApiKey: old };
    case "get_openai_config":
      return { baseUrl: old ? "https://old.example" : "https://new.example" };
    case "get_enhancement_options":
      return { preset: old ? "Writing" : "Code" };
    case "get_writing_settings":
      return { custom_words: [{ phrase: old ? "old" : "new", enabled: true }] };
    default:
      throw new Error(`Unexpected command: ${command}`);
  }
}

describe("Polish initial loader ownership", () => {
  beforeEach(() => vi.clearAllMocks());

  it.each([
    "list_ai_providers",
    "get_ai_settings",
    "get_ai_settings_for_provider",
    "get_openai_config",
    "get_enhancement_options",
    "get_writing_settings",
  ])("does not apply an old %s response after the replacement load", async (blockedCommand) => {
    let old = true;
    let resolveOld!: (value: unknown) => void;
    const blocked = new Promise((resolve) => {
      resolveOld = resolve;
    });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (old && command === blockedCommand) return blocked;
      return response(command, old);
    });
    const { result, rerender } = renderHook(({ ready }) => useInitialLoad(ready), {
      initialProps: { ready: true },
    });
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        blockedCommand,
        ...(blockedCommand === "get_ai_settings_for_provider" ? [{ provider: "custom" }] : []),
      ),
    );
    old = false;
    rerender({ ready: false });
    await waitFor(() => expect(result.current.section.settingsLoaded).toBe(true));
    await act(async () => {
      resolveOld(response(blockedCommand, true));
    });
    expect(result.current.provider.providers[0].name).toBe("New");
    expect(result.current.provider.aiSettings.model).toBe("new-model");
    expect(result.current.provider.customModelName).toBe("new-model");
    expect(result.current.provider.aiModelNeedsReselection).toBe(false);
    expect(result.current.provider.providerApiKeys.custom).toBe(false);
    expect(result.current.provider.openAIDefaultBaseUrl).toBe("https://new.example");
    expect(result.current.section.enhancementOptions.preset).toBe("Code");
    expect(result.current.section.writingSettings.custom_words[0].phrase).toBe("new");
  });

  it.each(["list_provider_models", "probe_agent_cli"])(
    "allows %s to finish after settings become ready",
    async (backgroundCommand) => {
      const providerId = backgroundCommand === "probe_agent_cli" ? "claude-code" : "openrouter";
      let resolveBackground!: (value: unknown) => void;
      vi.mocked(invoke).mockImplementation(async (command) => {
        if (command === backgroundCommand) {
          return new Promise((resolve) => {
            resolveBackground = resolve;
          });
        }
        if (command === "list_ai_providers") {
          return [{ id: providerId, name: providerId, status: "production" }];
        }
        if (command === "get_ai_settings") {
          return { enabled: true, provider: providerId, model: "model", hasApiKey: true };
        }
        return response(command, false);
      });
      const { result, unmount } = renderHook(() => useInitialLoad(false));
      await waitFor(() => expect(result.current.section.settingsLoaded).toBe(true));
      await act(async () => {
        resolveBackground(
          backgroundCommand === "probe_agent_cli"
            ? { state: "ready" }
            : [{ id: "new-model", name: "New", recommended: true }],
        );
      });
      if (backgroundCommand === "probe_agent_cli") {
        expect(result.current.provider.agentCliStatus[providerId].state).toBe("ready");
      } else {
        expect(result.current.provider.getModels(providerId)[0].id).toBe("new-model");
      }
      unmount();
    },
  );

  it("lets a replacement CLI probe finish without a canceled probe overwriting readiness", async () => {
    const { result } = renderHook(() =>
      useAiProviderSettings({
        readinessAiReady: false,
        settingsLoaded: false,
        onPolishEnabled: noop,
        onModelSelected: noop,
        onEnabledModelSelected: noop,
        onPolishToggled: noop,
        onActiveProviderCleared: noop,
      }),
    );
    let resolveOld!: (value: unknown) => void;
    vi.mocked(invoke).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveOld = resolve;
        }),
    );
    const controller = new AbortController();
    act(() => {
      void result.current.probeAgentCli("claude-code", false, controller.signal);
    });
    act(() => controller.abort());
    vi.mocked(invoke).mockResolvedValueOnce({ state: "ready" });
    await act(async () => {
      await result.current.probeAgentCli("claude-code", false);
    });
    await act(async () => {
      resolveOld({ state: "missing" });
    });
    expect(result.current.agentCliStatus["claude-code"].state).toBe("ready");
    expect(result.current.providerApiKeys["claude-code"]).toBe(true);
    expect(result.current.agentCliProbing["claude-code"]).toBe(false);
  });
});
