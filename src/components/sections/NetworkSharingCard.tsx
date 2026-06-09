import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useSettings } from "@/contexts/SettingsContext";
import { isMacOS, isWindows } from "@/lib/platform";
import { getModelDisplayName } from "@/lib/model-display";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, Check, CheckCircle, Copy, Eye, EyeOff, ExternalLink, Network, Server, Shield, XCircle } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

interface BindingResult {
  ip: string;
  success: boolean;
  error: string | null;
  interface_name?: string;
}

interface SharingStatus {
  enabled: boolean;
  port: number | null;
  model_name: string | null;
  server_name: string | null;
  active_connections: number;
  password_configured: boolean;
  binding_results: BindingResult[];
  allow_model_control: boolean;
}

interface FirewallStatus {
  firewall_enabled: boolean;
  app_allowed: boolean;
  may_be_blocked: boolean;
}

interface ModelInfo {
  name: string;
  display_name: string;
  downloaded: boolean;
  engine?: string;
  kind?: string;
}

interface ModelStatusResponse {
  models: ModelInfo[];
}

const MIN_SHARING_PORT = 1;
const MAX_SHARING_PORT = 65535;
const NO_NETWORK_SENTINEL = "No network connection";

function isShareableEngine(engine?: string | null): boolean {
  return engine === "whisper" || engine === "parakeet";
}

function isShareableModel(model: ModelInfo): boolean {
  return (
    model.downloaded &&
    model.kind !== "cloud" &&
    isShareableEngine(model.engine)
  );
}

function parseSharingPort(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed || !/^\d+$/.test(trimmed)) return null;
  const port = Number(trimmed);
  if (!Number.isInteger(port) || port < MIN_SHARING_PORT || port > MAX_SHARING_PORT) {
    return null;
  }
  return port;
}

function bindingResultsFromLocalIps(localIps: string[]): BindingResult[] {
  return localIps
    .filter((entry) => entry && entry !== NO_NETWORK_SENTINEL)
    .map((entry) => {
      const match = entry.match(/^(.*?) \((.*?)\)$/);

      return {
        ip: match?.[1] ?? entry,
        success: true,
        error: null,
        interface_name: match?.[2],
      };
    });
}

export function NetworkSharingCard() {
  const { settings, updateSettings } = useSettings();
  const [status, setStatus] = useState<SharingStatus>({
    enabled: false,
    port: null,
    model_name: null,
    server_name: null,
    active_connections: 0,
    password_configured: false,
    binding_results: [],
    allow_model_control: false,
  });
  const [showPassword, setShowPassword] = useState(false);
  const [port, setPort] = useState("47842");
  const [password, setPassword] = useState("");
  const [savedPort, setSavedPort] = useState("47842");
  const [savedPassword, setSavedPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [savingPort, setSavingPort] = useState(false);
  const [savingPassword, setSavingPassword] = useState(false);
  const [savingModelControl, setSavingModelControl] = useState(false);
  const [modelDisplayName, setModelDisplayName] = useState<string | null>(null);
  const [hasShareableModel, setHasShareableModel] = useState<boolean>(true);
  const [currentSelectionShareable, setCurrentSelectionShareable] = useState<boolean>(true);
  const [activeRemoteServer, setActiveRemoteServer] = useState<string | null>(null);
  const [firewallStatus, setFirewallStatus] = useState<FirewallStatus | null>(null);

  const currentModel = settings?.current_model;
  const currentEngine = settings?.current_model_engine ?? "whisper";
  const previousLocalSelectionRef = useRef<{
    model?: string | null;
    engine?: string | null;
  } | null>(null);
  const sharedModelDisplayName =
    status.enabled && status.model_name
      ? (getModelDisplayName(status.model_name) ?? status.model_name)
      : modelDisplayName;

  // Fetch current sharing status
  const fetchStatus = useCallback(async () => {
    try {
      const result = await invoke<SharingStatus>("get_sharing_status");
      let bindingResults = result.binding_results ?? [];

      if (result.enabled && bindingResults.length === 0) {
        try {
          const localIps = await invoke<string[]>("get_local_ips");
          bindingResults = bindingResultsFromLocalIps(localIps);
        } catch (error) {
          console.error("Failed to get local IPs:", error);
        }
      }

      const normalizedResult = {
        ...result,
        binding_results: bindingResults,
      };
      setStatus(normalizedResult);
      // Only update port/password from server status when sharing is enabled
      // When disabled, we rely on persisted settings to preserve values
      if (normalizedResult.enabled) {
        if (normalizedResult.port) {
          const portStr = normalizedResult.port.toString();
          setPort(portStr);
          setSavedPort(portStr);
        }
      }
    } catch (error) {
      console.error("Failed to get sharing status:", error);
    }
  }, []);

  // Fetch active remote server
  const fetchActiveRemoteServer = useCallback(async () => {
    try {
      const activeId = await invoke<string | null>("get_active_remote_server");
      setActiveRemoteServer(activeId);
    } catch (error) {
      console.error("Failed to get active remote server:", error);
    }
  }, []);

  // Fetch firewall status (macOS and Windows)
  const fetchFirewallStatus = useCallback(async () => {
    try {
      const result = await invoke<FirewallStatus>("get_firewall_status");
      setFirewallStatus(result);
    } catch (error) {
      console.error("Failed to get firewall status:", error);
      setFirewallStatus(null);
    }
  }, []);

  // Fetch available model info and get display name for current selection
  const fetchModelInfo = useCallback(async () => {
    try {
      const response = await invoke<ModelStatusResponse>("get_model_status");
      const models = response.models || [];
      const shareableModels = models.filter(isShareableModel);
      setHasShareableModel(shareableModels.length > 0);

      const selectedModel = currentModel
        ? models.find((m) => m.name === currentModel)
        : null;
      const selectionShareable = selectedModel
        ? isShareableModel(selectedModel)
        : isShareableEngine(currentEngine) && shareableModels.length > 0;
      setCurrentSelectionShareable(selectionShareable);

      if (currentModel && selectionShareable) {
        const selected = shareableModels.find((m) => m.name === currentModel);
        setModelDisplayName(
          selected?.display_name || getModelDisplayName(currentModel) || currentModel,
        );
      } else if (shareableModels.length > 0) {
        setModelDisplayName(shareableModels[0].display_name);
      } else {
        setModelDisplayName(null);
      }
    } catch (error) {
      console.error("Failed to get model status:", error);
      setHasShareableModel(false);
      setCurrentSelectionShareable(false);
    }
  }, [currentModel, currentEngine]);

  // Fetch status, model info, remote server state, and firewall status on mount
  useEffect(() => {
    fetchStatus();
    fetchModelInfo();
    fetchActiveRemoteServer();
    fetchFirewallStatus();
  }, [fetchStatus, fetchModelInfo, fetchActiveRemoteServer, fetchFirewallStatus]);

  // Refetch model info when current model changes
  useEffect(() => {
    fetchModelInfo();
  }, [currentModel, fetchModelInfo]);

  // Listen for sharing status changes from backend (e.g., tray menu actions)
  useEffect(() => {
    const setupListener = async () => {
      const unlisten = await listen("sharing-status-changed", () => {
        console.log("[NetworkSharingCard] Received sharing-status-changed event, refreshing status...");
        fetchStatus();
        fetchActiveRemoteServer(); // Also refresh remote server state in case it changed
      });
      return unlisten;
    };

    const unlistenPromise = setupListener();

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [fetchStatus, fetchActiveRemoteServer]);

  // Load persisted port and password from settings
  useEffect(() => {
    if (settings) {
      if (settings.sharing_port) {
        const portStr = settings.sharing_port.toString();
        setPort(portStr);
        setSavedPort(portStr);
      }
    }
  }, [settings?.sharing_port]);

  // Auto-restart sharing only after the local model selection changes.
  useEffect(() => {
    if (!settings) return;
    const previousSelection = previousLocalSelectionRef.current;
    const nextSelection = {
      model: currentModel,
      engine: currentEngine,
    };

    previousLocalSelectionRef.current = nextSelection;

    if (!previousSelection) return;
    if (
      previousSelection.model === nextSelection.model &&
      previousSelection.engine === nextSelection.engine
    ) {
      return;
    }
    if (!status.enabled || !currentModel) return;
    if (previousSelection.model !== currentModel && status.model_name === currentModel) return;

    const autoRestartSharing = async () => {
      console.log(`[Remote Transcription] Local model changed to ${currentModel}, restarting sharing...`);

      const validatedPort = parseSharingPort(port);
      if (!validatedPort) {
        toast.error(`Enter a valid port between ${MIN_SHARING_PORT} and ${MAX_SHARING_PORT}`);
        await fetchStatus();
        return;
      }

      try {
        await invoke("stop_sharing");
        await invoke("start_sharing", {
          port: validatedPort,
          password: null,
          preservePassword: true,
          serverName: null,
        });
        await fetchStatus();
        toast.success(`Remote transcription now uses ${modelDisplayName ?? getModelDisplayName(currentModel) ?? currentModel}`);
      } catch (error) {
        console.error("Failed to restart remote transcription with new model:", error);
        toast.error("Failed to switch remote transcription model");
        await fetchStatus();
      }
    };

    autoRestartSharing();
  }, [settings, currentModel, currentEngine, status.enabled, status.model_name, port, fetchStatus, modelDisplayName]);


  const handleToggleModelControl = async (checked: boolean) => {
    setSavingModelControl(true);
    try {
      await invoke("update_remote_model_control_enabled", { enabled: checked });
      setStatus((current) => ({ ...current, allow_model_control: checked }));
      toast.success(
        checked
          ? "Host model changes enabled for trusted devices"
          : "Host model changes disabled",
      );
    } catch (error) {
      console.error("Failed to update remote model control setting:", error);
      toast.error("Failed to update remote model control setting");
      await fetchStatus();
    } finally {
      setSavingModelControl(false);
    }
  };

  const handleToggleSharing = async (checked: boolean) => {
    setLoading(true);
    try {
      if (checked) {
        const validatedPort = parseSharingPort(port);
        if (!validatedPort) {
          toast.error(`Enter a valid port between ${MIN_SHARING_PORT} and ${MAX_SHARING_PORT}`);
          return;
        }

        await invoke("start_sharing", {
          port: validatedPort,
          password: password || null,
          preservePassword: !password,
          serverName: null, // Use hostname
        });
        toast.success("Remote transcription enabled");
        fetchFirewallStatus();
      } else {
        await invoke("stop_sharing");
        toast.success("Remote transcription disabled");
      }
      await fetchStatus();
    } catch (error) {
      console.error("Failed to toggle sharing:", error);
      const errorMessage =
        error instanceof Error ? error.message : String(error);
      toast.error(errorMessage || "Failed to toggle remote transcription");
      await fetchStatus();
    } finally {
      setLoading(false);
    }
  };

  const restorePreviousSharing = async (): Promise<boolean> => {
    const previousPort = parseSharingPort(savedPort);
    if (!previousPort) return false;

    await invoke("start_sharing", {
      port: previousPort,
      password: savedPassword || null,
      preservePassword: !savedPassword,
      serverName: null,
    });
    return true;
  };

  // Save port and restart server
  const handleSavePort = async () => {
    if (!status.enabled) return;

    const validatedPort = parseSharingPort(port);
    if (!validatedPort) {
      toast.error(`Enter a valid port between ${MIN_SHARING_PORT} and ${MAX_SHARING_PORT}`);
      return;
    }

    setSavingPort(true);
    try {
      await invoke("stop_sharing");
      await invoke("start_sharing", {
        port: validatedPort,
        password: null,
        preservePassword: true,
        serverName: null,
      });
      setSavedPort(port);
      await updateSettings({ sharing_port: validatedPort });
      await fetchStatus();
      toast.success(`Port changed to ${port}`);
    } catch (error) {
      console.error("Failed to update port:", error);
      setPort(savedPort);
      try {
        await restorePreviousSharing();
        toast.error("Failed to update port; sharing restored");
      } catch (restoreError) {
        console.error("Failed to restore sharing after port change:", restoreError);
        toast.error("Failed to update port and could not restore sharing");
      }
      await fetchStatus();
    } finally {
      setSavingPort(false);
    }
  };

  // Save password and restart server
  const handleSavePassword = async () => {
    if (!status.enabled) return;

    const validatedPort = parseSharingPort(savedPort);
    if (!validatedPort) {
      toast.error(`Enter a valid port between ${MIN_SHARING_PORT} and ${MAX_SHARING_PORT}`);
      return;
    }

    let disabledModelControl = false;

    setSavingPassword(true);
    try {
      if (!password && status.allow_model_control) {
        await invoke("update_remote_model_control_enabled", { enabled: false });
        disabledModelControl = true;
      }
      await invoke("stop_sharing");
      await invoke("start_sharing", {
        port: validatedPort,
        password: password || null,
        preservePassword: false,
        serverName: null,
      });
      if (disabledModelControl) {
        setStatus((current) => ({ ...current, allow_model_control: false }));
      }
      setSavedPassword(password);
      await fetchStatus();
      toast.success(password ? "Password updated" : "Password removed");
    } catch (error) {
      console.error("Failed to update password:", error);
      setPassword(savedPassword);
      try {
        await restorePreviousSharing();
      } catch (restoreError) {
        console.error("Failed to restore sharing after password change:", restoreError);
        toast.error("Failed to update password and could not restore sharing");
        await fetchStatus();
        return;
      }

      if (disabledModelControl) {
        try {
          await invoke("update_remote_model_control_enabled", { enabled: true });
          setStatus((current) => ({ ...current, allow_model_control: true }));
        } catch (restoreModelControlError) {
          console.error("Failed to restore remote model control:", restoreModelControlError);
          toast.error("Failed to update password; sharing restored without remote model changes");
          await fetchStatus();
          return;
        }
      }

      toast.error("Failed to update password; sharing restored");
      await fetchStatus();
    } finally {
      setSavingPassword(false);
    }
  };

  const copyAddress = (ip: string) => {
    // Extract just the IP from "192.168.1.1 (eth0)" format
    const justIp = ip.split(" ")[0];
    const address = `${justIp}:${savedPort}`;
    navigator.clipboard.writeText(address);
    toast.success("Address copied to clipboard");
  };

  const reachableBindings = status.binding_results.filter(
    (result) =>
      result.success &&
      result.ip !== "127.0.0.1" &&
      result.ip !== NO_NETWORK_SENTINEL,
  );

  return (
    <div className="rounded-lg border border-border/50 bg-card">
      {/* Header with Toggle */}
      <div className="px-4 py-3 border-b border-border/50">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <div className="p-1.5 rounded-md bg-primary/10">
              <Network className="h-4 w-4 text-primary" />
            </div>
            <div>
              <h3 className="font-medium">Remote Transcription</h3>
              <p className="text-xs text-muted-foreground">
                Use this device's transcription from another VoiceTypr app
              </p>
            </div>
          </div>
          <Switch
            id="network-sharing"
            checked={status.enabled}
            onCheckedChange={handleToggleSharing}
            disabled={loading || (!status.enabled && (!!activeRemoteServer || !hasShareableModel || !currentSelectionShareable))}
          />
        </div>
      </div>

      {/* Warning if no model downloaded */}
      {!hasShareableModel && !status.enabled && (
        <div className="px-4 py-3">
          <div className="flex items-start gap-2 p-3 rounded-lg bg-amber-500/10 border border-amber-500/20">
            <AlertTriangle className="h-4 w-4 text-amber-500 mt-0.5 flex-shrink-0" />
            <div>
              <p className="text-sm font-medium text-amber-700 dark:text-amber-400">
                No shareable local model
              </p>
              <p className="text-xs text-amber-600 dark:text-amber-500">
                Remote sharing requires a downloaded Whisper or Parakeet model on this device.
              </p>
            </div>
          </div>
        </div>
      )}

      {hasShareableModel && !currentSelectionShareable && !status.enabled && !activeRemoteServer && (
        <div className="px-4 py-3">
          <div className="flex items-start gap-2 p-3 rounded-lg bg-amber-500/10 border border-amber-500/20">
            <AlertTriangle className="h-4 w-4 text-amber-500 mt-0.5 flex-shrink-0" />
            <div>
              <p className="text-sm font-medium text-amber-700 dark:text-amber-400">
                Current model cannot be shared
              </p>
              <p className="text-xs text-amber-600 dark:text-amber-500">
                Soniox and cloud sources cannot be shared over the network. Select a downloaded Whisper or Parakeet model in the Models tab to enable sharing.
              </p>
            </div>
          </div>
        </div>
      )}

      {/* Warning if using remote server */}
      {activeRemoteServer && !status.enabled && (
        <div className="px-4 py-3">
          <div className="flex items-start gap-2 p-3 rounded-lg bg-blue-500/10 border border-blue-500/20">
            <Network className="h-4 w-4 text-blue-500 mt-0.5 flex-shrink-0" />
            <div>
              <p className="text-sm font-medium text-blue-700 dark:text-blue-400">
                Using remote VoiceTypr
              </p>
              <p className="text-xs text-blue-600 dark:text-blue-500">
                Remote transcription is unavailable while using another VoiceTypr device as your model source.
              </p>
            </div>
          </div>
        </div>
      )}

      {/* Model info when disabled but model available - hide when using remote server */}
      {hasShareableModel && currentSelectionShareable && !status.enabled && modelDisplayName && !activeRemoteServer && (
        <div className="px-4 py-3">
          <p className="text-xs text-muted-foreground">
            When enabled, another VoiceTypr app can use this device's{" "}
            <span className="font-medium text-foreground">{modelDisplayName}</span>{" "}
            model for transcription.
          </p>
        </div>
      )}

      {/* Sub-settings - only show when enabled */}
      {status.enabled && (
        <div className="mx-3 mb-3 mt-0 rounded-lg bg-muted/20">
          <div className="p-4 space-y-4">
            {/* Status Display */}
            <div className="flex items-center gap-2 p-3 rounded-lg bg-green-500/10 border border-green-500/20">
              <Server className="h-4 w-4 text-green-500" />
              <div className="flex-1">
                <p className="text-sm font-medium text-green-700 dark:text-green-400">
                  Ready for remote transcription
                </p>
                <p className="text-xs text-muted-foreground">
                  {sharedModelDisplayName
                    ? `Model: ${sharedModelDisplayName}`
                    : "No model selected"}
                </p>
              </div>
            </div>

            {/* Firewall Warning */}
            {firewallStatus?.may_be_blocked && (
              <div className="flex items-start gap-2 p-3 rounded-lg bg-amber-500/10 border border-amber-500/20">
                <Shield className="h-4 w-4 text-amber-500 mt-0.5 flex-shrink-0" />
                <div className="flex-1">
                  <p className="text-sm font-medium text-amber-700 dark:text-amber-400">
                    Firewall may block connections
                  </p>
                  {isMacOS && (
                    <>
                      <p className="text-xs text-amber-600 dark:text-amber-500 mb-2">
                        Your macOS firewall is enabled. To allow other devices to connect:
                      </p>
                      <ol className="text-xs text-amber-600 dark:text-amber-500 mb-2 list-decimal list-inside space-y-0.5">
                        <li>Open <strong>System Settings → Network → Firewall</strong></li>
                        <li>Click <strong>Options...</strong></li>
                        <li>Click the <strong>+</strong> button at the bottom of the app list</li>
                        <li>Navigate to <strong>Applications</strong> and select <strong>VoiceTypr</strong></li>
                        <li>Ensure it's set to <strong>Allow incoming connections</strong></li>
                      </ol>
                    </>
                  )}
                  {isWindows && (
                    <>
                      <p className="text-xs text-amber-600 dark:text-amber-500 mb-2">
                        Windows Firewall may be blocking incoming connections. To allow other devices to connect:
                      </p>
                      <ol className="text-xs text-amber-600 dark:text-amber-500 mb-2 list-decimal list-inside space-y-0.5">
                        <li>Open <strong>Windows Firewall</strong> settings</li>
                        <li>Click <strong>Allow an app through firewall</strong></li>
                        <li>Click <strong>Change settings</strong> (may require admin)</li>
                        <li>Click <strong>Allow another app...</strong></li>
                        <li>Browse to and select <strong>VoiceTypr</strong></li>
                        <li>Check both <strong>Private</strong> and <strong>Public</strong> networks</li>
                      </ol>
                    </>
                  )}
                  {!isMacOS && !isWindows && (
                    <p className="text-xs text-amber-600 dark:text-amber-500 mb-2">
                      Your firewall may be blocking incoming connections. Please configure your firewall to allow VoiceTypr.
                    </p>
                  )}
                  <div className="flex items-center gap-3">
                    <button
                      onClick={async () => {
                        try {
                          await invoke("open_firewall_settings");
                        } catch (error) {
                          console.error("Failed to open firewall settings:", error);
                          const settingsPath = isMacOS
                            ? "System Settings > Network > Firewall"
                            : isWindows
                              ? "Control Panel > Windows Firewall"
                              : "your firewall settings";
                          toast.error(`Could not open Firewall settings. Please open ${settingsPath} manually.`);
                        }
                      }}
                      className="inline-flex items-center gap-1.5 text-xs font-medium text-amber-700 dark:text-amber-400 hover:underline"
                    >
                      <ExternalLink className="h-3 w-3" />
                      {isMacOS ? "Open System Settings" : isWindows ? "Open Windows Firewall" : "Open Firewall Settings"}
                    </button>
                    <button
                      onClick={async () => {
                        toast.info("Checking firewall status...");
                        await fetchFirewallStatus();
                      }}
                      className="text-xs text-amber-600 dark:text-amber-500 hover:underline"
                    >
                      Check again
                    </button>
                  </div>
                </div>
              </div>
            )}

            {/* Connection Info Section */}
            <div className="space-y-2">
              <Label className="text-sm font-medium">Connect from another device</Label>
              <div className="space-y-1">
                {reachableBindings.length === 0 ? (
                  <div className="rounded-md border border-amber-500/20 bg-amber-500/10 px-3 py-2 text-sm text-amber-700 dark:text-amber-400">
                    <p className="font-medium">No network address available</p>
                    <p className="mt-1 text-xs text-amber-600 dark:text-amber-500">
                      Connect this device to Wi-Fi or Ethernet so other VoiceTypr apps can reach it.
                    </p>
                  </div>
                ) : (
                  <>
                    {/* Show successful bindings first (exclude localhost - can't connect to self) */}
                    {reachableBindings.map((result, index) => (
                        <div key={`success-${index}`} className="flex items-center gap-2">
                          <CheckCircle className="h-4 w-4 text-green-500 flex-shrink-0" />
                          <div className="flex-1 px-3 py-2 rounded-md bg-background/60 border border-border/60 font-mono text-sm">
                            <span className="font-semibold">{result.ip}:{port}</span>
                            {result.interface_name && (
                              <span className="ml-2 text-xs text-muted-foreground">
                                ({result.interface_name})
                              </span>
                            )}
                          </div>
                          <button
                            onClick={() => copyAddress(result.ip)}
                            className="p-2 rounded-md border border-transparent text-muted-foreground hover:bg-accent hover:text-accent-foreground hover:border-border/50 active:bg-accent/80 active:scale-95 transition-all duration-150"
                            title="Copy address"
                            aria-label={`Copy address ${result.ip}:${port}`}
                          >
                            <Copy className="h-4 w-4" />
                          </button>
                        </div>
                      ))}
                    {/* Show failed bindings with tooltip (exclude localhost) */}
                    {status.binding_results
                      .filter((result) => !result.success && result.ip !== "127.0.0.1")
                      .map((result, index) => (
                        <div
                          key={`failed-${index}`}
                          className="flex items-center gap-2 opacity-50"
                          title={result.error || "Could not use this address"}
                        >
                          <XCircle className="h-4 w-4 text-red-400 flex-shrink-0" />
                          <div className="flex-1 px-3 py-2 rounded-md bg-muted/30 border border-border/30 font-mono text-sm text-muted-foreground">
                            <span>{result.ip}:{port}</span>
                            {result.interface_name && (
                              <span className="ml-2 text-xs text-muted-foreground">
                                ({result.interface_name})
                              </span>
                            )}
                            <span className="ml-2 text-xs text-red-400">(could not use this address)</span>
                          </div>
                        </div>
                      ))}
                  </>
                )}
              </div>
              {reachableBindings.length > 0 && (
                <p className="text-xs text-muted-foreground">
                  Enter one of these addresses in VoiceTypr on another device on the same network.
                </p>
              )}
              {status.binding_results.filter((r) => !r.success && r.ip !== "127.0.0.1").length > 0 && (
                <p className="text-xs text-amber-500">
                  Some addresses could not be used - hover for details
                </p>
              )}
            </div>

            {/* Connection Settings Section */}
            <div className="rounded-lg border border-border/50 bg-background/50 p-3 space-y-3">
              <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                Connection Settings
              </p>

              {/* Port Setting */}
              <div className="space-y-1.5">
                <Label htmlFor="sharing-port" className="text-sm">
                  Port
                </Label>
                <div className="flex items-center gap-2">
                  <Input
                    id="sharing-port"
                    type="number"
                    value={port}
                    onChange={(e) => setPort(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && port !== savedPort) {
                        handleSavePort();
                      }
                    }}
                    placeholder="47842"
                    className="font-mono h-9 flex-1"
                  />
                  {status.enabled && port !== savedPort && (
                    <button
                      onClick={handleSavePort}
                      disabled={savingPort}
                      className="p-2 rounded-md bg-green-500/10 text-green-600 hover:bg-green-500/20 disabled:opacity-50 transition-colors"
                      title="Save and restart server"
                    >
                      <Check className="h-4 w-4" />
                    </button>
                  )}
                </div>
                <p className="text-xs text-muted-foreground">
                  Default: 47842. {status.enabled && port !== savedPort ? "Click checkmark to apply." : ""}
                </p>
              </div>

              {/* Password Setting */}
              <div className="space-y-1.5">
                <Label htmlFor="sharing-password" className="text-sm">
                  Password (Optional)
                </Label>
                <p className="text-xs text-muted-foreground">
                  Other devices need this password to connect to your shared transcription.
                </p>
                <div className="flex items-center gap-2">
                  <div className="relative flex-1">
                    <Input
                      id="sharing-password"
                      type={showPassword ? "text" : "password"}
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" && password !== savedPassword) {
                          handleSavePassword();
                        }
                      }}
                      placeholder={status.password_configured ? "Password saved" : "No password"}
                      className="h-9 pr-10 [&::-ms-reveal]:hidden [&::-webkit-credentials-auto-fill-button]:hidden"
                    />
                    <button
                      type="button"
                      onClick={() => setShowPassword(!showPassword)}
                      className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-muted-foreground hover:text-foreground transition-colors"
                      tabIndex={-1}
                    >
                      {showPassword ? (
                        <EyeOff className="h-4 w-4" />
                      ) : (
                        <Eye className="h-4 w-4" />
                      )}
                    </button>
                  </div>
                  {status.enabled && password !== savedPassword && (
                    <button
                      onClick={handleSavePassword}
                      disabled={savingPassword}
                      className="p-2 rounded-md bg-green-500/10 text-green-600 hover:bg-green-500/20 disabled:opacity-50 transition-colors"
                      title="Save password"
                    >
                      <Check className="h-4 w-4" />
                    </button>
                  )}
                  {status.enabled && status.password_configured && !password && (
                    <button
                      onClick={handleSavePassword}
                      disabled={savingPassword}
                      className="px-3 py-2 rounded-md border border-destructive/30 text-xs font-medium text-destructive hover:bg-destructive/10 disabled:opacity-50 transition-colors"
                      title="Remove saved password"
                    >
                      Remove
                    </button>
                  )}
                </div>
              </div>

              {/* Remote model control opt-in */}
              <div className="space-y-1.5">
                <div className="flex items-start justify-between gap-3">
                  <div className="space-y-1">
                    <Label htmlFor="allow-model-control" className="text-sm">
                      Allow trusted devices to change shared model
                    </Label>
                    <p className="text-xs text-muted-foreground">
                      Requires a sharing password so only trusted devices can change the model this device shares.
                    </p>
                  </div>
                  <Switch
                    id="allow-model-control"
                    checked={status.allow_model_control}
                    onCheckedChange={(checked) => {
                      void handleToggleModelControl(checked);
                    }}
                    disabled={savingModelControl || !status.password_configured}
                  />
                </div>
                {!status.password_configured && (
                  <p className="text-xs text-muted-foreground">
                    Add a sharing password first so only trusted devices can change the shared model.
                  </p>
                )}
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
