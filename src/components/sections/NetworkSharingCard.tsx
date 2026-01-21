import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useSettings } from "@/contexts/SettingsContext";
import { isMacOS, isWindows } from "@/lib/platform";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, Check, CheckCircle, Copy, Eye, EyeOff, ExternalLink, Network, Server, Shield, XCircle } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

interface BindingResult {
  ip: string;
  success: boolean;
  error: string | null;
}

interface SharingStatus {
  enabled: boolean;
  port: number | null;
  model_name: string | null;
  server_name: string | null;
  active_connections: number;
  password: string | null;
  binding_results: BindingResult[];
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
}

interface ModelStatusResponse {
  models: ModelInfo[];
}

export function NetworkSharingCard() {
  const { settings, updateSettings } = useSettings();
  const [status, setStatus] = useState<SharingStatus>({
    enabled: false,
    port: null,
    model_name: null,
    server_name: null,
    active_connections: 0,
    password: null,
    binding_results: [],
  });
  const [showPassword, setShowPassword] = useState(false);
  const [port, setPort] = useState("47842");
  const [password, setPassword] = useState("");
  const [savedPort, setSavedPort] = useState("47842");
  const [savedPassword, setSavedPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [savingPort, setSavingPort] = useState(false);
  const [savingPassword, setSavingPassword] = useState(false);
  const [modelDisplayName, setModelDisplayName] = useState<string | null>(null);
  const [hasDownloadedModel, setHasDownloadedModel] = useState<boolean>(true);
  const [activeRemoteServer, setActiveRemoteServer] = useState<string | null>(null);
  const [firewallStatus, setFirewallStatus] = useState<FirewallStatus | null>(null);

  const currentModel = settings?.current_model;

  // Fetch current sharing status
  const fetchStatus = useCallback(async () => {
    try {
      const result = await invoke<SharingStatus>("get_sharing_status");
      setStatus(result);
      // Only update port/password from server status when sharing is enabled
      // When disabled, we rely on persisted settings to preserve values
      if (result.enabled) {
        if (result.port) {
          const portStr = result.port.toString();
          setPort(portStr);
          setSavedPort(portStr);
        }
        // Populate password from backend when server is running
        const pwd = result.password || "";
        setPassword(pwd);
        setSavedPassword(pwd);
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
      const downloaded = models.filter((m) => m.downloaded);
      setHasDownloadedModel(downloaded.length > 0);

      // Find display name for the currently selected model
      if (currentModel && downloaded.length > 0) {
        const selected = downloaded.find((m) => m.name === currentModel);
        setModelDisplayName(selected?.display_name || currentModel);
      } else if (downloaded.length > 0) {
        // Fallback to first downloaded model
        setModelDisplayName(downloaded[0].display_name);
      } else {
        setModelDisplayName(null);
      }
    } catch (error) {
      console.error("Failed to get model status:", error);
      setHasDownloadedModel(false);
    }
  }, [currentModel]);

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
      if (settings.sharing_password !== undefined) {
        setPassword(settings.sharing_password || "");
        setSavedPassword(settings.sharing_password || "");
      }
    }
  }, [settings?.sharing_port, settings?.sharing_password]);

  // Auto-restart sharing when model selection changes
  useEffect(() => {
    const autoRestartSharing = async () => {
      // Only auto-restart if sharing is enabled and model has changed
      if (!status.enabled || !status.model_name || !currentModel) return;
      if (status.model_name === currentModel) return;

      console.log(`[Network Sharing] Model changed from ${status.model_name} to ${currentModel}, restarting...`);

      try {
        await invoke("stop_sharing");
        await invoke("start_sharing", {
          port: parseInt(port, 10),
          password: password || null,
          serverName: null,
        });
        await fetchStatus();
        toast.success(`Now sharing ${currentModel}`);
      } catch (error) {
        console.error("Failed to restart sharing with new model:", error);
        toast.error("Failed to switch sharing model");
      }
    };

    autoRestartSharing();
  }, [currentModel, status.enabled, status.model_name, port, password, fetchStatus]);

  const handleToggleSharing = async (checked: boolean) => {
    setLoading(true);
    try {
      if (checked) {
        await invoke("start_sharing", {
          port: parseInt(port, 10),
          password: password || null,
          serverName: null, // Use hostname
        });
        toast.success("Network sharing enabled");
        // Re-fetch firewall status when enabling sharing
        fetchFirewallStatus();
      } else {
        await invoke("stop_sharing");
        toast.success("Network sharing disabled");
      }
      await fetchStatus();
    } catch (error) {
      console.error("Failed to toggle sharing:", error);
      const errorMessage =
        error instanceof Error ? error.message : String(error);
      toast.error(errorMessage || "Failed to toggle sharing");
    } finally {
      setLoading(false);
    }
  };

  // Save port and restart server
  const handleSavePort = async () => {
    if (!status.enabled) return;
    setSavingPort(true);
    try {
      await invoke("stop_sharing");
      await invoke("start_sharing", {
        port: parseInt(port, 10),
        password: savedPassword || null,
        serverName: null,
      });
      setSavedPort(port);
      // Persist to settings
      await updateSettings({ sharing_port: parseInt(port, 10) });
      await fetchStatus();
      toast.success(`Port changed to ${port}`);
    } catch (error) {
      console.error("Failed to update port:", error);
      toast.error("Failed to update port");
      setPort(savedPort); // Revert on error
    } finally {
      setSavingPort(false);
    }
  };

  // Save password and restart server
  const handleSavePassword = async () => {
    if (!status.enabled) return;
    setSavingPassword(true);
    try {
      await invoke("stop_sharing");
      await invoke("start_sharing", {
        port: parseInt(savedPort, 10),
        password: password || null,
        serverName: null,
      });
      setSavedPassword(password);
      // Persist to settings
      await updateSettings({ sharing_password: password || undefined });
      await fetchStatus();
      toast.success(password ? "Password updated" : "Password removed");
    } catch (error) {
      console.error("Failed to update password:", error);
      toast.error("Failed to update password");
      setPassword(savedPassword); // Revert on error
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

  return (
    <div className="rounded-lg border border-border/50 bg-card">
      {/* Header with Toggle */}
      <div className="px-4 py-3 border-b border-border/50">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <div className="p-1.5 rounded-md bg-blue-500/10">
              <Network className="h-4 w-4 text-blue-500" />
            </div>
            <div>
              <h3 className="font-medium">Network Sharing</h3>
              <p className="text-xs text-muted-foreground">
                Share your transcription model with other devices
              </p>
            </div>
          </div>
          <Switch
            id="network-sharing"
            checked={status.enabled}
            onCheckedChange={handleToggleSharing}
            disabled={loading || (!status.enabled && (!hasDownloadedModel || !!activeRemoteServer))}
          />
        </div>
      </div>

      {/* Warning if no model downloaded */}
      {!hasDownloadedModel && !status.enabled && (
        <div className="px-4 py-3">
          <div className="flex items-start gap-2 p-3 rounded-lg bg-amber-500/10 border border-amber-500/20">
            <AlertTriangle className="h-4 w-4 text-amber-500 mt-0.5 flex-shrink-0" />
            <div>
              <p className="text-sm font-medium text-amber-700 dark:text-amber-400">
                No model downloaded
              </p>
              <p className="text-xs text-amber-600 dark:text-amber-500">
                Download a transcription model in the Models tab to enable network sharing.
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
                Network sharing is unavailable while using a remote VoiceTypr instance as your model source.
              </p>
            </div>
          </div>
        </div>
      )}

      {/* Model info when disabled but model available - hide when using remote server */}
      {hasDownloadedModel && !status.enabled && modelDisplayName && !activeRemoteServer && (
        <div className="px-4 py-3">
          <p className="text-xs text-muted-foreground">
            When enabled, other VoiceTypr instances on your network can use your{" "}
            <span className="font-medium text-foreground">{modelDisplayName}</span>{" "}
            model for transcription.
          </p>
        </div>
      )}

      {/* Sub-settings - only show when enabled */}
      {status.enabled && (
        <div className="mx-3 mb-3 mt-0 rounded-lg border-l-4 border-l-blue-500/50 bg-muted/20">
          <div className="p-4 space-y-4">
            {/* Status Display */}
            <div className="flex items-center gap-2 p-3 rounded-lg bg-green-500/10 border border-green-500/20">
              <Server className="h-4 w-4 text-green-500" />
              <div className="flex-1">
                <p className="text-sm font-medium text-green-700 dark:text-green-400">
                  Sharing Active
                </p>
                <p className="text-xs text-muted-foreground">
                  {modelDisplayName
                    ? `Model: ${modelDisplayName}`
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
              <Label className="text-sm font-medium">Connect Using</Label>
              <div className="space-y-1">
                {status.binding_results.length === 0 ? (
                  <div className="px-3 py-2 rounded-md bg-muted/50 border border-border/50 font-mono text-sm text-muted-foreground">
                    Starting server...
                  </div>
                ) : (
                  <>
                    {/* Show successful bindings first */}
                    {status.binding_results
                      .filter((result) => result.success)
                      .map((result, index) => (
                        <div key={`success-${index}`} className="flex items-center gap-2">
                          <CheckCircle className="h-4 w-4 text-green-500 flex-shrink-0" />
                          <div className="flex-1 px-3 py-2 rounded-md bg-muted/50 border border-green-500/30 font-mono text-sm">
                            <span className="font-semibold">{result.ip}:{port}</span>
                          </div>
                          <button
                            onClick={() => copyAddress(result.ip)}
                            className="p-2 rounded-md border border-transparent text-muted-foreground hover:bg-accent hover:text-accent-foreground hover:border-border/50 active:bg-accent/80 active:scale-95 transition-all duration-150"
                            title="Copy address"
                          >
                            <Copy className="h-4 w-4" />
                          </button>
                        </div>
                      ))}
                    {/* Show failed bindings with tooltip */}
                    {status.binding_results
                      .filter((result) => !result.success)
                      .map((result, index) => (
                        <div
                          key={`failed-${index}`}
                          className="flex items-center gap-2 opacity-50"
                          title={result.error || "Failed to bind"}
                        >
                          <XCircle className="h-4 w-4 text-red-400 flex-shrink-0" />
                          <div className="flex-1 px-3 py-2 rounded-md bg-muted/30 border border-border/30 font-mono text-sm text-muted-foreground">
                            <span>{result.ip}:{port}</span>
                            <span className="ml-2 text-xs text-red-400">(bind failed)</span>
                          </div>
                        </div>
                      ))}
                  </>
                )}
              </div>
              {status.binding_results.filter((r) => r.success).length > 0 && (
                <p className="text-xs text-muted-foreground">
                  Other VoiceTypr instances can connect using any of the addresses above
                </p>
              )}
              {status.binding_results.filter((r) => !r.success).length > 0 && (
                <p className="text-xs text-amber-500">
                  Some addresses failed to bind - hover for details
                </p>
              )}
            </div>

            {/* Server Configuration Section */}
            <div className="rounded-lg border border-border/50 bg-background/50 p-3 space-y-3">
              <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                Server Configuration
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
                      placeholder="No password"
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
                </div>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
