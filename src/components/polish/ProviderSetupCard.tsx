import { Badge } from "@/components/ui/badge";
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
import type { AISettings } from "@/types/ai";
import type {
  AIProviderConfig,
  AIProviderModel,
  AgentCliProbe,
} from "@/types/providers";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  Brain,
  ChevronDown,
  ExternalLink,
  Key,
  RefreshCw,
  Search,
  Settings2,
  Trash2,
  X,
} from "lucide-react";
import { humanizeModelId } from "@/lib/model-display";
import { useEnhancementsStore } from "@/state/enhancements";
import {
  AGENT_CLI_DEFAULT_LABEL,
  CLI_DEFAULT_MODEL_VALUE,
  fallbackAgentCliDefault,
  formatModelCost,
  formatReasoningLevel,
  defaultAgentCliReasoning,
  getAgentCliProbeState,
  shortModelName,
  isAgentCliProvider,
  isDefaultAgentCliModel,
  modelMatchesQuery,
  providerSupportsReasoning,
  toModelSelectValue,
  fromModelSelectValue,
  qualifyModelId,
  type ModelPickerGroup,
  type ModelPickerItem,
} from "./agentCli";
import { AgentModelPickerDialog } from "./AgentModelPickerDialog";
import type { ProviderTab } from "./useAiProviderSettings";

export interface ProviderSetupCardProps {
  aiSettings: AISettings;
  providers: AIProviderConfig[];
  providerApiKeys: Record<string, boolean>;
  agentCliStatus: Record<string, AgentCliProbe>;
  agentCliProbing: Record<string, boolean>;
  customModelName: string;
  guidedSetupProvider: string | null;
  setGuidedSetupProvider: (provider: string | null) => void;
  isModelsLoading: (providerId: string) => boolean;
  getModels: (providerId: string) => AIProviderModel[];
  fetchModels: (providerId: string) => void;
  getError: (providerId: string) => string | null;
  getDisplayModels: (providerId: string) => AIProviderModel[];
  hasSelectedModel: boolean;
  showGuidedSetup: boolean;
  showAiModelReselectionNotice: boolean;
  activeProviderName: string;
  activeModelName: string;
  activeReasoningName: string;
  providerTab: ProviderTab;
  setProviderTab: (tab: ProviderTab) => void;
  providerSearch: string;
  setProviderSearch: (search: string) => void;
  providerSetupOpen: boolean;
  setProviderSetupOpen: (open: boolean) => void;
  onSelectModel: (providerId: string, modelId: string) => Promise<void>;
  onSelectReasoning: (providerId: string, reasoning: string) => Promise<void>;
  onToggleFastMode: (providerId: string, enabled: boolean) => Promise<void>;
  onRefreshAgentCli: (provider: AIProviderConfig) => Promise<void>;
  onSetupApiKey: (providerId: string) => Promise<void>;
  onRemoveApiKey: (providerId: string) => Promise<void>;
}

export function ProviderSetupCard(props: ProviderSetupCardProps) {
  const {
    aiSettings,
    providers,
    providerApiKeys,
    agentCliStatus,
    agentCliProbing,
    customModelName,
    guidedSetupProvider,
    setGuidedSetupProvider,
    isModelsLoading,
    getModels,
    fetchModels,
    getError,
    getDisplayModels,
    hasSelectedModel,
    showGuidedSetup,
    showAiModelReselectionNotice,
    activeProviderName,
    activeModelName,
    activeReasoningName,
    providerTab,
    setProviderTab,
    providerSearch,
    setProviderSearch,
    providerSetupOpen,
    setProviderSetupOpen,
    onSelectModel,
    onSelectReasoning,
    onToggleFastMode,
    onRefreshAgentCli,
    onSetupApiKey,
    onRemoveApiKey,
  } = props;

  const polishError = useEnhancementsStore((s) => s.polishError);
  const clearPolishError = useEnhancementsStore((s) => s.clearPolishError);

  const polishSetupExpanded = showGuidedSetup || providerSetupOpen;

  const providerQuery = providerSearch.trim().toLowerCase();
  const filteredProviders = providers.filter((provider) => {
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
  });

  const openProviderSetup = () => {
    setProviderTab(isAgentCliProvider(aiSettings.provider) ? "local" : "cloud");
    setProviderSearch("");
    setProviderSetupOpen(true);
  };

  const hasLoadingProviders = providers.some((provider) =>
    isModelsLoading(provider.id),
  );

  return (
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

              {filteredProviders.map((provider) => (
                <ProviderRow
                  key={provider.id}
                  provider={provider}
                  {...{ aiSettings, providerApiKeys, agentCliStatus, agentCliProbing, customModelName, guidedSetupProvider, setGuidedSetupProvider, isModelsLoading, getModels, fetchModels, getError, getDisplayModels, providerQuery, showGuidedSetup, onSelectModel, onSelectReasoning, onToggleFastMode, onRefreshAgentCli, onSetupApiKey, onRemoveApiKey }}
                />
              ))}
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
}

interface ProviderRowProps {
  provider: AIProviderConfig;
  aiSettings: AISettings;
  providerApiKeys: Record<string, boolean>;
  agentCliStatus: Record<string, AgentCliProbe>;
  agentCliProbing: Record<string, boolean>;
  customModelName: string;
  guidedSetupProvider: string | null;
  setGuidedSetupProvider: (provider: string | null) => void;
  isModelsLoading: (providerId: string) => boolean;
  getModels: (providerId: string) => AIProviderModel[];
  fetchModels: (providerId: string) => void;
  getError: (providerId: string) => string | null;
  getDisplayModels: (providerId: string) => AIProviderModel[];
  providerQuery: string;
  showGuidedSetup: boolean;
  onSelectModel: (providerId: string, modelId: string) => Promise<void>;
  onSelectReasoning: (providerId: string, reasoning: string) => Promise<void>;
  onToggleFastMode: (providerId: string, enabled: boolean) => Promise<void>;
  onRefreshAgentCli: (provider: AIProviderConfig) => Promise<void>;
  onSetupApiKey: (providerId: string) => Promise<void>;
  onRemoveApiKey: (providerId: string) => Promise<void>;
}

function ProviderRow({
  provider,
  aiSettings,
  providerApiKeys,
  agentCliStatus,
  agentCliProbing,
  customModelName,
  setGuidedSetupProvider,
  isModelsLoading,
  getModels,
  fetchModels,
  getError,
  getDisplayModels,
  providerQuery,
  showGuidedSetup,
  onSelectModel,
  onSelectReasoning,
  onToggleFastMode,
  onRefreshAgentCli,
  onSetupApiKey,
  onRemoveApiKey,
}: ProviderRowProps) {
  const agentCli = isAgentCliProvider(provider.id);
  const agentProbe = agentCli ? agentCliStatus[provider.id] : undefined;
  const agentCliState = agentProbe ? getAgentCliProbeState(agentProbe) : null;
  const agentCliChecking = agentCli && !agentProbe && agentCliProbing[provider.id];
  const agentCliRefreshing =
    agentCli && Boolean(agentProbe) && agentCliProbing[provider.id];
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
    defaultAgentCliModel && !discoveredModels.some(isDefaultAgentCliModel)
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
      ? models.filter((model) => modelMatchesQuery(model, providerQuery))
      : models;
  const toPickerItem = (model: AIProviderModel): ModelPickerItem => ({
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
  const modelPickerItems = modelPickerGroups.flatMap((group) => group.items);
  const selectedModelItem =
    modelPickerItems.find((item) => item.value === modelSelectValue) ??
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
  const fastModeEnabled = aiSettings.fastModeByProvider[provider.id] ?? false;

  return (
    <div
      role={agentCli && hasKey ? "button" : undefined}
      tabIndex={agentCli && hasKey ? 0 : undefined}
      aria-pressed={agentCli && hasKey ? isSelected : undefined}
      aria-label={
        agentCli && hasKey ? `Select ${provider.name}` : undefined
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
        void onSelectModel(provider.id, selectedModel);
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
        void onSelectModel(provider.id, selectedModel);
      }}
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <div className="min-w-0 flex-1">
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
        </div>

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
                onSelect={(modelId) => void onSelectModel(provider.id, modelId)}
              />
              {reasoningLevels.length > 0 && (
                <Select
                  value={reasoning}
                  disabled={!agentCliReady}
                  onValueChange={(value) =>
                    value != null &&
                    void onSelectReasoning(provider.id, value)
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
                      void onToggleFastMode(provider.id, enabled)
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
                onClick={() => void onRefreshAgentCli(provider)}
                disabled={agentCliProbing[provider.id]}
              >
                <RefreshCw
                  className={`h-3.5 w-3.5 ${
                    agentCliProbing[provider.id] ? "animate-spin" : ""
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
                      void onSelectModel(
                        provider.id,
                        fromModelSelectValue(item.value),
                      );
                    }
                  }}
                  itemToStringLabel={(item) => item.label}
                  itemToStringValue={(item) => item.value}
                  isItemEqualToValue={(item, value) => item.value === value.value}
                  filter={(item, query) => {
                    const searchable = [
                      item.label,
                      item.qualifiedId,
                      item.model.sourceProvider ?? "",
                    ]
                      .join(" ")
                      .toLowerCase();
                    return searchable.includes(query.trim().toLowerCase());
                  }}
                  autoHighlight
                >
                  <ComboboxInput
                    className="w-full sm:w-64"
                    placeholder="Search models"
                    aria-label={`Model for ${provider.name}`}
                  />
                  <ComboboxContent className="min-w-80">
                    <ComboboxEmpty>No models found.</ComboboxEmpty>
                    <ComboboxList>
                      {(group: ModelPickerGroup) => (
                        <ComboboxGroup
                          key={`${provider.id}-${group.value}`}
                          items={group.items}
                        >
                          <ComboboxLabel>{group.value}</ComboboxLabel>
                          <ComboboxCollection>
                            {(item: ModelPickerItem) => {
                              const cost = formatModelCost(item.model);
                              const detail = [
                                item.model.reasoning ||
                                providerSupportsReasoning(provider)
                                  ? "reasoning"
                                  : "",
                                cost,
                              ]
                                .filter(Boolean)
                                .join(" · ");
                              return (
                                <ComboboxItem key={item.value} value={item}>
                                  <span className="flex min-w-0 flex-1 items-center justify-between gap-3">
                                    <span className="min-w-0 truncate">
                                      {item.label}
                                      {detail ? ` · ${detail}` : ""}
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
                        ? void onSetupApiKey(provider.id)
                        : void fetchModels(provider.id)
                    }
                    disabled={
                      !provider.isCustom && isModelsLoading(provider.id)
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
                      if (confirmed) void onRemoveApiKey(provider.id);
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
                      onClick={() => window.open(provider.apiKeyUrl, "_blank")}
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
                      void onSetupApiKey(provider.id);
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
}
