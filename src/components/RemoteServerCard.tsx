import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import {
  AlertTriangle,
  CheckCircle2,
  KeyRound,
  Loader2,
  Pencil,
  Server,
  Trash2,
  Wifi,
  WifiOff,
} from "lucide-react";
import { useState } from "react";

// Connection status enum matching backend
export type ConnectionStatus = "Unknown" | "Online" | "Offline" | "AuthFailed" | "SelfConnection";

export interface SavedConnection {
  id: string;
  host: string;
  port: number;
  password: string | null;
  name: string | null;
  created_at: number;
  model?: string | null;
  status?: ConnectionStatus;
  last_checked?: number;
}

export interface StatusResponse {
  status: string;
  version: string;
  model: string;
  name: string;
  machine_id: string;
}

interface RemoteServerCardProps {
  server: SavedConnection;
  isActive: boolean;
  onSelect: (serverId: string) => void;
  onRemove: (serverId: string) => void;
  onEdit: (server: SavedConnection) => void;
  /** Whether a global refresh is in progress */
  isRefreshing?: boolean;
}

export function RemoteServerCard({
  server,
  isActive,
  onSelect,
  onRemove,
  onEdit,
  isRefreshing = false,
}: RemoteServerCardProps) {
  const [removing, setRemoving] = useState(false);

  // Map backend ConnectionStatus to display status
  // Status is now from cached data on server prop
  const getDisplayStatus = (): "unknown" | "online" | "auth_failed" | "offline" | "self_connection" => {
    switch (server.status) {
      case "Online": return "online";
      case "Offline": return "offline";
      case "AuthFailed": return "auth_failed";
      case "SelfConnection": return "self_connection";
      default: return "unknown";
    }
  };
  const status = getDisplayStatus();

  const handleRemove = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setRemoving(true);
    try {
      await onRemove(server.id);
    } finally {
      setRemoving(false);
    }
  };

  const handleEdit = (e: React.MouseEvent) => {
    e.stopPropagation();
    onEdit(server);
  };

  const displayName = server.name || `${server.host}:${server.port}`;

  // All servers are selectable - status is informational only
  // (Per user request: don't block selection based on status)
  const isSelectable = status !== "self_connection";

  return (
    <Card
      className={cn(
        "px-4 py-3 border transition",
        isSelectable ? "cursor-pointer" : "cursor-default",
        status === "self_connection"
          ? "bg-amber-500/5 border-amber-500/30"
          : isActive
            ? "bg-primary/8 border-primary/50 ring-2 ring-primary/20"
            : isSelectable
              ? "border-border/50 hover:border-border"
              : "border-border/50"
      )}
      onClick={() => isSelectable && onSelect(server.id)}
    >
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-3 min-w-0">
          <div
            className={cn(
              "p-2 rounded-md",
              isActive
                ? "bg-primary/20"
                : status === "online"
                  ? "bg-green-500/10"
                  : status === "unknown"
                    ? "bg-muted"
                    : status === "auth_failed" || status === "self_connection"
                      ? "bg-amber-500/10"
                      : "bg-muted"
            )}
          >
            <Server
              className={cn(
                "h-4 w-4",
                isActive
                  ? "text-primary"
                  : status === "online"
                    ? "text-green-500"
                    : status === "unknown"
                      ? "text-muted-foreground"
                      : status === "auth_failed" || status === "self_connection"
                        ? "text-amber-500"
                        : "text-muted-foreground"
              )}
            />
          </div>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h3
                className={cn(
                  "font-medium text-sm truncate",
                  isActive && "text-primary"
                )}
              >
                {displayName}
              </h3>
              {isActive && (
                <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-primary/20 text-primary text-xs font-medium flex-shrink-0">
                  <CheckCircle2 className="h-3 w-3" />
                  Active
                </span>
              )}
            </div>
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              {status === "unknown" ? (
                // Show last known model if available, otherwise show checking indicator
                server.model ? (
                  <>
                    <Wifi className="h-3 w-3 text-muted-foreground" />
                    <span className="text-muted-foreground">{server.model}</span>
                    {isRefreshing && <Loader2 className="h-3 w-3 animate-spin ml-1" />}
                  </>
                ) : (
                  <span className="flex items-center gap-1 text-muted-foreground">
                    {isRefreshing ? (
                      <>
                        <Loader2 className="h-3 w-3 animate-spin" />
                        Checking...
                      </>
                    ) : (
                      "Status unknown"
                    )}
                  </span>
                )
              ) : status === "online" ? (
                <>
                  <Wifi className="h-3 w-3 text-green-500" />
                  <span className="text-green-600 dark:text-green-400">
                    Online
                  </span>
                  {server.model && (
                    <span className="text-muted-foreground">
                      • {server.model}
                    </span>
                  )}
                  {isRefreshing && <Loader2 className="h-3 w-3 animate-spin ml-1" />}
                </>
              ) : status === "auth_failed" ? (
                <>
                  <KeyRound className="h-3 w-3 text-amber-500" />
                  <span className="text-amber-600 dark:text-amber-400">
                    Auth Failed
                  </span>
                  {isRefreshing && <Loader2 className="h-3 w-3 animate-spin ml-1" />}
                </>
              ) : status === "self_connection" ? (
                <>
                  <AlertTriangle className="h-3 w-3 text-amber-500" />
                  <span className="text-amber-600 dark:text-amber-400">
                    This Machine
                  </span>
                  <span className="text-muted-foreground">
                    • Cannot use self
                  </span>
                </>
              ) : (
                <>
                  <WifiOff className="h-3 w-3 text-red-500" />
                  <span className="text-red-600 dark:text-red-400">
                    Offline
                  </span>
                  {isRefreshing && <Loader2 className="h-3 w-3 animate-spin ml-1" />}
                </>
              )}
            </div>
          </div>
        </div>

        <div className="flex items-center gap-1 flex-shrink-0">
          <Button
            size="sm"
            variant="ghost"
            className="h-8 w-8 p-0 text-muted-foreground hover:text-foreground"
            onClick={handleEdit}
            title="Edit server"
          >
            <Pencil className="h-4 w-4" />
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="h-8 w-8 p-0 text-muted-foreground hover:text-destructive"
            onClick={handleRemove}
            disabled={removing}
            title="Remove server"
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
