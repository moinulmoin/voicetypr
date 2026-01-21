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
import { listen } from "@tauri-apps/api/event";
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
      const startTime = performance.now();
      console.log(`⏱️ [UI TIMING] handleSelectRemoteServer START - serverId=${serverId}`);
      try {
        const invokeStart = performance.now();
        console.log(`⏱️ [UI TIMING] Calling set_active_remote_server... (+${(invokeStart - startTime).toFixed(0)}ms)`);
        await invoke("set_active_remote_server", {
          serverId: serverId === activeRemoteServer ? null : serverId,
        });
        console.log(`⏱️ [UI TIMING] set_active_remote_server returned (+${(performance.now() - startTime).toFixed(0)}ms)`);
        setActiveRemoteServer(
          serverId === activeRemoteServer ? null : serverId
        );
        console.log(`⏱️ [UI TIMING] State updated (+${(performance.now() - startTime).toFixed(0)}ms)`);
        toast.success(
          serverId === activeRemoteServer
            ? "Remote VoiceTypr deselected"
            : "Remote VoiceTypr selected"
        );
        console.log(`⏱️ [UI TIMING] handleSelectRemoteServer COMPLETE - total: ${(performance.now() - startTime).toFixed(0)}ms`);
      } catch (error) {
        console.error("Failed to set active remote server:", error);
        console.log(`⏱️ [UI TIMING] handleSelectRemoteServer ERROR after ${(performance.now() - startTime).toFixed(0)}ms`);
        toast.error("Failed to select remote VoiceTypr");
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
                        onSelect={async (modelName) => {
                          const startTime = performance.now();
                          console.log(`⏱️ [UI TIMING] Local model onSelect START - model=${modelName}`);
                          // Clear active remote server when selecting a local model
                          if (activeRemoteServer) {
                            try {
                              console.log(`⏱️ [UI TIMING] Clearing remote server... (+${(performance.now() - startTime).toFixed(0)}ms)`);
                              await invoke("set_active_remote_server", { serverId: null });
                              console.log(`⏱️ [UI TIMING] Remote server cleared (+${(performance.now() - startTime).toFixed(0)}ms)`);
                              setActiveRemoteServer(null);
                            } catch (error) {
                              console.error("Failed to clear active remote server:", error);
                            }
                          }
                          console.log(`⏱️ [UI TIMING] Calling onSelect... (+${(performance.now() - startTime).toFixed(0)}ms)`);
                          void onSelect(modelName);
                          console.log(`⏱️ [UI TIMING] Local model onSelect COMPLETE - total: ${(performance.now() - startTime).toFixed(0)}ms`);
                        }}
                        showSelectButton={model.downloaded}
                        isSelected={!activeRemoteServer && currentModel === name}
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
                        onSelect={async (modelName) => {
                          const startTime = performance.now();
                          console.log(`⏱️ [UI TIMING] Local model onSelect START - model=${modelName}`);
                          // Clear active remote server when selecting a local model
                          if (activeRemoteServer) {
                            try {
                              console.log(`⏱️ [UI TIMING] Clearing remote server... (+${(performance.now() - startTime).toFixed(0)}ms)`);
                              await invoke("set_active_remote_server", { serverId: null });
                              console.log(`⏱️ [UI TIMING] Remote server cleared (+${(performance.now() - startTime).toFixed(0)}ms)`);
                              setActiveRemoteServer(null);
                            } catch (error) {
                              console.error("Failed to clear active remote server:", error);
                            }
                          }
                          console.log(`⏱️ [UI TIMING] Calling onSelect... (+${(performance.now() - startTime).toFixed(0)}ms)`);
                          void onSelect(modelName);
                          console.log(`⏱️ [UI TIMING] Local model onSelect COMPLETE - total: ${(performance.now() - startTime).toFixed(0)}ms`);
                        }}
                        showSelectButton={model.downloaded}
                        isSelected={!activeRemoteServer && currentModel === name}
                      />
                    ) : (
                      renderCloudCard([name, model])
                    ),
                  )}
                </div>
              </div>
            )}

            {/* Remote VoiceTypr Models Section */}
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <h2 className="text-base font-semibold text-foreground flex items-center gap-2">
                  <Server className="h-4 w-4" />
                  Remote VoiceTypr Models ({remoteServers.length})
                </h2>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setAddServerModalOpen(true)}
                >
                  <Plus className="h-4 w-4 mr-1" />
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
                <Card className="px-4 py-6 border-border/50 border-dashed">
                  <div className="text-center text-muted-foreground">
                    <Server className="h-8 w-8 mx-auto mb-2 opacity-50" />
                    <p className="text-sm">No remote VoiceTyprs configured</p>
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
