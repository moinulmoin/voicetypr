import { ApiKeyModal } from "@/components/ApiKeyModal";
import { LanguageSelection } from "@/components/LanguageSelection";
import { ModelCard } from "@/components/ModelCard";
import {
  RemoteServerCard,
  SavedConnection,
} from "@/components/RemoteServerCard";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@/components/ui/field";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Spinner } from "@/components/ui/spinner";
import { useSettings } from "@/contexts/SettingsContext";
import { getCloudProviderByModel } from "@/lib/cloudProviders";
import { cn } from "@/lib/utils";
import { ModelInfo, isCloudModel, isLocalModel } from "@/types";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Bot,
  CheckCircle,
  Cloud,
  Download,
  HardDrive,
  HelpCircle,
  Plus,
  Server,
  Star,
  Zap,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
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
  const { settings, updateSettings, refreshSettings } = useSettings();
  const [cloudModal, setCloudModal] = useState<CloudModalState | null>(null);
  const [cloudModalLoading, setCloudModalLoading] = useState(false);
  const [remoteServers, setRemoteServers] = useState<SavedConnection[]>([]);
  const [activeRemoteServer, setActiveRemoteServer] = useState<string | null>(
    null
  );
  const [addServerModalOpen, setAddServerModalOpen] = useState(false);
  const [editingServer, setEditingServer] = useState<SavedConnection | null>(null);
  const [isRefreshingServers, setIsRefreshingServers] = useState(false);

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

  const readyLocalModels = availableToUse.filter(([, model]) => isLocalModel(model));
  const readyCloudModels = availableToUse.filter(([, model]) => isCloudModel(model));
  const setupLocalModels = availableToSetup.filter(([, model]) => isLocalModel(model));
  const setupCloudModels = availableToSetup.filter(([, model]) => isCloudModel(model));

  // No header summary line — section titles include counts

  const currentEngine = (settings?.current_model_engine ?? "whisper") as
    | "whisper"
    | "parakeet"
    | "soniox";
  const currentModelName = settings?.current_model ?? "";
  const languageValue = settings?.speech_language ?? "en";

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
        await updateSettings({ speech_language: value });
      } catch (error) {
        console.error("Failed to update spoken language:", error);
        toast.error("Failed to update spoken language");
      }
    },
    [updateSettings],
  );

  // Remote servers management
  // Quick list fetch (no status checks) - for immediate display
  const fetchRemoteServers = useCallback(async () => {
    try {
      const servers = await invoke<SavedConnection[]>("list_remote_servers");
      setRemoteServers(servers);
    } catch (error) {
      console.error("Failed to fetch remote servers:", error);
    }
  }, []);

  // Full refresh with status checks - check each server in parallel for immediate UI updates
  const refreshRemoteServers = useCallback(async () => {
    setIsRefreshingServers(true);
    try {
      // First get the list of servers
      const servers = await invoke<SavedConnection[]>("list_remote_servers");
      setRemoteServers(servers);

      // Check each server in parallel - update UI as each responds
      const checkPromises = servers.map(async (server) => {
        try {
          const updated = await invoke<SavedConnection>("check_remote_server_status", {
            serverId: server.id,
          });
          // Update this specific server in state immediately
          setRemoteServers((prev) => {
            const index = prev.findIndex((s) => s.id === updated.id);
            if (index >= 0) {
              const newList = [...prev];
              newList[index] = updated;
              return newList;
            }
            return prev;
          });
          return updated;
        } catch (error) {
          console.error(`Failed to check server ${server.id}:`, error);
          return server; // Keep existing data on error
        }
      });

      // Wait for all checks to complete
      await Promise.all(checkPromises);
    } catch (error) {
      console.error("Failed to refresh remote servers:", error);
    } finally {
      setIsRefreshingServers(false);
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
    // On mount: fetch list quickly, then refresh status in background
    fetchRemoteServers();
    fetchActiveRemoteServer();
    // Trigger status refresh after initial list load
    refreshRemoteServers();
  }, [fetchRemoteServers, fetchActiveRemoteServer, refreshRemoteServers]);

  // Note: Status updates are handled via the refreshRemoteServers function
  // which calls check_remote_server_status for each server in parallel
  // and updates the UI immediately as each server responds

  // Refresh active remote server when window gains focus (handles tray menu changes)
  useEffect(() => {
    const handleFocus = () => {
      fetchActiveRemoteServer();
      // Refresh server status when user returns to the app
      refreshRemoteServers();
    };

    window.addEventListener("focus", handleFocus);

    // Also listen for Tauri window focus events
    const unlisten = listen("tauri://focus", handleFocus);

    return () => {
      window.removeEventListener("focus", handleFocus);
      unlisten.then((fn) => fn());
    };
  }, [fetchActiveRemoteServer, refreshRemoteServers]);

  // Listen for model-changed events (from tray menu selection or UI)
  useEffect(() => {
    const unlistenModelChanged = listen<{ model: string; engine: string }>(
      "model-changed",
      (event) => {
        console.log("[ModelsSection] model-changed event received:", event.payload);
        // Refresh all model-related state
        fetchActiveRemoteServer();
        fetchRemoteServers();
        refreshSettings();
      }
    );

    return () => {
      unlistenModelChanged.then((fn) => fn());
    };
  }, [fetchActiveRemoteServer, fetchRemoteServers, refreshSettings]);

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
            ? "Remote VoiceTypr deselected"
            : "Remote VoiceTypr selected"
        );
      } catch (error) {
        console.error("Failed to set active remote server:", error);
        toast.error("Failed to select remote VoiceTypr");
      }
    },
    [activeRemoteServer]
  );

  const handleRemoveRemoteServer = useCallback(
    async (serverId: string) => {
      try {
        if (activeRemoteServer === serverId) {
          await invoke("set_active_remote_server", { serverId: null });
          setActiveRemoteServer(null);
        }

        await invoke("remove_remote_server", { serverId });
        setRemoteServers((prev) => prev.filter((s) => s.id !== serverId));
        toast.success("Remote VoiceTypr removed");
      } catch (error) {
        console.error("Failed to remove remote server:", error);
        toast.error("Failed to remove remote VoiceTypr");
      }
    },
    [activeRemoteServer]
  );

  const handleServerAdded = useCallback(
    (server: SavedConnection) => {
      setRemoteServers((prev) => {
        // Check if this is an update (server already exists)
        const existingIndex = prev.findIndex((s) => s.id === server.id);
        if (existingIndex >= 0) {
          // Update existing server
          const updated = [...prev];
          updated[existingIndex] = server;
          return updated;
        }
        // Add new server
        return [...prev, server];
      });
      setEditingServer(null);
      // Trigger status refresh for all servers
      refreshRemoteServers();
    },
    [refreshRemoteServers]
  );

  const handleEditServer = useCallback((server: SavedConnection) => {
    setEditingServer(server);
    setAddServerModalOpen(true);
  }, []);

  const activeModelLabel = useMemo(() => {
    // If a remote server is active, show "ServerName - ModelName" format
    if (activeRemoteServer) {
      const activeServer = remoteServers.find(s => s.id === activeRemoteServer);
      if (activeServer) {
        const serverName = activeServer.name || activeServer.host;
        if (activeServer.model) {
          return `${serverName} - ${activeServer.model}`;
        }
        return serverName;
      }
    }
    if (!currentModel) return null;
    const entry = models.find(([name]) => name === currentModel);
    if (!entry) return currentModel;
    return entry[1].display_name || currentModel;
  }, [currentModel, models, activeRemoteServer, remoteServers]);

  useEffect(() => {
    if (!settings) return;
    if (isEnglishOnlyModel && settings.speech_language !== "en") {
      updateSettings({ speech_language: "en" }).catch((error) => {
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

  const clearActiveRemote = async () => {
    try {
      await invoke("set_active_remote_server", { serverId: null });
      setActiveRemoteServer(null);
    } catch (error) {
      console.error("Failed to clear active remote:", error);
    }
  };


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
          await clearActiveRemote();
          await Promise.resolve(onSelect(provider.modelName));
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        toast.error(`Failed to save ${provider.providerName} key: ${message}`);
      } finally {
        setCloudModalLoading(false);
      }
    },
    [cloudModal, onSelect, refreshModels, clearActiveRemote],
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
            "px-4 py-3 border transition-all hover:shadow-sm",
            requiresSetup ? "bg-card/70 opacity-90" : "cursor-pointer bg-card/90 hover:border-border",
            isActive && "border-primary/45 bg-primary/5 shadow-sm ring-2 ring-primary/15",
          )}
          onClick={async () => {
            if (requiresSetup) {
              openCloudModal(name, "connect");
              return;
            }
            await clearActiveRemote();
            void onSelect(name);
          }}
        >
          <div className="flex items-center justify-between gap-4">
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <h3 className={cn("truncate text-sm font-semibold tracking-tight", isActive && "text-primary")}>
                  {model.display_name || provider?.displayName || name}
                </h3>
                {isActive && (
                  <Badge className="gap-1">
                    <CheckCircle className="size-3" />
                    Active
                  </Badge>
                )}
              </div>
              <p className="mt-1 flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                <span>{provider?.providerName ?? "Cloud transcription"}</span>
                <span>Speed <span className="font-medium text-foreground">{model.speed_score ?? "—"}</span></span>
                <span>Accuracy <span className="font-medium text-foreground">{model.accuracy_score ?? "—"}</span></span>
              </p>
            </div>
            {requiresSetup ? (
              <Button
                size="sm"
                variant="outline"
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
        </Card>
      );
    },
    [currentModel, handleCloudDisconnect, onSelect, openCloudModal, clearActiveRemote],
  );

  return (
    <div className="flex h-full flex-col bg-background">
      <div className="border-b border-border/40 px-6 py-5">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h1 className="text-2xl font-semibold tracking-tight">
                Transcription sources
              </h1>
              <Dialog>
                <DialogTrigger asChild>
                  <Button type="button" variant="secondary" size="icon" aria-label="Transcription sources guide" className="rounded-full">
                    <HelpCircle className="h-4.5 w-4.5" />
                  </Button>
                </DialogTrigger>
                <DialogContent className="sm:max-w-lg">
                  <DialogHeader>
                    <DialogTitle>Transcription sources guide</DialogTitle>
                    <DialogDescription>
                      Pick where speech recognition runs before recording or uploading files.
                    </DialogDescription>
                  </DialogHeader>
                  <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                    <p><strong className="text-foreground">Local</strong> models run on this machine and keep raw audio local.</p>
                    <p><strong className="text-foreground">Cloud</strong> sources use a connected provider when you choose one.</p>
                    <p><strong className="text-foreground">Remote VoiceTypr</strong> uses another device on your network when that server is online.</p>
                  </div>
                </DialogContent>
              </Dialog>
            </div>
            <p className="mt-1 text-sm leading-6 text-muted-foreground">
              Choose local models now, cloud transcription when connected, or another VoiceTypr device on your network.
            </p>
          </div>
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-hidden">
        <ScrollArea className="h-full">
          <div className="space-y-6 px-6 py-5">
            <Card className="space-y-4 border-border/60 bg-card p-4 shadow-sm">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex flex-wrap items-center gap-2 text-sm">
                  {(hasDownloading || hasVerifying) && (
                    <Badge variant="outline" className="gap-1.5 bg-primary/10 text-primary">
                      {hasDownloading ? (
                        <Download className="size-3.5" />
                      ) : (
                        <Spinner className="size-3.5" />
                      )}
                      {hasDownloading ? "Downloading..." : "Verifying..."}
                    </Badge>
                  )}
                  {activeModelLabel ? (
                    <Badge variant="secondary" className="max-w-[320px] justify-start truncate">
                      Active: {activeModelLabel}
                    </Badge>
                  ) : (
                    availableToUse.length > 0 && (
                      <Badge variant="outline" className="border-amber-500/30 bg-amber-500/10 text-amber-700">
                        No model selected
                      </Badge>
                    )
                  )}
                </div>
                <div className="flex flex-wrap items-center gap-4 text-xs text-muted-foreground">
                  <span className="flex items-center gap-1.5">
                    <Zap className="size-3.5 text-emerald-600" />
                    Speed
                  </span>
                  <span className="flex items-center gap-1.5">
                    <CheckCircle className="size-3.5 text-blue-600" />
                    Accuracy
                  </span>
                  <span className="flex items-center gap-1.5">
                    <HardDrive className="size-3.5" />
                    Size
                  </span>
                  <span className="flex items-center gap-1.5">
                    <Star className="size-3.5 fill-amber-500 text-amber-500" />
                    Recommended
                  </span>
                </div>
              </div>

              <Field orientation="responsive" className="rounded-xl border bg-muted/30 p-4">
                <FieldContent>
                  <FieldLabel htmlFor="language">Spoken Language</FieldLabel>
                  <FieldDescription>
                    The language you speak into VoiceTypr. English-only models lock this to English.
                  </FieldDescription>
                </FieldContent>
                <LanguageSelection
                  value={languageValue}
                  engine={currentEngine}
                  englishOnly={isEnglishOnlyModel}
                  onValueChange={(value) => {
                    void handleLanguageChange(value);
                  }}
                />
              </Field>
            </Card>
            {readyLocalModels.length > 0 && (
              <section className="space-y-3">
                <div>
                  <h2 className="flex items-center gap-2 text-base font-semibold tracking-tight text-foreground">
                    <HardDrive className="size-4" />
                    Local models ({readyLocalModels.length})
                  </h2>
                  <p className="text-xs text-muted-foreground">
                    Offline transcription models stored on this machine.
                  </p>
                </div>
                <div className="grid gap-3">
                  {readyLocalModels.map(([name, model]) => (
                    <ModelCard
                      key={name}
                      name={name}
                      model={model}
                      downloadProgress={downloadProgress[name]}
                      isVerifying={verifyingModels.has(name)}
                      onDownload={onDownload}
                      onDelete={onDelete}
                      onCancelDownload={onCancelDownload}
                      onSelect={async (modelName) => {
                        await clearActiveRemote();
                        void onSelect(modelName);
                      }}
                      showSelectButton={model.downloaded}
                      isSelected={!activeRemoteServer && currentModel === name}
                    />
                  ))}
                </div>
              </section>
            )}

            {readyCloudModels.length > 0 && (
              <section className="space-y-3">
                <div>
                  <h2 className="flex items-center gap-2 text-base font-semibold tracking-tight text-foreground">
                    <Cloud className="size-4" />
                    Cloud transcription ({readyCloudModels.length})
                  </h2>
                  <p className="text-xs text-muted-foreground">
                    Connected providers that can transcribe without a local model.
                  </p>
                </div>
                <div className="grid gap-3">
                  {readyCloudModels.map(([name, model]) => renderCloudCard([name, model]))}
                </div>
              </section>
            )}

            {(setupLocalModels.length > 0 || setupCloudModels.length > 0) && (
              <section className="space-y-3">
                <div>
                  <h2 className="text-base font-semibold tracking-tight text-foreground">
                    Set up sources
                  </h2>
                  <p className="text-xs text-muted-foreground">
                    Download local models or connect cloud providers before selecting them.
                  </p>
                </div>
                {setupLocalModels.length > 0 && (
                  <div className="grid gap-3">
                    {setupLocalModels.map(([name, model]) => (
                      <ModelCard
                        key={name}
                        name={name}
                        model={model}
                        downloadProgress={downloadProgress[name]}
                        isVerifying={verifyingModels.has(name)}
                        onDownload={onDownload}
                        onDelete={onDelete}
                        onCancelDownload={onCancelDownload}
                        onSelect={async (modelName) => {
                          await clearActiveRemote();
                          void onSelect(modelName);
                        }}
                        showSelectButton={model.downloaded}
                        isSelected={!activeRemoteServer && currentModel === name}
                      />
                    ))}
                  </div>
                )}
                {setupCloudModels.length > 0 && (
                  <div className="grid gap-3">
                    {setupCloudModels.map(([name, model]) => renderCloudCard([name, model]))}
                  </div>
                )}
              </section>
            )}

            <section className="space-y-3">
              <div className="flex items-center justify-between gap-4">
                <div>
                  <h2 className="flex items-center gap-2 text-base font-semibold tracking-tight text-foreground">
                    <Server className="size-4" />
                    Remote VoiceTypr ({remoteServers.length})
                  </h2>
                  <p className="text-xs text-muted-foreground">
                    Use a stronger Mac on your network without copying audio to the cloud.
                  </p>
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setAddServerModalOpen(true)}
                >
                  <Plus className="size-4" />
                  Add Remote
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
                      onEdit={handleEditServer}
                      isRefreshing={isRefreshingServers}
                    />
                  ))}
                </div>
              ) : (
                <Empty className="border border-border/60 bg-card/70 py-8">
                  <EmptyHeader>
                    <EmptyMedia variant="icon">
                      <Server className="size-5" />
                    </EmptyMedia>
                    <EmptyTitle>No remote VoiceTyprs configured</EmptyTitle>
                    <EmptyDescription>
                      Connect another VoiceTypr device to use its local model from this machine.
                    </EmptyDescription>
                  </EmptyHeader>
                </Empty>
              )}
            </section>

            {availableToUse.length === 0 &&
              availableToSetup.length === 0 &&
              remoteServers.length === 0 && (
                <Empty className="border border-border/60 bg-card/70 py-12">
                  <EmptyHeader>
                    <EmptyMedia variant="icon">
                      <Bot className="size-5" />
                    </EmptyMedia>
                    <EmptyTitle>No models available</EmptyTitle>
                    <EmptyDescription>
                      Models will appear here when they become available.
                    </EmptyDescription>
                  </EmptyHeader>
                </Empty>
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
        onOpenChange={(open) => {
          setAddServerModalOpen(open);
          if (!open) setEditingServer(null);
        }}
        onServerAdded={handleServerAdded}
        editServer={editingServer}
      />
    </div>
  );
}
