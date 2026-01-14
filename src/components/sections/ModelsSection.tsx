import { ApiKeyModal } from "@/components/ApiKeyModal";
import { LanguageSelection } from "@/components/LanguageSelection";
import { ModelCard } from "@/components/ModelCard";
import {
  RemoteServerCard,
  SavedConnection,
} from "@/components/RemoteServerCard";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useSettings } from "@/contexts/SettingsContext";
import { getCloudProviderByModel } from "@/lib/cloudProviders";
import { cn } from "@/lib/utils";
import { ModelInfo, isCloudModel, isLocalModel } from "@/types";
import {
  Bot,
  CheckCircle,
  Download,
  HardDrive,
  Plus,
  Server,
  Star,
  Zap,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Label } from "../ui/label";
import { invoke } from "@tauri-apps/api/core";
import { AddServerModal } from "./AddServerModal";

interface ModelsSectionProps {
  models: [string, ModelInfo][];
  downloadProgress: Record<string, number>;
  verifyingModels: Set<string>;
  currentModel?: string;
  onDownload: (modelName: string) => Promise<void> | void;
  onDelete: (modelName: string) => Promise<void> | void;
  onCancelDownload: (modelName: string) => Promise<void> | void;
  onSelect: (modelName: string) => Promise<void> | void;
  refreshModels: () => Promise<void>;
}

type CloudModalMode = "connect" | "update";

interface CloudModalState {
  providerId: string;
  mode: CloudModalMode;
}

export function ModelsSection({
  models,
  downloadProgress,
  verifyingModels,
  currentModel,
  onDownload,
  onDelete,
  onCancelDownload,
  onSelect,
  refreshModels,
}: ModelsSectionProps) {
  const { settings, updateSettings } = useSettings();
  const [cloudModal, setCloudModal] = useState<CloudModalState | null>(null);
  const [cloudModalLoading, setCloudModalLoading] = useState(false);
  const [remoteServers, setRemoteServers] = useState<SavedConnection[]>([]);
  const [activeRemoteServer, setActiveRemoteServer] = useState<string | null>(
    null
  );
  const [addServerModalOpen, setAddServerModalOpen] = useState(false);

  const { availableToUse, availableToSetup } = useMemo(() => {
    const useList: [string, ModelInfo][] = [];
    const setupList: [string, ModelInfo][] = [];

    models.forEach(([name, model]) => {
      const isReady = !!model.downloaded && !model.requires_setup;
      if (isReady) {
        useList.push([name, model]);
      } else {
        setupList.push([name, model]);
      }
    });

    // Locals first within each list
    const sortFn = ([, a]: [string, ModelInfo], [, b]: [string, ModelInfo]) => {
      if (isLocalModel(a) && isCloudModel(b)) return -1;
      if (isCloudModel(a) && isLocalModel(b)) return 1;
      return 0;
    };
    useList.sort(sortFn);
    setupList.sort(sortFn);

    return { availableToUse: useList, availableToSetup: setupList };
  }, [models]);

  // No header summary line — section titles include counts

  const currentEngine = (settings?.current_model_engine ?? "whisper") as
    | "whisper"
    | "parakeet"
    | "soniox";
  const currentModelName = settings?.current_model ?? "";
  const languageValue = settings?.language ?? "en";

  const isEnglishOnlyModel = useMemo(() => {
    if (!settings) return false;
    if (currentEngine === "whisper") {
      return /\.en$/i.test(currentModelName);
    }
    if (currentEngine === "parakeet") {
      return currentModelName.includes("-v2");
    }
    return false;
  }, [currentEngine, currentModelName, settings]);

  const handleLanguageChange = useCallback(
    async (value: string) => {
      try {
        await updateSettings({ language: value });
      } catch (error) {
        console.error("Failed to update language:", error);
        toast.error("Failed to update language");
      }
    },
    [updateSettings],
  );

  // Remote servers management
  const fetchRemoteServers = useCallback(async () => {
    try {
      const servers = await invoke<SavedConnection[]>("list_remote_servers");
      setRemoteServers(servers);
    } catch (error) {
      console.error("Failed to fetch remote servers:", error);
    }
  }, []);

  const fetchActiveRemoteServer = useCallback(async () => {
    try {
      const activeId = await invoke<string | null>("get_active_remote_server");
      setActiveRemoteServer(activeId);
    } catch (error) {
      console.error("Failed to fetch active remote server:", error);
    }
  }, []);

  useEffect(() => {
    fetchRemoteServers();
    fetchActiveRemoteServer();
  }, [fetchRemoteServers, fetchActiveRemoteServer]);

  const handleSelectRemoteServer = useCallback(
    async (serverId: string) => {
      try {
        await invoke("set_active_remote_server", {
          serverId: serverId === activeRemoteServer ? null : serverId,
        });
        setActiveRemoteServer(
          serverId === activeRemoteServer ? null : serverId
        );
        toast.success(
          serverId === activeRemoteServer
            ? "Remote server deselected"
            : "Remote server selected"
        );
      } catch (error) {
        console.error("Failed to set active remote server:", error);
        toast.error("Failed to select remote server");
      }
    },
    [activeRemoteServer]
  );

  const handleRemoveRemoteServer = useCallback(
    async (serverId: string) => {
      try {
        await invoke("remove_remote_server", { serverId });
        setRemoteServers((prev) => prev.filter((s) => s.id !== serverId));
        if (activeRemoteServer === serverId) {
          setActiveRemoteServer(null);
        }
        toast.success("Server removed");
      } catch (error) {
        console.error("Failed to remove remote server:", error);
        toast.error("Failed to remove server");
      }
    },
    [activeRemoteServer]
  );

  const handleServerAdded = useCallback(
    (server: SavedConnection) => {
      setRemoteServers((prev) => [...prev, server]);
    },
    []
  );

  const activeModelLabel = useMemo(() => {
    if (!currentModel) return null;
    const entry = models.find(([name]) => name === currentModel);
    if (!entry) return currentModel;
    return entry[1].display_name || currentModel;
  }, [currentModel, models]);

  useEffect(() => {
    if (!settings) return;
    if (isEnglishOnlyModel && settings.language !== "en") {
      updateSettings({ language: "en" }).catch((error) => {
        console.error("Failed to enforce English fallback:", error);
      });
    }
  }, [isEnglishOnlyModel, settings, updateSettings]);

  const hasDownloading = useMemo(
    () => Object.keys(downloadProgress).length > 0,
    [downloadProgress],
  );
  const hasVerifying = verifyingModels.size > 0;

  const openCloudModal = useCallback(
    (providerId: string, mode: CloudModalMode) => {
      setCloudModal({ providerId, mode });
    },
    [],
  );

  const closeCloudModal = useCallback(() => {
    if (cloudModalLoading) return;
    setCloudModal(null);
  }, [cloudModalLoading]);

  const handleCloudKeySubmit = useCallback(
    async (apiKey: string) => {
      if (!cloudModal) return;
      const provider = getCloudProviderByModel(cloudModal.providerId);
      if (!provider) {
        toast.error("Unknown cloud provider");
        return;
      }

      setCloudModalLoading(true);
      try {
        await provider.addKey(apiKey);
        await refreshModels();
        toast.success(
          `${provider.providerName} key ${
            cloudModal.mode === "update" ? "updated" : "saved"
          }`,
        );
        setCloudModal(null);
        if (cloudModal.mode === "connect") {
          await Promise.resolve(onSelect(provider.modelName));
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        toast.error(`Failed to save ${provider.providerName} key: ${message}`);
      } finally {
        setCloudModalLoading(false);
      }
    },
    [cloudModal, onSelect, refreshModels],
  );

  const handleCloudDisconnect = useCallback(
    async (modelName: string) => {
      const provider = getCloudProviderByModel(modelName);
      if (!provider) {
        toast.error("Unknown cloud provider");
        return;
      }

      try {
        await provider.removeKey();
        toast.success(`${provider.providerName} disconnected`);
        if (settings?.current_model === provider.modelName) {
          await updateSettings({
            current_model: "",
            current_model_engine: "whisper",
          });
        }
        await refreshModels();
        // Ensure tray menu reflects removal immediately even if selection unchanged
        try {
          await invoke('update_tray_menu');
        } catch (e) {
          console.warn('[ModelsSection] Failed to refresh tray menu after disconnect:', e);
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        toast.error(
          `Failed to disconnect ${provider.providerName}: ${message}`,
        );
      }
    },
    [refreshModels, settings?.current_model, updateSettings],
  );

  const activeProvider = cloudModal
    ? getCloudProviderByModel(cloudModal.providerId)
    : undefined;
  const isModalOpen = !!cloudModal && !!activeProvider;

  const renderCloudCard = useCallback(
    ([name, model]: [string, ModelInfo]) => {
      if (!isCloudModel(model)) return null;

      const provider =
        getCloudProviderByModel(name) ?? getCloudProviderByModel(model.engine);
      const requiresSetup = model.requires_setup;
      const isActive = currentModel === name;

      return (
        <Card
          key={name}
          className={cn(
            "px-4 py-3 border-border/50 transition",
            requiresSetup ? "opacity-90" : "cursor-pointer hover:border-border",
            isActive && "bg-primary/5",
          )}
          onClick={() => {
            if (requiresSetup) {
              openCloudModal(name, "connect");
              return;
            }
            void onSelect(name);
          }}
        >
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0">
              <h3 className="font-medium text-sm">
                {model.display_name || provider?.displayName || name}
              </h3>
            </div>
            <div className="flex items-center gap-2">
              {requiresSetup ? (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={(event) => {
                    event.stopPropagation();
                    openCloudModal(name, "connect");
                  }}
                >
                  {provider?.setupCta ?? "Add API Key"}
                </Button>
              ) : (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={(event) => {
                    event.stopPropagation();
                    handleCloudDisconnect(name);
                  }}
                >
                  Remove API Key
                </Button>
              )}
            </div>
          </div>
        </Card>
      );
    },
    [currentModel, handleCloudDisconnect, onSelect, openCloudModal],
  );

  return (
    <div className="h-full flex flex-col">
      <div className="px-6 py-4 border-b border-border/40">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold">Models</h1>
            <p className="text-sm text-muted-foreground mt-1">
              Download/Setup and select the model to use
            </p>
          </div>
          <div className="flex items-center gap-3">
            {(hasDownloading || hasVerifying) && (
              <div className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-blue-500/10 text-sm font-medium">
                <Download className="h-3.5 w-3.5 text-blue-500" />
                {hasDownloading ? "Downloading..." : "Verifying..."}
              </div>
            )}
            {activeModelLabel ? (
              <span className="text-sm text-muted-foreground">
                Active:{" "}
                <span className="text-amber-600 dark:text-amber-500">
                  {activeModelLabel}
                </span>
              </span>
            ) : (
              availableToUse.length > 0 && (
                <span className="text-sm text-amber-600 dark:text-amber-500">
                  No model selected
                </span>
              )
            )}
          </div>
        </div>
      </div>

      {/* Legend + Spoken Language (same row, like Settings style) */}
      <div className="px-6 py-3 border-b border-border/20 space-y-6">
        <div className="flex items-center justify-between gap-2">
          <div className="space-y-0.5">
            <Label htmlFor="language" className="text-sm font-medium">
              Spoken Language
            </Label>
            <p className="text-xs text-muted-foreground">
              The language you'll be speaking in
            </p>
          </div>
          <LanguageSelection
            value={languageValue}
            engine={currentEngine}
            englishOnly={isEnglishOnlyModel}
            onValueChange={(value) => {
              void handleLanguageChange(value);
            }}
          />
        </div>
        <div className="flex items-center gap-6 text-xs text-muted-foreground">
          <span className="flex items-center gap-1.5">
            <Zap className="w-3.5 h-3.5 text-green-500" />
            Speed
          </span>
          <span className="flex items-center gap-1.5">
            <CheckCircle className="w-3.5 h-3.5 text-blue-500" />
            Accuracy
          </span>
          <span className="flex items-center gap-1.5">
            <HardDrive className="w-3.5 h-3.5 text-purple-500" />
            Size
          </span>
          <span className="flex items-center gap-1.5">
            <Star className="w-3.5 h-3.5 fill-yellow-500 text-yellow-500" />
            Recommended
          </span>
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-hidden">
        <ScrollArea className="h-full">
          <div className="px-6 py-2 space-y-6">
            {availableToUse.length > 0 && (
              <div className="space-y-4">
                <h2 className="text-base font-semibold text-foreground">
                  Available to Use ({availableToUse.length})
                </h2>
                <div className="grid gap-3">
                  {availableToUse.map(([name, model]) =>
                    isLocalModel(model) ? (
                      <ModelCard
                        key={name}
                        name={name}
                        model={model}
                        downloadProgress={downloadProgress[name]}
                        isVerifying={verifyingModels.has(name)}
                        onDownload={onDownload}
                        onDelete={onDelete}
                        onCancelDownload={onCancelDownload}
                        onSelect={(modelName) => {
                          void onSelect(modelName);
                        }}
                        showSelectButton={model.downloaded}
                        isSelected={currentModel === name}
                      />
                    ) : (
                      renderCloudCard([name, model])
                    ),
                  )}
                </div>
              </div>
            )}

            {availableToSetup.length > 0 && (
              <div className="space-y-4">
                <h2 className="text-base font-semibold text-foreground">
                  Available to Set Up ({availableToSetup.length})
                </h2>
                <div className="grid gap-3">
                  {availableToSetup.map(([name, model]) =>
                    isLocalModel(model) ? (
                      <ModelCard
                        key={name}
                        name={name}
                        model={model}
                        downloadProgress={downloadProgress[name]}
                        isVerifying={verifyingModels.has(name)}
                        onDownload={onDownload}
                        onDelete={onDelete}
                        onCancelDownload={onCancelDownload}
                        onSelect={(modelName) => {
                          void onSelect(modelName);
                        }}
                        showSelectButton={model.downloaded}
                        isSelected={currentModel === name}
                      />
                    ) : (
                      renderCloudCard([name, model])
                    ),
                  )}
                </div>
              </div>
            )}

            {/* Remote Servers Section */}
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <h2 className="text-base font-semibold text-foreground flex items-center gap-2">
                  <Server className="h-4 w-4" />
                  Remote Servers ({remoteServers.length})
                </h2>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setAddServerModalOpen(true)}
                >
                  <Plus className="h-4 w-4 mr-1" />
                  Add Server
                </Button>
              </div>
              {remoteServers.length > 0 ? (
                <div className="grid gap-3">
                  {remoteServers.map((server) => (
                    <RemoteServerCard
                      key={server.id}
                      server={server}
                      isActive={activeRemoteServer === server.id}
                      onSelect={handleSelectRemoteServer}
                      onRemove={handleRemoveRemoteServer}
                    />
                  ))}
                </div>
              ) : (
                <Card className="px-4 py-6 border-border/50 border-dashed">
                  <div className="text-center text-muted-foreground">
                    <Server className="h-8 w-8 mx-auto mb-2 opacity-50" />
                    <p className="text-sm">No remote servers configured</p>
                    <p className="text-xs mt-1 opacity-75">
                      Connect to another VoiceTypr to use its transcription
                    </p>
                  </div>
                </Card>
              )}
            </div>

            {availableToUse.length === 0 &&
              availableToSetup.length === 0 &&
              remoteServers.length === 0 && (
                <div className="flex-1 flex items-center justify-center py-12">
                  <div className="text-center">
                    <Bot className="w-12 h-12 text-muted-foreground/30 mx-auto mb-4" />
                    <p className="text-sm text-muted-foreground">
                      No models available
                    </p>
                    <p className="text-xs text-muted-foreground/70 mt-2">
                      Models will appear here when they become available.
                    </p>
                  </div>
                </div>
              )}
          </div>
        </ScrollArea>
      </div>

      {activeProvider && (
        <ApiKeyModal
          isOpen={isModalOpen}
          onClose={closeCloudModal}
          onSubmit={handleCloudKeySubmit}
          providerName={activeProvider.providerName}
          isLoading={cloudModalLoading}
          title={
            cloudModal?.mode === "update"
              ? `Update ${activeProvider.providerName} API Key`
              : `Add ${activeProvider.providerName} API Key`
          }
          description={
            cloudModal?.mode === "update"
              ? `Update your ${activeProvider.providerName} API key to keep cloud transcription running smoothly.`
              : `Enter your ${activeProvider.providerName} API key to enable cloud transcription. Your key is stored securely in the system keychain.`
          }
          submitLabel={
            cloudModal?.mode === "update" ? "Update API Key" : "Save API Key"
          }
          docsUrl={activeProvider.docsUrl}
        />
      )}

      <AddServerModal
        open={addServerModalOpen}
        onOpenChange={setAddServerModalOpen}
        onServerAdded={handleServerAdded}
      />
    </div>
  );
}
