import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { invoke } from "@tauri-apps/api/core";
import {
  CheckCircle2,
  Loader2,
  Server,
  Trash2,
  Wifi,
  WifiOff,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";

export interface SavedConnection {
  id: string;
  host: string;
  port: number;
  password: string | null;
  name: string | null;
  created_at: number;
}

export interface StatusResponse {
  status: string;
  version: string;
  model: string;
  name: string;
}

interface RemoteServerCardProps {
  server: SavedConnection;
  isActive: boolean;
  onSelect: (serverId: string) => void;
  onRemove: (serverId: string) => void;
}

export function RemoteServerCard({
  server,
  isActive,
  onSelect,
  onRemove,
}: RemoteServerCardProps) {
  const [status, setStatus] = useState<"checking" | "online" | "offline">(
    "checking"
  );
  const [serverInfo, setServerInfo] = useState<StatusResponse | null>(null);
  const [removing, setRemoving] = useState(false);

  const checkStatus = useCallback(async () => {
    setStatus("checking");
    try {
      const response = await invoke<StatusResponse>("test_remote_server", {
        serverId: server.id,
      });
      setServerInfo(response);
      setStatus("online");
    } catch {
      setServerInfo(null);
      setStatus("offline");
    }
  }, [server.id]);

  useEffect(() => {
    checkStatus();
    // Re-check every 30 seconds
    const interval = setInterval(checkStatus, 30000);
    return () => clearInterval(interval);
  }, [checkStatus]);

  const handleRemove = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setRemoving(true);
    try {
      await onRemove(server.id);
    } finally {
      setRemoving(false);
    }
  };

  const displayName = server.name || `${server.host}:${server.port}`;

  return (
    <Card
      className={cn(
        "px-4 py-3 border-border/50 transition cursor-pointer",
        "hover:border-border",
        isActive && "bg-primary/5 border-primary/30"
      )}
      onClick={() => status === "online" && onSelect(server.id)}
    >
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-3 min-w-0">
          <div
            className={cn(
              "p-2 rounded-md",
              status === "online" ? "bg-green-500/10" : "bg-muted"
            )}
          >
            <Server
              className={cn(
                "h-4 w-4",
                status === "online" ? "text-green-500" : "text-muted-foreground"
              )}
            />
          </div>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h3 className="font-medium text-sm truncate">{displayName}</h3>
              {isActive && (
                <CheckCircle2 className="h-4 w-4 text-primary flex-shrink-0" />
              )}
            </div>
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              {status === "checking" ? (
                <span className="flex items-center gap-1">
                  <Loader2 className="h-3 w-3 animate-spin" />
                  Checking...
                </span>
              ) : status === "online" ? (
                <>
                  <Wifi className="h-3 w-3 text-green-500" />
                  <span className="text-green-600 dark:text-green-400">
                    Online
                  </span>
                  {serverInfo?.model && (
                    <span className="text-muted-foreground">
                      • {serverInfo.model}
                    </span>
                  )}
                </>
              ) : (
                <>
                  <WifiOff className="h-3 w-3 text-red-500" />
                  <span className="text-red-600 dark:text-red-400">
                    Offline
                  </span>
                </>
              )}
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2 flex-shrink-0">
          <Button
            size="sm"
            variant="ghost"
            className="h-8 w-8 p-0 text-muted-foreground hover:text-destructive"
            onClick={handleRemove}
            disabled={removing}
          >
            {removing ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Trash2 className="h-4 w-4" />
            )}
          </Button>
        </div>
      </div>
    </Card>
  );
}
