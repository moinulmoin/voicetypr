import { useTauriEvent } from "@/hooks/useTauriEvent";
import { SettingsCard, SettingsPage } from "@/components/settings/settings-ui";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useSettings } from "@/contexts/SettingsContext";
import { getModelDisplayName } from "@/lib/model-display";
import { isCloudModel, isLocalModel } from "@/types";
import { Download } from "lucide-react";
import { useMemo, useState } from "react";
import { createLogger } from "@/lib/logger";
import {
  CloudApiKeyModal,
  CloudProvidersBlock,
  CloudSetupGrid,
} from "./models/CloudProvidersBlock";
import { LocalModelsList, LocalSetupGrid } from "./models/LocalModelsList";
import { ModelsEmptyStates } from "./models/ModelsEmptyStates";
import { ModelsLanguageRow } from "./models/ModelsLanguageRow";
import { ModelsSourcesHeader } from "./models/ModelsSourcesHeader";
import { RemoteServersBlock } from "./models/RemoteServersBlock";
import type { ModelsSectionProps } from "./models/types";
import { useCloudProviders } from "./models/useCloudProviders";
import { useRemoteServers } from "./models/useRemoteServers";
import { useSpokenLanguage } from "./models/useSpokenLanguage";

const log = createLogger("models");

export function ModelsSection({
  models,
  downloadProgress,
  downloadPhases = {},
  verifyingModels,
  currentModel,
  downloadErrors = {},
  isLoading = false,
  onDownload,
  onDelete,
  onCancelDownload,
  onRepair,
  onSelect,
  refreshModels,
}: ModelsSectionProps) {
  const { refreshSettings } = useSettings();
  const language = useSpokenLanguage();
  const remotes = useRemoteServers();
  const cloud = useCloudProviders({
    onSelect,
    refreshModels,
    clearActiveRemote: remotes.clearActiveRemote,
  });
  const [sourceFilter, setSourceFilter] = useState<"local" | "cloud" | "remote">("local");

  // Plan 044: when a Soniox storage-limit escalation lands the dashboard
  // here, the cloud cards (and the Soniox stored-files cleanup card) must be
  // visible even if the user had filtered to local/remote sources.
  useTauriEvent("soniox-storage-limit", () => setSourceFilter("cloud"));

  const { availableToUse, availableToSetup } = useMemo(() => {
    const useList: typeof models = [];
    const setupList: typeof models = [];

    models.forEach(([name, model]) => {
      const isReady = !!model.downloaded && !model.requires_setup;
      if (isReady) {
        useList.push([name, model]);
      } else {
        setupList.push([name, model]);
      }
    });

    // Locals first within each list
    const sortFn = ([, a]: (typeof models)[number], [, b]: (typeof models)[number]) => {
      if (isLocalModel(a) && isCloudModel(b)) return -1;
      if (isCloudModel(a) && isLocalModel(b)) return 1;
      return 0;
    };
    useList.sort(sortFn);
    setupList.sort(sortFn);

    return { availableToUse: useList, availableToSetup: setupList };
  }, [models]);

  const prioritizeCurrent = ([left]: (typeof models)[number], [right]: (typeof models)[number]) =>
    Number(right === currentModel) - Number(left === currentModel);
  const readyLocalModels = availableToUse
    .filter(([, model]) => isLocalModel(model))
    .sort(prioritizeCurrent);
  const readyCloudModels = availableToUse
    .filter(([, model]) => isCloudModel(model))
    .sort(prioritizeCurrent);
  const setupLocalModels = availableToSetup.filter(([, model]) => isLocalModel(model));
  const setupCloudModels = availableToSetup.filter(([, model]) => isCloudModel(model));

  const localCount = readyLocalModels.length + setupLocalModels.length;
  const cloudCount = readyCloudModels.length + setupCloudModels.length;
  const remoteCount = remotes.remoteServers.length;
  const showLocal = sourceFilter === "local";
  const showCloud = sourceFilter === "cloud";
  const showRemote = sourceFilter === "remote";

  const selectedModel = models.find(([name]) => name === currentModel)?.[1];
  const selectedSourceType = selectedModel && isCloudModel(selectedModel) ? "cloud" : "local";
  const activeRemote = remotes.remoteServers.find(
    (server) => server.id === remotes.activeRemoteServer,
  );
  const currentSourceType = remotes.activeRemoteServer
    ? "Remote"
    : selectedSourceType === "cloud"
      ? "Cloud"
      : "Local";
  const currentSourceLabel = remotes.activeRemoteServer
    ? activeRemote?.name ||
      (activeRemote ? `${activeRemote.host}:${activeRemote.port}` : "Remote Voicetypr")
    : getModelDisplayName(currentModel) || "No source selected";

  // Keep the tab honest when the active source changes underneath (tray or
  // model switch) — adjust during render instead of a flashing effect. The
  // user can still switch tabs freely until the next external change.
  const trackedSource = remotes.activeRemoteServer ? "remote" : selectedSourceType;
  const [lastTrackedSource, setLastTrackedSource] = useState<string | null>(null);
  if (trackedSource !== lastTrackedSource) {
    setLastTrackedSource(trackedSource);
    setSourceFilter(trackedSource);
  }

  useTauriEvent<{ model: string; engine: string }>("model-changed", (payload) => {
    log.debug("[ModelsSection] model-changed event received:", payload);
    // Refresh all model-related state
    void remotes.fetchActiveRemoteServer();
    void remotes.fetchRemoteServers();
    void refreshSettings();
  });

  const hasDownloading = useMemo(
    () => Object.keys(downloadProgress).length > 0,
    [downloadProgress],
  );
  const hasVerifying = verifyingModels.size > 0;

  const localActions = {
    downloadProgress,
    downloadPhases,
    verifyingModels,
    downloadErrors,
    onDownload,
    onDelete,
    onCancelDownload,
    onRepair,
    onSelect,
    currentModel,
    activeRemoteServer: remotes.activeRemoteServer,
    clearActiveRemote: remotes.clearActiveRemote,
  };

  const cloudBindings = {
    currentModel,
    activeRemoteServer: remotes.activeRemoteServer,
    onSelect,
    clearActiveRemote: remotes.clearActiveRemote,
    cloud,
  };

  return (
    <>
      <SettingsPage>
        <ModelsSourcesHeader
          currentSourceType={currentSourceType}
          currentSourceLabel={currentSourceLabel}
        >
          <ModelsLanguageRow
            languageValue={language.languageValue}
            currentEngine={language.currentEngine}
            isEnglishOnlyModel={language.isEnglishOnlyModel}
            hasDownloading={hasDownloading}
            hasVerifying={hasVerifying}
            onLanguageChange={language.handleLanguageChange}
          />
        </ModelsSourcesHeader>

        <Tabs
          value={sourceFilter}
          onValueChange={(value) => setSourceFilter(value as typeof sourceFilter)}
        >
          <TabsList aria-label="Transcription source type">
            <TabsTrigger value="local">Local ({localCount})</TabsTrigger>
            <TabsTrigger value="cloud">Cloud ({cloudCount})</TabsTrigger>
            <TabsTrigger value="remote">Remote ({remoteCount})</TabsTrigger>
          </TabsList>
          {showLocal && localCount > 0 && (
            <p className="mt-1.5 text-xs text-muted-foreground">
              {localCount} available · {readyLocalModels.length} downloaded
            </p>
          )}
        </Tabs>

        {showLocal && <LocalModelsList readyLocalModels={readyLocalModels} {...localActions} />}

        {showCloud && (
          <CloudProvidersBlock readyCloudModels={readyCloudModels} {...cloudBindings} />
        )}

        {((showLocal && setupLocalModels.length > 0) ||
          (showCloud && setupCloudModels.length > 0)) && (
          <SettingsCard
            icon={Download}
            title="Set up sources"
            description="Download local models or connect cloud providers before selecting them."
          >
            <div className="mt-4 space-y-3">
              {showLocal && setupLocalModels.length > 0 && (
                <LocalSetupGrid setupLocalModels={setupLocalModels} {...localActions} />
              )}
              {showCloud && setupCloudModels.length > 0 && (
                <CloudSetupGrid setupCloudModels={setupCloudModels} {...cloudBindings} />
              )}
            </div>
          </SettingsCard>
        )}

        <RemoteServersBlock remotes={remotes} visible={showRemote} />

        <ModelsEmptyStates
          isLoading={isLoading}
          hasModels={availableToUse.length > 0 || availableToSetup.length > 0}
          hasRemoteServers={remotes.remoteServers.length > 0}
        />
      </SettingsPage>

      <CloudApiKeyModal cloud={cloud} />
    </>
  );
}
