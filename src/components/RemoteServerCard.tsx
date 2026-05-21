import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ButtonGroup } from "@/components/ui/button-group";
import { Card } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { cn } from "@/lib/utils";
import {
  AlertTriangle,
  CheckCircle2,
  KeyRound,
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
  password?: string | null;
  has_password?: boolean;
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
        "px-4 py-3 border transition-all hover:shadow-sm",
        isSelectable ? "cursor-pointer" : "cursor-default",
        status === "self_connection"
          ? "border-amber-500/30 bg-amber-500/10"
          : isActive
            ? "border-primary/45 bg-primary/5 shadow-sm ring-2 ring-primary/15"
            : isSelectable
              ? "border-border/60 bg-card/90 hover:border-border"
              : "border-border/60 bg-card/90"
      )}
      onClick={() => isSelectable && onSelect(server.id)}
    >
      <div className="flex items-center justify-between gap-4">
        <div className="flex min-w-0 items-center gap-3">
          <div
            className={cn(
              "flex size-9 shrink-0 items-center justify-center rounded-lg border",
              isActive
                ? "border-primary/25 bg-primary/10 text-primary"
                : status === "online"
                  ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-700"
                  : status === "auth_failed" || status === "self_connection"
                    ? "border-amber-500/20 bg-amber-500/10 text-amber-700"
                    : "border-border bg-muted/60 text-muted-foreground"
            )}
          >
            <Server className="size-4" />
          </div>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h3
                className={cn(
                  "truncate text-sm font-semibold tracking-tight",
                  isActive && "text-primary"
                )}
              >
                {displayName}
              </h3>
              {isActive && (
                <Badge className="gap-1">
                  <CheckCircle2 className="size-3" />
                  Active
                </Badge>
              )}
            </div>
            <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
              {status === "unknown" ? (
                server.model ? (
                  <>
                    <Wifi className="size-3 text-muted-foreground" />
                    <span>{server.model}</span>
                    {isRefreshing && <Spinner className="size-3" />}
                  </>
                ) : (
                  <span className="inline-flex items-center gap-1">
                    {isRefreshing ? (
                      <>
                        <Spinner className="size-3" />
                        Checking...
                      </>
                    ) : (
                      "Status unknown"
                    )}
                  </span>
                )
              ) : status === "online" ? (
                <>
                  <Wifi className="size-3 text-emerald-600" />
                  <span className="text-emerald-700 dark:text-emerald-400">
                    Online
                  </span>
                  {server.model && (
                    <span>
                      • {server.model}
                    </span>
                  )}
                  {isRefreshing && <Spinner className="size-3" />}
                </>
              ) : status === "auth_failed" ? (
                <>
                  <KeyRound className="size-3 text-amber-600" />
                  <span className="text-amber-700 dark:text-amber-400">
                    Auth Failed
                  </span>
                  {isRefreshing && <Spinner className="size-3" />}
                </>
              ) : status === "self_connection" ? (
                <>
                  <AlertTriangle className="size-3 text-amber-600" />
                  <span className="text-amber-700 dark:text-amber-400">
                    This Machine
                  </span>
                  <span>
                    • Cannot use self
                  </span>
                </>
              ) : (
                <>
                  <WifiOff className="size-3 text-destructive" />
                  <span className="text-destructive">
                    Offline
                  </span>
                  {isRefreshing && <Spinner className="size-3" />}
                </>
              )}
            </div>
          </div>
        </div>

        <ButtonGroup className="shrink-0">
          <Button
            size="sm"
            variant="ghost"
            className="size-8 p-0 text-muted-foreground hover:text-foreground"
            onClick={handleEdit}
            title="Edit server"
          >
            <Pencil className="size-4" />
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="size-8 p-0 text-muted-foreground hover:text-destructive"
            onClick={handleRemove}
            disabled={removing}
            title="Remove server"
          >
            {removing ? (
              <Spinner className="size-4" />
            ) : (
              <Trash2 className="size-4" />
            )}
          </Button>
        </ButtonGroup>
      </div>
    </Card>
  );
}
