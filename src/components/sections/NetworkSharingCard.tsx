import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { invoke } from "@tauri-apps/api/core";
import { Copy, Network, Server } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

interface SharingStatus {
  enabled: boolean;
  port: number | null;
  model_name: string | null;
  server_name: string | null;
  active_connections: number;
}

export function NetworkSharingCard() {
  const [status, setStatus] = useState<SharingStatus>({
    enabled: false,
    port: null,
    model_name: null,
    server_name: null,
    active_connections: 0,
  });
  const [port, setPort] = useState("47842");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [localIp, setLocalIp] = useState<string | null>(null);

  // Fetch current sharing status
  const fetchStatus = useCallback(async () => {
    try {
      const result = await invoke<SharingStatus>("get_sharing_status");
      setStatus(result);
      if (result.port) {
        setPort(result.port.toString());
      }
    } catch (error) {
      console.error("Failed to get sharing status:", error);
    }
  }, []);

  // Fetch status on mount
  useEffect(() => {
    // For local IP, we just show a placeholder since getting it requires OS-specific code
    // Users can find their IP via system settings or use hostname
    setLocalIp("your-ip-here");
    fetchStatus();
  }, [fetchStatus]);

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

  const copyAddress = () => {
    const address = `${localIp || "localhost"}:${port}`;
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
                Share transcription with other devices
              </p>
            </div>
          </div>
          <Switch
            id="network-sharing"
            checked={status.enabled}
            onCheckedChange={handleToggleSharing}
            disabled={loading}
          />
        </div>
      </div>

      {/* Content */}
      <div className="p-4 space-y-4">
        {/* Status Display */}
        {status.enabled && (
          <div className="flex items-center gap-2 p-3 rounded-lg bg-green-500/10 border border-green-500/20">
            <Server className="h-4 w-4 text-green-500" />
            <div className="flex-1">
              <p className="text-sm font-medium text-green-700 dark:text-green-400">
                Sharing Active
              </p>
              <p className="text-xs text-muted-foreground">
                {status.model_name
                  ? `Model: ${status.model_name}`
                  : "No model selected"}
              </p>
            </div>
          </div>
        )}

        {/* Server Address */}
        <div className="space-y-2">
          <Label className="text-sm font-medium">Server Address</Label>
          <div className="flex items-center gap-2">
            <div className="flex-1 px-3 py-2 rounded-md bg-muted/50 border border-border/50 font-mono text-sm">
              {localIp || "..."}:{port}
            </div>
            <button
              onClick={copyAddress}
              className="p-2 rounded-md hover:bg-muted transition-colors"
              title="Copy address"
            >
              <Copy className="h-4 w-4" />
            </button>
          </div>
          <p className="text-xs text-muted-foreground">
            Other VoiceTypr instances can connect to this address
          </p>
        </div>

        {/* Port Setting */}
        <div className="space-y-2">
          <Label htmlFor="sharing-port" className="text-sm font-medium">
            Port
          </Label>
          <Input
            id="sharing-port"
            type="number"
            value={port}
            onChange={(e) => setPort(e.target.value)}
            placeholder="47842"
            disabled={status.enabled}
            className="font-mono"
          />
          <p className="text-xs text-muted-foreground">
            Default: 47842. Change requires restart of sharing.
          </p>
        </div>

        {/* Password Setting */}
        <div className="space-y-2">
          <Label htmlFor="sharing-password" className="text-sm font-medium">
            Password (Optional)
          </Label>
          <Input
            id="sharing-password"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Leave empty for no authentication"
            disabled={status.enabled}
          />
          <p className="text-xs text-muted-foreground">
            Require password for connections. Recommended for public networks.
          </p>
        </div>
      </div>
    </div>
  );
}
