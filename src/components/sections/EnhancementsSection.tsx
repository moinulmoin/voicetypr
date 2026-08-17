import { ApiKeyModal } from "@/components/ApiKeyModal";
import { EnhancementSettings } from "@/components/EnhancementSettings";
import { OpenAICompatConfigModal } from "@/components/OpenAICompatConfigModal";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import {
  Combobox,
  ComboboxCollection,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxGroup,
  ComboboxInput,
  ComboboxItem,
  ComboboxLabel,
  ComboboxList,
} from "@/components/ui/combobox";
import {
  FieldDescription,
  FieldSet,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type {
  AISettings,
  EnhancementOptions,
  EnhancementPreset,
} from "@/types/ai";
import { fromBackendOptions, toBackendOptions } from "@/types/ai";
import type { WritingSettings } from "@/types/writing";
import { defaultWritingSettings, mergeWritingSettings } from "@/types/writing";
import type {
  AiProvider,
  AIProviderConfig,
  AIProviderModel,
  AgentCliProbe,
  AgentCliProbeState,
} from "@/types/providers";
import { toProviderConfig } from "@/types/providers";
import { useAllProviderModels } from "@/hooks/useProviderModels";
import {
  hasApiKey,
  removeApiKey,
  saveApiKey,
  getApiKey,
} from "@/utils/keyring";
import { getErrorMessage } from "@/utils/error";
import { useReadinessState } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { humanizeModelId } from "@/lib/model-display";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState, useCallback, useRef } from "react";
import { toast } from "sonner";
import {
  AlertTriangle,
  Brain,
  Check,
  ChevronDown,
  ExternalLink,
  HelpCircle,
  Key,
  RefreshCw,
  Search,
  Settings2,
  Trash2,
  X,
} from "lucide-react";
import { createLogger } from "@/lib/logger";
import {
  useEnhancementsStore,
  type PolishErrorKind,
} from "@/state/enhancements";

const log = createLogger("enhancements");

type ProviderTab = "cloud" | "local";

type AISettingsResponse = Omit<
  AISettings,
  "modelsByProvider" | "reasoningByProvider" | "fastModeByProvider"
> & {
  modelsByProvider?: Record<string, string>;
  reasoningByProvider?: Record<string, string>;
  fastModeByProvider?: Record<string, boolean>;
  aiModelNeedsReselection?: boolean;
};

const normalizeAISettings = (settings: AISettingsResponse): AISettings => ({
  ...settings,
  modelsByProvider: settings.modelsByProvider ?? {},
  reasoningByProvider: settings.reasoningByProvider ?? {},
  fastModeByProvider: settings.fastModeByProvider ?? {},
});

const providerSupportsReasoning = (provider: AIProviderConfig) =>
  provider.supports_reasoning ?? provider.supportsReasoning ?? false;

const formatModelCost = (model: AIProviderModel) => {
  if (model.costInput == null && model.costOutput == null) {
    return null;
  }
  const input = model.costInput == null ? "?" : `$${model.costInput}`;
  const output = model.costOutput == null ? "?" : `$${model.costOutput}`;
  return `${input}/${output}`;
};

const modelMatchesQuery = (model: AIProviderModel, query: string) =>
  model.id.toLowerCase().includes(query) ||
  model.name.toLowerCase().includes(query) ||
  Boolean(model.sourceProvider?.toLowerCase().includes(query));

/// Subscription-authenticated local coding CLIs.
const AGENT_CLI_PROVIDER_IDS = [
  "claude-code",
  "pi",
  "omp",
  "codex",
  "droid",
  "grok",
  "opencode",
  "cline",
] as const;

const CLAUDE_CODE_MODELS: AIProviderModel[] = [
  {
    id: "",
    name: "Default",
    recommended: true,
    sourceProvider: null,
    cliDefault: true,
  },
  {
    id: "haiku",
    name: "Haiku",
    recommended: false,
    sourceProvider: null,
    cliDefault: false,
  },
  {
    id: "sonnet",
    name: "Sonnet",
    recommended: false,
    sourceProvider: null,
    cliDefault: false,
  },
  {
    id: "opus",
    name: "Opus",
    recommended: false,
    sourceProvider: null,
    cliDefault: false,
  },
];

const AGENT_CLI_DEFAULT_LABEL = "Default";

const fallbackAgentCliDefault = (_providerId: string): AIProviderModel => ({
  id: "",
  name: AGENT_CLI_DEFAULT_LABEL,
  recommended: true,
  cliDefault: true,
});

const isAgentCliProvider = (providerId: string): boolean =>
  (AGENT_CLI_PROVIDER_IDS as readonly string[]).includes(providerId);

const CLI_DEFAULT_MODEL_VALUE = "__voicetypr_cli_default__";

const toModelSelectValue = (modelId: string) =>
  modelId === "" ? CLI_DEFAULT_MODEL_VALUE : modelId;

const fromModelSelectValue = (modelId: string) =>
  modelId === CLI_DEFAULT_MODEL_VALUE ? "" : modelId;

interface ModelPickerItem {
  value: string;
  label: string;
  qualifiedId: string;
  model: AIProviderModel;
}

interface ModelPickerGroup {
  value: string;
  items: ModelPickerItem[];
}
interface AgentModelPickerDialogProps {
  provider: AIProviderConfig;
  groups: ModelPickerGroup[];
  selectedItem: ModelPickerItem | null;
  disabled: boolean;
  loading: boolean;
  onSelect: (modelId: string) => void;
  onOpen: () => void;
}

function AgentModelPickerDialog({
  provider,
  groups,
  selectedItem,
  disabled,
  loading,
  onSelect,
  onOpen,
}: AgentModelPickerDialogProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const normalizedQuery = query.trim().toLowerCase();
  const visibleGroups = useMemo(
    () =>
      groups
        .map((group) => ({
          ...group,
          items: group.items.filter((item) => {
            if (!normalizedQuery) return true;
            return [
              item.label,
              item.qualifiedId,
              item.model.sourceProvider ?? "",
            ]
              .join(" ")
              .toLowerCase()
              .includes(normalizedQuery);
          }),
        }))
        .filter((group) => group.items.length > 0),
    [groups, normalizedQuery],
  );

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        setOpen(nextOpen);
        if (nextOpen) {
          onOpen();
        } else {
          setQuery("");
        }
      }}
    >
      <DialogTrigger
        render={
          <Button
            type="button"
            variant="outline"
            disabled={disabled}
            className="h-9 w-full justify-between gap-3 px-3 sm:w-52"
            aria-label={`Model for ${provider.name}`}
          />
        }
      >
        <span className="min-w-0 truncate text-left">
          {selectedItem?.label ?? AGENT_CLI_DEFAULT_LABEL}
        </span>
        {loading ? (
          <Spinner className="h-3.5 w-3.5 shrink-0" />
        ) : (
          <ChevronDown className="h-3.5 w-3.5 shrink-0 opacity-60" />
        )}
      </DialogTrigger>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Choose a {provider.name} model</DialogTitle>
          <DialogDescription>
            Search models reported by your installed CLI. The selection is saved
            for this agent.
          </DialogDescription>
        </DialogHeader>
        <div className="relative">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            autoFocus
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={`Search ${provider.name} models`}
            aria-label={`Search ${provider.name} models`}
            className="pl-9"
          />
        </div>
        <ScrollArea className="max-h-[min(60vh,28rem)] pr-3">
          <div className="space-y-5 py-1">
            {visibleGroups.length === 0 ? (
              <p className="rounded-lg border border-dashed px-4 py-8 text-center text-sm text-muted-foreground">
                No models match your search.
              </p>
            ) : (
              visibleGroups.map((group) => (
                <section
                  key={`${provider.id}-${group.value}`}
                  className="space-y-2"
                >
                  <h3 className="px-1 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                    {group.value}
                  </h3>
                  <div className="grid gap-1">
                    {group.items.map((item) => {
                      const selected = selectedItem?.value === item.value;
                      const detail = [
                        item.model.reasoning ? "Reasoning" : "",
                        item.model.contextWindow
                          ? `${Math.round(item.model.contextWindow / 1000)}k context`
                          : "",
                      ]
                        .filter(Boolean)
                        .join(" · ");
                      return (
                        <button
                          key={item.value}
                          type="button"
                          aria-pressed={selected}
                          className={`flex min-h-14 w-full items-center gap-3 rounded-lg border px-3 py-2.5 text-left transition-colors ${
                            selected
                              ? "border-sage/50 bg-sage-bg/50"
                              : "border-transparent hover:border-border hover:bg-muted/50"
                          }`}
                          onClick={() => {
                            onSelect(fromModelSelectValue(item.value));
                            setOpen(false);
                          }}
                        >
                          <span className="min-w-0 flex-1">
                            <span className="block truncate text-sm font-medium">
                              {item.label}
                            </span>
                            <span className="mt-0.5 block truncate font-mono text-[11px] text-muted-foreground">
                              {item.qualifiedId}
                              {detail ? ` · ${detail}` : ""}
                            </span>
                          </span>
                          <Check
                            className={`h-4 w-4 shrink-0 text-sage ${
                              selected ? "opacity-100" : "opacity-0"
                            }`}
                          />
                        </button>
                      );
                    })}
                  </div>
                </section>
              ))
            )}
          </div>
        </ScrollArea>
      </DialogContent>
    </Dialog>
  );
}

const qualifyModelId = (providerId: string, modelId: string) =>
  modelId
    ? modelId.includes("/")
      ? modelId
      : `${providerId}/${modelId}`
    : `${providerId}/default`;

const shortModelName = (modelId: string, fallbackName?: string) => {
  if (fallbackName && !fallbackName.includes("/")) {
    return fallbackName;
  }
  const shortId = modelId.includes("/")
    ? modelId.slice(modelId.lastIndexOf("/") + 1)
    : modelId;
  return humanizeModelId(shortId || modelId);
};

const defaultAgentCliReasoning = (providerId: string) =>
  providerId === "pi" || providerId === "omp" ? "off" : "low";

const formatAgentCliReasoning = (providerId: string, reasoning: string) =>
  providerId === "claude-code"
    ? `Effort ${reasoning}`
    : `Thinking ${reasoning}`;

const formatReasoningLevel = (reasoning: string) =>
  reasoning.charAt(0).toUpperCase() + reasoning.slice(1);

const getAgentCliProbeState = (probe?: AgentCliProbe): AgentCliProbeState =>
  probe?.state ?? "missing";

const isAgentCliReady = (probe?: AgentCliProbe): boolean =>
  getAgentCliProbeState(probe) === "ready";

const isDefaultAgentCliModel = (model: AIProviderModel) =>
  Boolean(model.cliDefault) || model.id === "";

export function EnhancementsSection() {
  const readiness = useReadinessState();
  const { settings, updateSettings } = useSettings();
  const {
    fetchModels,
    getModels,
    isLoading: isModelsLoading,
    getError,
    clearModels,
  } = useAllProviderModels();

  const [aiSettings, setAISettings] = useState<AISettings>({
    enabled: false,
    provider: "",
    model: "",
    hasApiKey: false,
    modelsByProvider: {},
    reasoningByProvider: {},
    fastModeByProvider: {},
  });
  const [aiModelNeedsReselection, setAiModelNeedsReselection] = useState(false);

  const [providers, setProviders] = useState<AIProviderConfig[]>([]);

  const [providerSearch, setProviderSearch] = useState("");
  const [providerSetupOpen, setProviderSetupOpen] = useState(false);
  const [providerTab, setProviderTab] = useState<ProviderTab>("cloud");

  const [showApiKeyModal, setShowApiKeyModal] = useState(false);
  const [showOpenAIConfig, setShowOpenAIConfig] = useState(false);
  const [openAIDefaultBaseUrl, setOpenAIDefaultBaseUrl] = useState(
    "https://api.openai.com/v1",
  );
  const [customModelName, setCustomModelName] = useState<string>("");
  const [selectedProvider, setSelectedProvider] = useState<string>("");
  const [guidedSetupProvider, setGuidedSetupProvider] = useState<string | null>(
    null,
  );
  const [isLoading, setIsLoading] = useState(false);
  const [providerApiKeys, setProviderApiKeys] = useState<
    Record<string, boolean>
  >({});
  // Per-provider executable-resolution result for local agent CLIs.
  const [agentCliStatus, setAgentCliStatus] = useState<
    Record<string, AgentCliProbe>
  >({});
  const [agentCliProbing, setAgentCliProbing] = useState<
    Record<string, boolean>
  >({});
  const agentCliProbingRef = useRef<Set<string>>(new Set());
  const settingsLoadStartedRef = useRef(false);
  const [enhancementOptions, setEnhancementOptions] = useState<{
    preset: EnhancementPreset;
  }>({
    preset: "PersonalDictation",
  });
  const [writingSettings, setWritingSettings] = useState<WritingSettings>(
    defaultWritingSettings,
  );
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const polishError = useEnhancementsStore((s) => s.polishError);
  const setPolishError = useEnhancementsStore((s) => s.setPolishError);
  const clearPolishError = useEnhancementsStore((s) => s.clearPolishError);
  const writingSaveGeneration = useRef(0);
  const enhancementSaveGeneration = useRef(0);
  const writingSettingsRef = useRef(writingSettings);
  const writingSaveQueueRef = useRef(Promise.resolve());

  useEffect(() => {
    writingSettingsRef.current = writingSettings;
  }, [writingSettings]);

  const loadEnhancementOptions = async (aiEnabled: boolean) => {
    try {
      const options = await invoke<EnhancementOptions>(
        "get_enhancement_options",
      );
      setEnhancementOptions(fromBackendOptions(options, aiEnabled));
    } catch (error) {
      log.error("Failed to load Polish options:", error);
    }
  };

  const loadWritingSettings = async () => {
    try {
      const nextSettings = await invoke<Partial<WritingSettings>>(
        "get_writing_settings",
      );
      setWritingSettings(mergeWritingSettings(nextSettings));
      return true;
    } catch (error) {
      log.error("Failed to load writing settings:", error);
      return false;
    }
  };
  const probeAgentCli = useCallback(
    async (
      providerId: string,
      refresh: boolean,
    ): Promise<AgentCliProbe | null> => {
      if (agentCliProbingRef.current.has(providerId)) return null;
      agentCliProbingRef.current.add(providerId);
      setAgentCliProbing((prev) => ({ ...prev, [providerId]: true }));
      try {
        const probe = await invoke<AgentCliProbe>("probe_agent_cli", {
          provider: providerId,
          refresh,
        });
        const ready = isAgentCliReady(probe);
        setAgentCliStatus((prev) => ({ ...prev, [providerId]: probe }));
        setProviderApiKeys((prev) => ({ ...prev, [providerId]: ready }));
        return probe;
      } catch (error) {
        log.error(`Failed to probe ${providerId} CLI:`, error);
        return null;
      } finally {
        agentCliProbingRef.current.delete(providerId);
        setAgentCliProbing((prev) => ({ ...prev, [providerId]: false }));
      }
    },
    [],
  );

  const loadAISettings = useCallback(async () => {
    try {
      const listedProviders = (
        await invoke<AiProvider[]>("list_ai_providers")
      ).map(toProviderConfig);
      setProviders(listedProviders);

      const loadedAISettingsResponse =
        await invoke<AISettingsResponse>("get_ai_settings");
      const loadedAISettings = normalizeAISettings(loadedAISettingsResponse);
      const customModel =
        loadedAISettings.modelsByProvider.custom ||
        (loadedAISettings.provider === "custom" ? loadedAISettings.model : "");
      if (customModel) {
        setCustomModelName(customModel);
      }
      setAiModelNeedsReselection(
        Boolean(loadedAISettingsResponse.aiModelNeedsReselection),
      );
      setAISettings(loadedAISettings);

      if (isAgentCliProvider(loadedAISettings.provider)) {
        void probeAgentCli(loadedAISettings.provider, false);
      }

      const keyStatus: Record<string, boolean> = {};
      await Promise.all(
        listedProviders
          .filter((provider) => !isAgentCliProvider(provider.id))
          .map(async ({ id: providerId }) => {
            let isConfigured = await hasApiKey(providerId);

            if (
              (providerId === "custom" || providerId === "openai") &&
              !isConfigured
            ) {
              try {
                const providerSettings = normalizeAISettings(
                  await invoke<AISettingsResponse>(
                    "get_ai_settings_for_provider",
                    {
                      provider: providerId,
                    },
                  ),
                );
                isConfigured = providerSettings.hasApiKey;
              } catch (error) {
                log.error(
                  `Failed to resolve ${providerId} provider readiness:`,
                  error,
                );
              }
            }

            keyStatus[providerId] = isConfigured;
            if (isConfigured) {
              try {
                const apiKey = await getApiKey(providerId);
                if (apiKey) {
                  await invoke("cache_ai_api_key", {
                    args: { provider: providerId, apiKey },
                  });
                }
              } catch (error) {
                log.error(`Failed to cache ${providerId} API key:`, error);
              }
            }
          }),
      );
      setProviderApiKeys((prev) => ({ ...prev, ...keyStatus }));

      if (
        readiness?.ai_ready &&
        loadedAISettings.provider &&
        !isAgentCliProvider(loadedAISettings.provider)
      ) {
        setProviderApiKeys((prev) => ({
          ...prev,
          [loadedAISettings.provider]: true,
        }));
      }

      try {
        const customConfig = await invoke<{ baseUrl: string }>(
          "get_openai_config",
        );
        setOpenAIDefaultBaseUrl(
          customConfig.baseUrl || "https://api.openai.com/v1",
        );
      } catch (error) {
        log.error("Failed to load custom config:", error);
      }

      listedProviders
        .filter(
          (provider) => !provider.isCustom && !isAgentCliProvider(provider.id),
        )
        .forEach((provider) => {
          void fetchModels(provider.id);
        });

      return loadedAISettings;
    } catch (error) {
      log.error("Failed to load AI settings:", error);
      return null;
    }
  }, [readiness?.ai_ready, fetchModels, probeAgentCli]);
  const handleRefreshAgentCli = useCallback(
    async (provider: AIProviderConfig) => {
      const probe = await probeAgentCli(provider.id, true);
      if (!probe) {
        toast.info(
          `${provider.name}: status refresh failed. Your previous selection was kept.`,
        );
        return;
      }

      const state = getAgentCliProbeState(probe);
      if (state === "ready") {
        toast.success(`${provider.name}: installed`);
      } else if (state === "missing") {
        toast.info(
          `${provider.name}: install it in an existing PATH directory, then Refresh. Restart only if PATH itself changed.`,
        );
      } else if (state === "unsafe_launcher") {
        toast.info(
          `${provider.name}: install a compatible launcher in an existing PATH directory, then Refresh.`,
        );
      } else {
        toast.info(`${provider.name}: status unavailable. Try Refresh again.`);
      }
    },
    [probeAgentCli],
  );

  useEffect(() => {
    if (settingsLoaded || settingsLoadStartedRef.current) {
      return;
    }

    settingsLoadStartedRef.current = true;
    (async () => {
      const loadedAISettings = await loadAISettings();
      await loadEnhancementOptions(loadedAISettings?.enabled ?? false);
      const writingSettingsLoaded = await loadWritingSettings();
      setSettingsLoaded(writingSettingsLoaded);
      if (!writingSettingsLoaded) {
        settingsLoadStartedRef.current = false;
      }
    })().catch((error) => {
      settingsLoadStartedRef.current = false;
      log.error("Failed to load Polish settings:", error);
    });
  }, [settingsLoaded, loadAISettings]);

  useEffect(() => {
    const unlistenReady = listen("ai-ready", async () => {
      if (!settingsLoaded) return;
      const loadedAISettings = normalizeAISettings(
        await invoke<AISettingsResponse>("get_ai_settings"),
      );
      setAISettings(loadedAISettings);
      if (
        loadedAISettings.provider &&
        !isAgentCliProvider(loadedAISettings.provider)
      ) {
        setProviderApiKeys((prev) => ({
          ...prev,
          [loadedAISettings.provider]: true,
        }));
      }
    });

    const unlistenApiKey = listen("api-key-saved", async (event) => {
      const loadedAISettings = normalizeAISettings(
        await invoke<AISettingsResponse>("get_ai_settings"),
      );
      const provider = (event.payload as { provider?: string }).provider;
      if (!provider || provider === loadedAISettings.provider) {
        setAISettings(loadedAISettings);
      } else {
        const rememberedModel =
          loadedAISettings.modelsByProvider[provider] || "";
        setAISettings({
          ...loadedAISettings,
          provider,
          enabled: false,
          model: rememberedModel,
          hasApiKey: true,
        });
      }

      if (provider) {
        setProviderApiKeys((prev) => ({ ...prev, [provider]: true }));
      }
    });

    const unlistenApiKeyRemoved = listen<{ provider: string }>(
      "api-key-removed",
      async (event) => {
        let providerStillConfigured = false;

        if (
          event.payload.provider === "custom" ||
          event.payload.provider === "openai"
        ) {
          try {
            const providerSettings = normalizeAISettings(
              await invoke<AISettingsResponse>("get_ai_settings_for_provider", {
                provider: event.payload.provider,
              }),
            );
            providerStillConfigured = providerSettings.hasApiKey;
            setProviderApiKeys((prev) => ({
              ...prev,
              [event.payload.provider]: providerStillConfigured,
            }));
          } catch (error) {
            log.error(
              `Failed to refresh ${event.payload.provider} provider readiness after key removal:`,
              error,
            );
            setProviderApiKeys((prev) => ({
              ...prev,
              [event.payload.provider]: false,
            }));
          }
        } else {
          setProviderApiKeys((prev) => ({
            ...prev,
            [event.payload.provider]: false,
          }));
        }

        clearModels(event.payload.provider);

        const isCurrentProviderRemoved =
          aiSettings.provider === event.payload.provider &&
          !providerStillConfigured;

        if (isCurrentProviderRemoved) {
          setAISettings((prev) => ({
            ...prev,
            enabled: false,
            provider: "",
            model: "",
            hasApiKey: false,
          }));

          await invoke("update_ai_settings", {
            enabled: false,
            provider: "",
            model: "",
          });
          setEnhancementOptions({ preset: "PersonalDictation" });
          try {
            await updateSettings({
              final_text_language: "same_as_transcript",
              transcription_task: "transcribe",
            });
          } catch (error) {
            log.error(
              "Failed to refresh language settings after API key removal:",
              error,
            );
          }
        }
      },
    );

    const unlistenAiEnabledChanged = listen<boolean>(
      "ai-enabled-changed",
      (event) => {
        setAISettings((prev) => ({ ...prev, enabled: event.payload }));
      },
    );

    return () => {
      Promise.all([
        unlistenReady,
        unlistenApiKey,
        unlistenApiKeyRemoved,
        unlistenAiEnabledChanged,
      ]).then((fns) => {
        fns.forEach((fn) => fn());
      });
    };
  }, [settingsLoaded, aiSettings.provider, clearModels, updateSettings]);

  // Surface persistent Polish failures inline on the Polish card (toasts are
  // left to EnhancementsTab); a successful polish run clears the banner.
  useEffect(() => {
    const unlistenAuthError = listen("ai-enhancement-auth-error", (event) => {
      if (typeof event.payload === "string") {
        setPolishError("auth", event.payload);
      }
    });

    const unlistenEnhancementError = listen(
      "ai-enhancement-error",
      (event) => {
        if (typeof event.payload === "string") {
          setPolishError("generic", event.payload);
        }
      },
    );

    const unlistenEnhancementFailed = listen<{
      category?: string;
      message?: string;
    } | null>("enhancing-failed", (event) => {
      const payload = event.payload;
      if (!payload || payload.category === "canceled") return;
      const kind: PolishErrorKind =
        payload.category === "missing_api_key" ||
        payload.category === "invalid_api_key"
          ? "auth"
          : "generic";
      setPolishError(kind, payload.message || "Polish failed");
    });

    const unlistenEnhancementCompleted = listen(
      "enhancing-completed",
      () => {
        clearPolishError();
      },
    );

    return () => {
      Promise.all([
        unlistenAuthError,
        unlistenEnhancementError,
        unlistenEnhancementFailed,
        unlistenEnhancementCompleted,
      ]).then((fns) => {
        fns.forEach((fn) => fn());
      });
    };
  }, [setPolishError, clearPolishError]);

  const persistEnhancementOptions = async (nextOptions: {
    preset: EnhancementPreset;
  }) => {
    const rollbackOptions = enhancementOptions;
    const generationAtEnqueue = enhancementSaveGeneration.current + 1;
    enhancementSaveGeneration.current = generationAtEnqueue;
    setEnhancementOptions(nextOptions);
    try {
      await invoke("update_enhancement_options", {
        options: toBackendOptions(nextOptions),
      });
    } catch (error) {
      if (enhancementSaveGeneration.current === generationAtEnqueue) {
        setEnhancementOptions(rollbackOptions);
      }
      const message = getErrorMessage(error, "Failed to save Polish settings");
      toast.error(message);
    }
  };

  const enqueueWritingSettingsSave = (
    settingsToSave: WritingSettings,
    rollbackSettings: WritingSettings,
    generationAtEnqueue: number,
  ) => {
    writingSaveQueueRef.current = writingSaveQueueRef.current.then(async () => {
      try {
        await invoke("update_writing_settings", { settings: settingsToSave });
      } catch (error) {
        if (writingSaveGeneration.current === generationAtEnqueue) {
          setWritingSettings(rollbackSettings);
          writingSettingsRef.current = rollbackSettings;
          const message = getErrorMessage(
            error,
            "Failed to save writing settings",
          );
          toast.error(message);
        }
      }
    });
  };

  const handleWritingSettingsChange = (nextSettings: WritingSettings) => {
    const rollbackSettings = writingSettingsRef.current;
    const generationAtEnqueue = writingSaveGeneration.current + 1;
    writingSaveGeneration.current = generationAtEnqueue;
    setWritingSettings(nextSettings);
    writingSettingsRef.current = nextSettings;
    if (settingsLoaded) {
      enqueueWritingSettingsSave(
        nextSettings,
        rollbackSettings,
        generationAtEnqueue,
      );
    }
  };

  const handleFinalTextLanguageChange = async (value: string) => {
    if (!settings) return;
    const nextTask = value === "en" ? "translate_to_english" : "transcribe";
    try {
      await updateSettings({
        final_text_language: value,
        transcription_task: nextTask,
      });
    } catch (error) {
      const message = getErrorMessage(
        error,
        "Failed to save final text language",
      );
      toast.error(message);
    }
  };

  const getDisplayModels = useCallback(
    (providerId: string): AIProviderModel[] => {
      const models = getModels(providerId);
      if (isAgentCliReady(agentCliStatus[providerId]) && models.length === 0) {
        if (providerId === "claude-code") {
          return CLAUDE_CODE_MODELS;
        }
        if (isAgentCliProvider(providerId)) {
          return [fallbackAgentCliDefault(providerId)];
        }
      }
      return models;
    },
    [agentCliStatus, getModels],
  );

  const resolveRecommendedModel = async (providerId: string) => {
    const cachedModels = getDisplayModels(providerId);
    let models = cachedModels;

    if (!cachedModels.some((model) => model.recommended)) {
      const fetchedModels = await fetchModels(providerId);
      models =
        fetchedModels?.length > 0
          ? fetchedModels
          : getDisplayModels(providerId);
    }
    if (isAgentCliProvider(providerId)) {
      return (
        models.find((model) => model.cliDefault) ??
        fallbackAgentCliDefault(providerId)
      );
    }

    return models.find((model) => model.recommended) ?? null;
  };

  const enablePolishForProviderModel = async (
    providerId: string,
    modelId: string,
    modelsByProvider: Record<string, string>,
  ) => {
    await invoke("update_ai_settings", {
      enabled: true,
      provider: providerId,
      model: modelId,
    });

    setAISettings((prev) => {
      const nextModelsByProvider = {
        ...prev.modelsByProvider,
        ...modelsByProvider,
      };
      if (modelId) {
        nextModelsByProvider[providerId] = modelId;
      } else {
        delete nextModelsByProvider[providerId];
      }

      return {
        ...prev,
        enabled: true,
        provider: providerId,
        model: modelId,
        hasApiKey: true,
        modelsByProvider: nextModelsByProvider,
      };
    });
    setProviderApiKeys((prev) => ({ ...prev, [providerId]: true }));
    setAiModelNeedsReselection(false);

    if (enhancementOptions.preset !== "CleanDictation") {
      await persistEnhancementOptions({ preset: "CleanDictation" });
    }
  };

  const enableGuidedProvider = async (
    providerId: string,
    modelsByProvider: Record<string, string>,
  ) => {
    const recommendedModel = await resolveRecommendedModel(providerId);
    if (!recommendedModel) {
      setProviderSetupOpen(true);
      toast.error("No recommended model was available. Choose a model below.");
      return false;
    }

    await enablePolishForProviderModel(
      providerId,
      recommendedModel.id,
      modelsByProvider,
    );
    toast.success("Polish on");
    return true;
  };

  const handleToggleEnabled = async (enabled: boolean) => {
    const hasActiveProviderKey = Boolean(providerApiKeys[aiSettings.provider]);

    if (
      enabled &&
      (!hasActiveProviderKey ||
        (!aiSettings.model && !isAgentCliProvider(aiSettings.provider)))
    ) {
      toast.error("Polish is not set up yet. Connect an AI to turn it on.");
      return;
    }

    try {
      await invoke("update_ai_settings", {
        enabled,
        provider: aiSettings.provider,
        model: aiSettings.model,
      });

      setAISettings((prev) => ({ ...prev, enabled }));

      const nextPreset: EnhancementPreset = enabled
        ? "CleanDictation"
        : "PersonalDictation";

      if (
        nextPreset === "PersonalDictation" &&
        settings?.final_text_language &&
        settings.final_text_language !== "same_as_transcript"
      ) {
        await handleFinalTextLanguageChange("same_as_transcript");
      }

      const presetChanged = nextPreset !== enhancementOptions.preset;
      if (presetChanged) {
        await persistEnhancementOptions({ preset: nextPreset });
      }

      toast.success(enabled ? "Polish on" : "Polish off");
    } catch (error) {
      const message = getErrorMessage(error, "Failed to update Polish");
      toast.error(message);
    }
  };

  const handleSetupApiKey = async (providerId: string) => {
    setSelectedProvider(providerId);

    if (providerId === "custom") {
      try {
        const [savedConfig, providerSettingsResponse] = await Promise.all([
          invoke<{ baseUrl: string }>("get_openai_config"),
          invoke<AISettingsResponse>("get_ai_settings_for_provider", {
            provider: providerId,
          }),
        ]);
        const providerSettings = normalizeAISettings(providerSettingsResponse);
        setOpenAIDefaultBaseUrl(
          savedConfig.baseUrl || "https://api.openai.com/v1",
        );
        if (providerSettings.model) {
          setCustomModelName(providerSettings.model);
        }
      } catch (error) {
        log.error("Failed to load custom config:", error);
      }
      setShowOpenAIConfig(true);
    } else {
      setShowApiKeyModal(true);
    }
  };

  const handleApiKeySubmit = async (apiKey: string) => {
    setIsLoading(true);
    try {
      const trimmedKey = apiKey.trim();
      await saveApiKey(selectedProvider, trimmedKey);
      const providerSettings = normalizeAISettings(
        await invoke<AISettingsResponse>("get_ai_settings_for_provider", {
          provider: selectedProvider,
        }),
      );
      const rememberedModel = providerSettings.model || "";
      const modelsByProvider = {
        ...providerSettings.modelsByProvider,
        ...(rememberedModel ? { [selectedProvider]: rememberedModel } : {}),
      };
      const shouldAutoEnable = guidedSetupProvider === selectedProvider;

      setProviderApiKeys((prev) => ({ ...prev, [selectedProvider]: true }));

      if (shouldAutoEnable) {
        const didEnable = await enableGuidedProvider(
          selectedProvider,
          modelsByProvider,
        );
        if (!didEnable) {
          setAISettings((prev) => ({
            ...prev,
            provider: selectedProvider,
            enabled: false,
            model: rememberedModel,
            hasApiKey: true,
            modelsByProvider: {
              ...prev.modelsByProvider,
              ...modelsByProvider,
            },
          }));
        }
      } else {
        setAISettings((prev) => ({
          ...prev,
          provider: selectedProvider,
          enabled: prev.provider === selectedProvider ? prev.enabled : false,
          model: rememberedModel,
          hasApiKey: true,
          modelsByProvider: {
            ...prev.modelsByProvider,
            ...modelsByProvider,
          },
        }));
        toast.success("API key saved securely");
      }

      setShowApiKeyModal(false);
      setGuidedSetupProvider(null);
    } catch (error) {
      const message = getErrorMessage(error, "Failed to save API key");
      toast.error(message);
    } finally {
      setIsLoading(false);
    }
  };

  const handleRemoveApiKey = async (providerId: string) => {
    try {
      await removeApiKey(providerId);
      clearModels(providerId);
      toast.success("API key removed");
    } catch (error) {
      const message = getErrorMessage(error, "Failed to remove API key");
      toast.error(message);
    }
  };

  const handleSelectModel = async (providerId: string, modelId: string) => {
    try {
      const hasKey = providerApiKeys[providerId];
      const shouldEnable = hasKey ? aiSettings.enabled : false;

      await invoke("update_ai_settings", {
        enabled: shouldEnable,
        provider: providerId,
        model: modelId,
      });

      setAISettings((prev) => {
        const nextModelsByProvider = { ...prev.modelsByProvider };
        if (modelId) {
          nextModelsByProvider[providerId] = modelId;
        } else {
          delete nextModelsByProvider[providerId];
        }

        return {
          ...prev,
          enabled: shouldEnable,
          provider: providerId,
          model: modelId,
          hasApiKey: hasKey,
          modelsByProvider: nextModelsByProvider,
        };
      });
      setAiModelNeedsReselection(false);
      setProviderSetupOpen(true);
      if (shouldEnable) {
        await loadEnhancementOptions(true);
      }

      toast.success("Model selected");
    } catch (error) {
      const message = getErrorMessage(error, "Failed to select model");
      toast.error(message);
    }
  };

  const handleSelectReasoning = async (
    providerId: string,
    reasoning: string,
  ) => {
    try {
      await invoke("update_agent_cli_reasoning", {
        provider: providerId,
        reasoning,
      });
      setAISettings((prev) => ({
        ...prev,
        reasoningByProvider: {
          ...prev.reasoningByProvider,
          [providerId]: reasoning,
        },
      }));
      toast.success("Thinking setting saved");
    } catch (error) {
      toast.error(getErrorMessage(error, "Failed to save thinking setting"));
    }
  };

  const handleToggleFastMode = async (
    providerId: string,
    enabled: boolean,
  ) => {
    try {
      await invoke("update_agent_cli_fast_mode", {
        provider: providerId,
        enabled,
      });
      setAISettings((prev) => ({
        ...prev,
        fastModeByProvider: {
          ...prev.fastModeByProvider,
          [providerId]: enabled,
        },
      }));
      toast.success(`Fast mode ${enabled ? "on" : "off"}`);
    } catch (error) {
      toast.error(getErrorMessage(error, "Failed to save fast mode"));
    }
  };

  const isUsingCustomProvider = aiSettings.provider === "custom";
  const hasSelectedModel = Boolean(
    aiSettings.provider &&
    // Agent-CLI providers carry no model — waive the model requirement
    // (availability still requires a probed provider key below).
    (aiSettings.model || isAgentCliProvider(aiSettings.provider)) &&
    (isUsingCustomProvider || providerApiKeys[aiSettings.provider]),
  );

  const showAiModelReselectionNotice =
    aiModelNeedsReselection && aiSettings.enabled && !aiSettings.model;

  const storedFinalTextLanguage =
    settings?.final_text_language ?? "same_as_transcript";
  const effectiveFinalTextLanguage =
    enhancementOptions.preset === "PersonalDictation"
      ? "same_as_transcript"
      : storedFinalTextLanguage;

  const selectedDisplayModel = getDisplayModels(aiSettings.provider).find(
    (model) => model.id === aiSettings.model,
  );
  const activeModelName = isUsingCustomProvider
    ? customModelName
    : !aiSettings.model && isAgentCliProvider(aiSettings.provider)
      ? AGENT_CLI_DEFAULT_LABEL
      : shortModelName(aiSettings.model, selectedDisplayModel?.name);
  const activeProviderName =
    providers.find((provider) => provider.id === aiSettings.provider)?.name ||
    aiSettings.provider;
  const activeReasoningLevels =
    agentCliStatus[aiSettings.provider]?.reasoningLevels ?? [];
  const activeReasoningName =
    isAgentCliProvider(aiSettings.provider) && activeReasoningLevels.length > 0
      ? formatAgentCliReasoning(
          aiSettings.provider,
          aiSettings.reasoningByProvider[aiSettings.provider] ??
            defaultAgentCliReasoning(aiSettings.provider),
        )
      : "";

  const providerQuery = providerSearch.trim().toLowerCase();
  const filteredProviders = useMemo(
    () =>
      providers.filter((provider) => {
        const matchesTab =
          providerTab === "local"
            ? isAgentCliProvider(provider.id)
            : !isAgentCliProvider(provider.id);
        if (!matchesTab) return false;
        if (providerTab === "local" || !providerQuery) return true;

        return (
          provider.name.toLowerCase().includes(providerQuery) ||
          (provider.isCustom &&
            customModelName.toLowerCase().includes(providerQuery)) ||
          getDisplayModels(provider.id).some((model) =>
            modelMatchesQuery(model, providerQuery),
          )
        );
      }),
    [customModelName, getDisplayModels, providerQuery, providerTab, providers],
  );

  const openProviderSetup = () => {
    setProviderTab(isAgentCliProvider(aiSettings.provider) ? "local" : "cloud");
    setProviderSearch("");
    setProviderSetupOpen(true);
  };

  const hasLoadingProviders = providers.some((provider) =>
    isModelsLoading(provider.id),
  );
  const showGuidedSetup = settingsLoaded && !hasSelectedModel;
  const polishHeaderActions = hasSelectedModel ? (
    <div className="flex items-center gap-3 rounded-lg border border-border/60 bg-card px-3 py-1.5">
      <div className="min-w-0 max-w-56 flex-1 text-right">
        <p className="truncate text-xs font-medium text-foreground">
          {activeProviderName}
        </p>
        <p className="truncate text-[11px] text-muted-foreground">
          {activeModelName || "Connected"}
        </p>
      </div>
      <Switch
        id="polish-enabled"
        aria-label="Polish"
        className="shrink-0"
        checked={aiSettings.enabled}
        onCheckedChange={handleToggleEnabled}
      />
    </div>
  ) : (
    <Button type="button" variant="outline" onClick={openProviderSetup}>
      Select a provider to enable Polish
    </Button>
  );

  const polishSetupExpanded = showGuidedSetup || providerSetupOpen;

  useEffect(() => {
    if (!polishSetupExpanded || providerTab !== "local") return;

    providers
      .filter((provider) => isAgentCliProvider(provider.id))
      .forEach((provider) => {
        if (!agentCliStatus[provider.id]) {
          void probeAgentCli(provider.id, false);
        }
      });
  }, [
    agentCliStatus,
    polishSetupExpanded,
    probeAgentCli,
    providerTab,
    providers,
  ]);
  const polishSetupContent = (
    <FieldSet className="overflow-hidden rounded-xl border border-border/60 bg-card [&_button:not(:disabled)]:cursor-pointer">
      {polishError && (
        <div className="border-b border-border/60 p-4">
          <div className="flex items-start gap-2 rounded-lg border border-amber-500/20 bg-amber-500/10 p-3">
            <AlertTriangle className="mt-0.5 h-4 w-4 flex-shrink-0 text-amber-500" />
            <p className="flex-1 text-sm font-medium text-amber-700 dark:text-amber-400">
              {polishError.kind === "auth"
                ? "Polish failed — your API key was rejected. Update it below."
                : polishError.message}
            </p>
            <button
              type="button"
              onClick={clearPolishError}
              aria-label="Dismiss Polish error"
              className="-m-1 shrink-0 rounded-md p-1 text-amber-600 transition-colors hover:bg-amber-500/10 hover:text-amber-700 focus-visible:ring-2 focus-visible:ring-ring dark:text-amber-400 dark:hover:text-amber-300"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </div>
      )}
      <button
        type="button"
        className="flex w-full items-center justify-between gap-4 px-4 py-3 text-left outline-none transition-colors hover:bg-muted/30 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
        aria-expanded={polishSetupExpanded}
        aria-label={
          polishSetupExpanded
            ? "Collapse provider and model"
            : "Expand provider and model"
        }
        onClick={() => {
          if (showGuidedSetup) return;
          if (polishSetupExpanded) {
            setProviderSetupOpen(false);
          } else {
            openProviderSetup();
          }
        }}
      >
        <span className="min-w-0">
          <span className="block text-sm font-semibold">
            {showGuidedSetup
              ? "Connect an AI to turn on Polish"
              : "Provider & model"}
          </span>
          <span className="mt-0.5 block truncate text-xs text-muted-foreground">
            {hasSelectedModel
              ? `${activeProviderName}${activeModelName ? ` · ${activeModelName}` : ""}${activeReasoningName ? ` · ${activeReasoningName}` : ""}`
              : "Choose a cloud API or local agent"}
          </span>
        </span>
        <span className="flex shrink-0 items-center gap-2">
          {hasSelectedModel && (
            <Badge variant="secondary" className="text-sage">
              Active
            </Badge>
          )}
          <ChevronDown
            className={`size-4 text-muted-foreground transition-transform ${
              polishSetupExpanded ? "rotate-180" : ""
            }`}
          />
        </span>
      </button>

      {polishSetupExpanded && (
        <div className="border-t border-border/60 px-4 py-4">
          <FieldDescription className="mb-4">
            Use your own cloud API key or an agent already installed on this
            Mac.
          </FieldDescription>

          {showAiModelReselectionNotice && (
            <div className="mb-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-300">
              Your previous model is unavailable. Choose another model to
              continue.
            </div>
          )}

          <Tabs
            value={providerTab}
            onValueChange={(value) => {
              setProviderTab(value as ProviderTab);
              setProviderSearch("");
            }}
          >
            <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <TabsList aria-label="Polish provider type">
                <TabsTrigger value="cloud">Cloud API</TabsTrigger>
                <TabsTrigger value="local">Local Agents</TabsTrigger>
              </TabsList>
              {providerTab === "cloud" && (
                <div className="relative min-w-0 flex-1 sm:max-w-72">
                  <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                  <Input
                    id="ai-provider-model-search"
                    aria-label="Search cloud providers and models"
                    value={providerSearch}
                    onChange={(event) => setProviderSearch(event.target.value)}
                    placeholder="Search cloud APIs"
                    className="pl-9"
                  />
                </div>
              )}
            </div>

            <TabsContent value={providerTab} className="mt-3 space-y-2">
              {providerTab === "cloud" && hasLoadingProviders && (
                <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <Spinner className="h-3.5 w-3.5" />
                  Refreshing models
                </p>
              )}
              {filteredProviders.length === 0 && (
                <div className="rounded-lg border border-dashed border-border/70 px-4 py-6 text-center text-sm text-muted-foreground">
                  No providers or models match your search.
                </div>
              )}

              {filteredProviders.map((provider) => {
                const agentCli = isAgentCliProvider(provider.id);
                const agentProbe = agentCli
                  ? agentCliStatus[provider.id]
                  : undefined;
                const agentCliState = agentProbe
                  ? getAgentCliProbeState(agentProbe)
                  : null;
                const agentCliChecking =
                  agentCli && !agentProbe && agentCliProbing[provider.id];
                const agentCliRefreshing =
                  agentCli &&
                  Boolean(agentProbe) &&
                  agentCliProbing[provider.id];
                const agentCliReady = agentCliState === "ready";
                const hasKey = agentCli
                  ? agentCliReady
                  : Boolean(providerApiKeys[provider.id]);
                const isSelected = aiSettings.provider === provider.id;
                const hasSelectedProviderModel =
                  Object.prototype.hasOwnProperty.call(
                    aiSettings.modelsByProvider,
                    provider.id,
                  ) || isSelected;
                const selectedModel = provider.isCustom
                  ? aiSettings.modelsByProvider.custom || customModelName
                  : Object.prototype.hasOwnProperty.call(
                        aiSettings.modelsByProvider,
                        provider.id,
                      )
                    ? aiSettings.modelsByProvider[provider.id]
                    : aiSettings.provider === provider.id
                      ? aiSettings.model
                      : "";
                const discoveredModels = getDisplayModels(provider.id);
                const defaultAgentCliModel = agentCli
                  ? (discoveredModels.find(isDefaultAgentCliModel) ??
                    fallbackAgentCliDefault(provider.id))
                  : undefined;
                const models =
                  defaultAgentCliModel &&
                  !discoveredModels.some(isDefaultAgentCliModel)
                    ? [defaultAgentCliModel, ...discoveredModels]
                    : discoveredModels;
                const modelSelectValue = hasSelectedProviderModel
                  ? toModelSelectValue(selectedModel)
                  : defaultAgentCliModel
                    ? CLI_DEFAULT_MODEL_VALUE
                    : undefined;
                const providerMatches = provider.name
                  .toLowerCase()
                  .includes(providerQuery);
                const displayModels =
                  providerQuery && !providerMatches
                    ? models.filter((model) =>
                        modelMatchesQuery(model, providerQuery),
                      )
                    : models;
                const toPickerItem = (
                  model: AIProviderModel,
                ): ModelPickerItem => ({
                  value: toModelSelectValue(model.id),
                  label: isDefaultAgentCliModel(model)
                    ? AGENT_CLI_DEFAULT_LABEL
                    : shortModelName(model.id, model.name),
                  qualifiedId: qualifyModelId(provider.id, model.id),
                  model,
                });
                const modelPickerGroups: ModelPickerGroup[] = [
                  {
                    value: "Recommended",
                    items: displayModels
                      .filter((model) => model.recommended)
                      .map(toPickerItem),
                  },
                  {
                    value: "All models",
                    items: displayModels
                      .filter((model) => !model.recommended)
                      .map(toPickerItem),
                  },
                ].filter((group) => group.items.length > 0);
                const modelPickerItems = modelPickerGroups.flatMap(
                  (group) => group.items,
                );
                const selectedModelItem =
                  modelPickerItems.find(
                    (item) => item.value === modelSelectValue,
                  ) ??
                  (selectedModel
                    ? toPickerItem({
                        id: selectedModel,
                        name: humanizeModelId(selectedModel),
                        recommended: false,
                      })
                    : null);
                const statusCopy = agentCli
                  ? agentCliChecking
                    ? "Checking installation…"
                    : agentCliRefreshing
                      ? agentCliReady
                        ? "Installed · Refreshing…"
                        : "Refreshing installation…"
                      : agentCliReady
                        ? "Installed"
                        : agentCliState === "missing"
                            ? "Not installed in PATH"
                            : "Installation status unavailable"
                  : hasKey
                    ? isSelected
                      ? null
                      : "Ready"
                    : provider.isCustom
                      ? "Endpoint not configured"
                      : "API key required";
                const reasoning =
                  aiSettings.reasoningByProvider[provider.id] ??
                  defaultAgentCliReasoning(provider.id);
                const reasoningLevels = agentProbe?.reasoningLevels ?? [];
                const fastModeEnabled =
                  aiSettings.fastModeByProvider[provider.id] ?? false;
                const providerSummary = (
                  <div className="flex flex-wrap items-center gap-2">
                    <h3
                      className={`pointer-events-none text-sm font-semibold ${provider.color}`}
                    >
                      {provider.name}
                    </h3>
                    {isSelected && (
                      <Badge variant="secondary" className="text-sage">
                        Active
                      </Badge>
                    )}
                    {provider.status === "experimental" && (
                      <Badge variant="outline">Experimental</Badge>
                    )}
                    {statusCopy && (
                      <span className="text-xs text-muted-foreground">
                        {statusCopy}
                      </span>
                    )}
                  </div>
                );

                return (
                  <div
                    key={provider.id}
                    role={agentCli && hasKey ? "button" : undefined}
                    tabIndex={agentCli && hasKey ? 0 : undefined}
                    aria-pressed={agentCli && hasKey ? isSelected : undefined}
                    aria-label={
                      agentCli && hasKey
                        ? `Select ${provider.name}`
                        : undefined
                    }
                    className={`rounded-lg border p-3 transition-colors select-none ${
                      agentCli && hasKey
                        ? "cursor-pointer hover:border-sage/40 hover:bg-muted/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        : ""
                    } ${
                      isSelected
                        ? "border-sage/50 bg-sage-bg/40"
                        : "border-border/60 bg-background"
                    }`}
                    onClick={(event) => {
                      if (!agentCli || !hasKey) return;
                      if (
                        event.target instanceof Element &&
                        event.target.closest(
                          "button, input, [role='combobox'], [role='listbox'], [role='option'], [role='switch'], [role='dialog']",
                        )
                      ) {
                        return;
                      }
                      void handleSelectModel(provider.id, selectedModel);
                    }}
                    onKeyDown={(event) => {
                      if (!agentCli || !hasKey) return;
                      if (event.key !== "Enter" && event.key !== " ") return;
                      if (
                        event.target instanceof Element &&
                        event.target !== event.currentTarget
                      ) {
                        return;
                      }
                      event.preventDefault();
                      void handleSelectModel(provider.id, selectedModel);
                    }}
                  >
                    <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
                      <div className="min-w-0 flex-1">{providerSummary}</div>

                      <div className="flex min-w-0 flex-col gap-2 sm:flex-row sm:flex-nowrap sm:items-center sm:justify-end">
                        {agentCli ? (
                          <>
                              <AgentModelPickerDialog
                                provider={provider}
                                groups={modelPickerGroups}
                                selectedItem={selectedModelItem}
                                disabled={!agentCliReady}
                                loading={isModelsLoading(provider.id)}
                                onOpen={() => {
                                  if (getModels(provider.id).length === 0) {
                                    void fetchModels(provider.id);
                                  }
                                }}
                                onSelect={(modelId) =>
                                  void handleSelectModel(provider.id, modelId)
                                }
                              />
                              {reasoningLevels.length > 0 && (
                                <Select
                                  value={reasoning}
                                  disabled={!agentCliReady}
                                  onValueChange={(value) =>
                                    value != null &&
                                    void handleSelectReasoning(
                                      provider.id,
                                      value,
                                    )
                                  }
                                >
                                  <SelectTrigger
                                    className="h-9 w-full justify-between gap-2 px-2.5 sm:w-24"
                                    aria-label={`${provider.id === "claude-code" ? "Effort" : "Thinking"} for ${provider.name}`}
                                  >
                                    <SelectValue>
                                      <span className="inline-flex items-center gap-1.5">
                                        <Brain className="h-3.5 w-3.5 shrink-0" />
                                        {formatReasoningLevel(reasoning)}
                                      </span>
                                    </SelectValue>
                                  </SelectTrigger>
                                  <SelectContent>
                                    {reasoningLevels.map((level) => (
                                      <SelectItem key={level} value={level}>
                                        <span className="inline-flex items-center gap-1.5">
                                          <Brain className="h-3.5 w-3.5 shrink-0" />
                                          {formatReasoningLevel(level)}
                                        </span>
                                      </SelectItem>
                                    ))}
                                  </SelectContent>
                                </Select>
                              )}
                              {agentProbe?.supportsFastMode && (
                                <label
                                  className="flex h-9 items-center gap-2 rounded-md border border-input bg-background px-3 text-xs font-medium"
                                  title="Use this CLI's native fast service mode. Provider pricing may be higher."
                                >
                                  Fast
                                  <Switch
                                    aria-label={`Fast mode for ${provider.name}`}
                                    checked={fastModeEnabled}
                                    disabled={!agentCliReady}
                                    onCheckedChange={(enabled) =>
                                      void handleToggleFastMode(
                                        provider.id,
                                        enabled,
                                      )
                                    }
                                  />
                                </label>
                              )}
                            <Button
                              type="button"
                              variant="outline"
                              size="icon-sm"
                              aria-label={`Refresh ${provider.name} status`}
                              title={`Refresh ${provider.name} status`}
                              onClick={() =>
                                void handleRefreshAgentCli(provider)
                              }
                              disabled={agentCliProbing[provider.id]}
                            >
                              <RefreshCw
                                className={`h-3.5 w-3.5 ${
                                  agentCliProbing[provider.id]
                                    ? "animate-spin"
                                    : ""
                                }`}
                              />
                            </Button>
                          </>
                        ) : (
                          <>
                            {!provider.isCustom && hasKey && (
                              <Combobox<ModelPickerItem>
                                items={modelPickerGroups}
                                value={selectedModelItem}
                                onValueChange={(item) => {
                                  if (item) {
                                    void handleSelectModel(
                                      provider.id,
                                      fromModelSelectValue(item.value),
                                    );
                                  }
                                }}
                                itemToStringLabel={(item) => item.label}
                                itemToStringValue={(item) => item.value}
                                isItemEqualToValue={(item, value) =>
                                  item.value === value.value
                                }
                                filter={(item, query) => {
                                  const searchable = [
                                    item.label,
                                    item.qualifiedId,
                                    item.model.sourceProvider ?? "",
                                  ]
                                    .join(" ")
                                    .toLowerCase();
                                  return searchable.includes(
                                    query.trim().toLowerCase(),
                                  );
                                }}
                                autoHighlight
                              >
                                <ComboboxInput
                                  className="w-full sm:w-64"
                                  placeholder="Search models"
                                  aria-label={`Model for ${provider.name}`}
                                />
                                <ComboboxContent className="min-w-80">
                                  <ComboboxEmpty>
                                    No models found.
                                  </ComboboxEmpty>
                                  <ComboboxList>
                                    {(group: ModelPickerGroup) => (
                                      <ComboboxGroup
                                        key={`${provider.id}-${group.value}`}
                                        items={group.items}
                                      >
                                        <ComboboxLabel>
                                          {group.value}
                                        </ComboboxLabel>
                                        <ComboboxCollection>
                                          {(item: ModelPickerItem) => {
                                            const cost = formatModelCost(
                                              item.model,
                                            );
                                            const detail = [
                                              item.model.reasoning ||
                                              providerSupportsReasoning(
                                                provider,
                                              )
                                                ? "reasoning"
                                                : "",
                                              cost,
                                            ]
                                              .filter(Boolean)
                                              .join(" · ");
                                            return (
                                              <ComboboxItem
                                                key={item.value}
                                                value={item}
                                              >
                                                <span className="flex min-w-0 flex-1 items-center justify-between gap-3">
                                                  <span className="min-w-0 truncate">
                                                    {item.label}
                                                    {detail
                                                      ? ` · ${detail}`
                                                      : ""}
                                                  </span>
                                                  <span className="max-w-40 shrink-0 truncate font-mono text-[11px] text-muted-foreground">
                                                    {item.qualifiedId}
                                                  </span>
                                                </span>
                                              </ComboboxItem>
                                            );
                                          }}
                                        </ComboboxCollection>
                                      </ComboboxGroup>
                                    )}
                                  </ComboboxList>
                                </ComboboxContent>
                              </Combobox>
                            )}
                            {hasKey ? (
                              <>
                                <Button
                                  type="button"
                                  variant="outline"
                                  size="sm"
                                  onClick={() =>
                                    provider.isCustom
                                      ? void handleSetupApiKey(provider.id)
                                      : void fetchModels(provider.id)
                                  }
                                  disabled={
                                    !provider.isCustom &&
                                    isModelsLoading(provider.id)
                                  }
                                >
                                  {provider.isCustom ? (
                                    <Settings2 className="h-3.5 w-3.5" />
                                  ) : (
                                    <RefreshCw className="h-3.5 w-3.5" />
                                  )}
                                  {provider.isCustom ? "Configure" : "Refresh"}
                                </Button>
                                <Button
                                  type="button"
                                  variant="ghost"
                                  size="sm"
                                  className="text-muted-foreground hover:text-destructive"
                                  aria-label={`Remove ${provider.name} configuration`}
                                  onClick={async () => {
                                    const confirmed = await ask(
                                      provider.isCustom
                                        ? `Remove configuration for ${provider.name}?`
                                        : `Remove API key for ${provider.name}?`,
                                      {
                                        title: provider.isCustom
                                          ? "Remove Configuration"
                                          : "Remove API Key",
                                        kind: "warning",
                                      },
                                    );
                                    if (confirmed)
                                      void handleRemoveApiKey(provider.id);
                                  }}
                                >
                                  <Trash2 className="h-3.5 w-3.5" />
                                </Button>
                              </>
                            ) : (
                              <>
                                {!provider.isCustom && provider.apiKeyUrl && (
                                  <Button
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    onClick={() =>
                                      window.open(provider.apiKeyUrl, "_blank")
                                    }
                                  >
                                    <ExternalLink className="h-3.5 w-3.5" />
                                    Get key
                                  </Button>
                                )}
                                <Button
                                  type="button"
                                  variant="outline"
                                  size="sm"
                                  aria-label={
                                    provider.isCustom
                                      ? `Configure ${provider.name}`
                                      : `Add ${provider.name} API key`
                                  }
                                  onClick={() => {
                                    if (showGuidedSetup)
                                      setGuidedSetupProvider(provider.id);
                                    void handleSetupApiKey(provider.id);
                                  }}
                                >
                                  {provider.isCustom ? (
                                    <Settings2 className="h-3.5 w-3.5" />
                                  ) : (
                                    <Key className="h-3.5 w-3.5" />
                                  )}
                                  {provider.isCustom ? "Configure" : "Add key"}
                                </Button>
                              </>
                            )}
                          </>
                        )}
                      </div>
                    </div>

                    {getError(provider.id) && (
                      <p className="mt-2 text-xs text-destructive">
                        {getError(provider.id)}
                      </p>
                    )}
                  </div>
                );
              })}
            </TabsContent>
          </Tabs>

          <p className="mt-3 text-xs text-muted-foreground">
            Cloud keys stay in the system keychain. Local agents receive the
            Polish system prompt and dictated text in isolated one-shot mode
            with tools and sessions disabled.
          </p>
        </div>
      )}
    </FieldSet>
  );

  return (
    <div className="h-full min-h-0 min-w-0 flex flex-col overflow-x-hidden">
      <div className="shrink-0 py-5 pl-2 pr-4">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h1 className="text-2xl font-semibold tracking-tight">Polish</h1>
              <Dialog>
                <DialogTrigger render={<Button
                    type="button"
                    variant="secondary"
                    size="icon"
                    aria-label="Polish guide"
                    className="rounded-full"
                  />}><HelpCircle className="h-4.5 w-4.5" /></DialogTrigger>
                <DialogContent className="sm:max-w-lg">
                  <DialogHeader>
                    <DialogTitle>Polish guide</DialogTitle>
                    <DialogDescription>
                      Configure the provider, dictionary, corrections, snippets,
                      and writing modes.
                    </DialogDescription>
                  </DialogHeader>
                  <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                    <p>
                      <strong className="text-foreground">Provider</strong>{" "}
                      chooses the cloud API or isolated local agent used by
                      Polish.
                    </p>
                    <p>
                      <strong className="text-foreground">Dictionary</strong>{" "}
                      protects words and names and can improve recognition.
                    </p>
                    <p>
                      <strong className="text-foreground">Corrections</strong>{" "}
                      applies exact replacements with or without Polish.
                    </p>
                    <p>
                      <strong className="text-foreground">Snippets</strong>{" "}
                      expands “insert” triggers into saved text.
                    </p>
                    <p>
                      <strong className="text-foreground">Modes</strong>{" "}
                      sets the default writing mode and optional per-app
                      overrides.
                    </p>
                  </div>
                </DialogContent>
              </Dialog>
            </div>
            <p className="mt-0.5 text-sm leading-relaxed text-muted-foreground">
              AI cleanup when enabled; dictionary, corrections, snippets, and
              app identity remain available independently.
            </p>
          </div>
          <div className="ml-auto shrink-0">{polishHeaderActions}</div>
        </div>
      </div>

      <ScrollArea className="flex-1 min-h-0 min-w-0 overflow-x-hidden">
        <div className="min-w-0 max-w-full overflow-x-hidden pt-5 pb-4 pl-2 pr-4">
          <EnhancementSettings
            preset={enhancementOptions.preset}
            finalTextLanguage={effectiveFinalTextLanguage}
            writingSettings={writingSettings}
            aiFormattingEnabled={aiSettings.enabled}
            providerContent={polishSetupContent}
            onPresetChange={(preset) =>
              void persistEnhancementOptions({ preset })
            }
            onFinalTextLanguageChange={handleFinalTextLanguageChange}
            onWritingSettingsChange={handleWritingSettingsChange}
            writingSettingsDisabled={!settingsLoaded}
          />
        </div>
      </ScrollArea>

      <ApiKeyModal
        isOpen={showApiKeyModal}
        onClose={() => {
          setShowApiKeyModal(false);
          setGuidedSetupProvider(null);
        }}
        onSubmit={handleApiKeySubmit}
        providerName={selectedProvider}
        isLoading={isLoading}
      />

      <OpenAICompatConfigModal
        isOpen={showOpenAIConfig}
        defaultBaseUrl={openAIDefaultBaseUrl}
        defaultModel={customModelName || ""}
        onClose={() => setShowOpenAIConfig(false)}
        onSubmit={async ({ baseUrl, model, apiKey }) => {
          try {
            setIsLoading(true);
            const trimmedBase = baseUrl.trim();
            const trimmedModel = model.trim();
            const trimmedKey = apiKey?.trim() || "";

            const existingKey = trimmedKey ? "" : await getApiKey("custom");
            const validationKey = trimmedKey || existingKey || "";
            const noAuth = !validationKey;

            if (trimmedKey) {
              await saveApiKey("custom", trimmedKey, {
                baseUrl: trimmedBase,
                model: trimmedModel,
                noAuth: false,
              });
            } else {
              await invoke("validate_ai_api_key", {
                args: {
                  provider: "custom",
                  apiKey: validationKey,
                  baseUrl: trimmedBase,
                  model: trimmedModel,
                  noAuth,
                },
              });
              if (validationKey) {
                await invoke("cache_ai_api_key", {
                  args: { provider: "custom", apiKey: validationKey },
                });
              }
            }

            await invoke("set_openai_config", {
              args: { baseUrl: trimmedBase, noAuth },
            });

            await invoke("update_ai_settings", {
              enabled: aiSettings.enabled,
              provider: "custom",
              model: trimmedModel,
            });

            setCustomModelName(trimmedModel);
            setOpenAIDefaultBaseUrl(trimmedBase);
            setAISettings((prev) => ({
              ...prev,
              provider: "custom",
              model: trimmedModel,
              hasApiKey: true,
              modelsByProvider: {
                ...prev.modelsByProvider,
                custom: trimmedModel,
              },
            }));
            setProviderApiKeys((prev) => ({ ...prev, custom: true }));

            toast.success("Custom provider configured");
            setShowOpenAIConfig(false);
          } catch (error) {
            const message = getErrorMessage(
              error,
              "Failed to save configuration",
            );
            toast.error(message);
          } finally {
            setIsLoading(false);
          }
        }}
      />
    </div>
  );
}
