import { Badge } from "@/components/ui/badge";
import {
  FieldDescription,
  FieldSet,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { AISettings } from "@/types/ai";
import type {
  AIProviderConfig,
  AIProviderModel,
  AgentCliProbe,
} from "@/types/providers";
import {
  AlertTriangle,
  ChevronDown,
  Search,
  X,
} from "lucide-react";
import { useEnhancementsStore } from "@/state/enhancements";
import {
  isAgentCliProvider,
  modelMatchesQuery,
} from "./agentCli";
import { AgentProviderRow } from "./AgentProviderRow";
import { CloudProviderRow } from "./CloudProviderRow";
import {
  buildProviderRowViewModel,
  type ProviderRowProps,
} from "./providerRowShared";
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

function ProviderRow(props: ProviderRowProps) {
  const model = buildProviderRowViewModel(props);
  if (model.agentCli) {
    return <AgentProviderRow props={props} model={model} />;
  }
  return <CloudProviderRow props={props} model={model} />;
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
