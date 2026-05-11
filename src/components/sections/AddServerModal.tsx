import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { invoke } from "@tauri-apps/api/core";
import { CheckCircle2, Eye, EyeOff, Loader2, Server, XCircle } from "lucide-react";
import React, { useState } from "react";
import { toast } from "sonner";

interface SavedConnection {
  id: string;
  host: string;
  port: number;
  password: string | null;
  name: string | null;
  created_at: number;
}

interface StatusResponse {
  status: string;
  version: string;
  model: string;
  name: string;
  machine_id: string;
}

interface AddServerModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onServerAdded?: (server: SavedConnection) => void;
  editServer?: SavedConnection | null; // If provided, modal is in edit mode
}

type TestStatus = "idle" | "testing" | "success" | "error";

export function AddServerModal({
  open,
  onOpenChange,
  onServerAdded,
  editServer,
}: AddServerModalProps) {
  const [host, setHost] = useState("");
  const [port, setPort] = useState("47842");
  const [password, setPassword] = useState("");
  const [name, setName] = useState("");
  const [testStatus, setTestStatus] = useState<TestStatus>("idle");
  const [showPassword, setShowPassword] = useState(false);
  const [testResult, setTestResult] = useState<StatusResponse | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [localMachineId, setLocalMachineId] = useState<string | null>(null);
  const [isSelfConnection, setIsSelfConnection] = useState(false);

  const isEditMode = !!editServer;

  // Populate form when editing
  useState(() => {
    if (editServer && open) {
      setHost(editServer.host);
      setPort(editServer.port.toString());
      setPassword(editServer.password || "");
      setName(editServer.name || "");
    }
  });

  // Update form when editServer changes
  React.useEffect(() => {
    if (editServer && open) {
      setHost(editServer.host);
      setPort(editServer.port.toString());
      setPassword(editServer.password || "");
      setName(editServer.name || "");
    }
  }, [editServer, open]);

  // Fetch local machine ID for self-connection detection
  React.useEffect(() => {
    if (open && !localMachineId) {
      invoke<string>("get_local_machine_id")
        .then(setLocalMachineId)
        .catch((err) => console.warn("Failed to get local machine ID:", err));
    }
  }, [open, localMachineId]);

  const resetForm = () => {
    setHost("");
    setPort("47842");
    setPassword("");
    setName("");
    setTestStatus("idle");
    setTestResult(null);
    setTestError(null);
    setIsSelfConnection(false);
  };

  const handleClose = () => {
    resetForm();
    onOpenChange(false);
  };

  const handleTestConnection = async () => {
    if (!host.trim()) {
      toast.error("Please enter a host address");
      return;
    }

    setTestStatus("testing");
    setTestError(null);
    setTestResult(null);
    setIsSelfConnection(false);

    try {
      const portNum = parseInt(port, 10) || 47842;

      // Use Tauri command for proper error differentiation
      const data = await invoke<StatusResponse>("test_remote_connection", {
        host: host.trim(),
        port: portNum,
        password: password || null,
      });

      // Check if this is our own machine
      if (localMachineId && data.machine_id === localMachineId) {
        setIsSelfConnection(true);
        setTestError("Cannot add your own machine as a remote");
        setTestStatus("error");
        return;
      }

      setTestResult(data);
      setTestStatus("success");

      // Auto-fill name if empty
      if (!name.trim() && data.name) {
        setName(data.name);
      }
    } catch (error) {
      console.error("Connection test failed:", error);
      let errorMessage = "Connection failed";

      if (typeof error === "string") {
        // Backend returns specific error messages
        if (error.includes("Authentication failed")) {
          errorMessage = "Authentication failed - check password";
        } else if (error.includes("Failed to connect")) {
          errorMessage = "Cannot connect - check host and port";
        } else {
          errorMessage = error;
        }
      }

      setTestError(errorMessage);
      setTestStatus("error");
    }
  };

  const handleSaveServer = async () => {
    if (!host.trim()) {
      toast.error("Please enter a host address");
      return;
    }

    // Block save if self-connection detected
    if (isSelfConnection) {
      toast.error("Cannot add your own machine as a remote");
      return;
    }

    setSaving(true);
    try {
      const portNum = parseInt(port, 10) || 47842;

      let server: SavedConnection;
      if (isEditMode && editServer) {
        // Update existing server
        server = await invoke<SavedConnection>("update_remote_server", {
          serverId: editServer.id,
          host: host.trim(),
          port: portNum,
          password: password || null,
          name: name.trim() || null,
        });
        toast.success(`"${server.name || server.host}" updated`);
      } else {
        // Add new server
        server = await invoke<SavedConnection>("add_remote_server", {
          host: host.trim(),
          port: portNum,
          password: password || null,
          name: name.trim() || null,
        });
        toast.success(`"${server.name || server.host}" added`);
      }

      onServerAdded?.(server);
      handleClose();
    } catch (error) {
      console.error(`Failed to ${isEditMode ? "update" : "add"} server:`, error);
      const errorMessage =
        error instanceof Error ? error.message : `Failed to ${isEditMode ? "update" : "add"} server`;
      toast.error(errorMessage);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Server className="h-5 w-5" />
            {isEditMode ? "Edit Remote VoiceTypr" : "Add Remote VoiceTypr"}
          </DialogTitle>
          <DialogDescription>
            {isEditMode
              ? "Update connection details for this remote VoiceTypr"
              : "Connect to another VoiceTypr over the network"}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          {/* Host Input */}
          <div className="space-y-2">
            <Label htmlFor="server-host">Host Address</Label>
            <Input
              id="server-host"
              placeholder="192.168.1.100 or hostname"
              value={host}
              onChange={(e) => setHost(e.target.value)}
              disabled={saving}
            />
          </div>

          {/* Port Input */}
          <div className="space-y-2">
            <Label htmlFor="server-port">Port</Label>
            <Input
              id="server-port"
              type="number"
              placeholder="47842"
              value={port}
              onChange={(e) => setPort(e.target.value)}
              disabled={saving}
              className="font-mono"
            />
          </div>

          {/* Password Input */}
          <div className="space-y-2">
            <Label htmlFor="server-password">Password (if required)</Label>
            <div className="relative">
              <Input
                id="server-password"
                type={showPassword ? "text" : "password"}
                placeholder="Leave empty if no password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                disabled={saving}
                className="pr-10 [&::-ms-reveal]:hidden [&::-webkit-credentials-auto-fill-button]:hidden"
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
          </div>

          {/* Name Input */}
          <div className="space-y-2">
            <Label htmlFor="server-name">Display Name (optional)</Label>
            <Input
              id="server-name"
              placeholder="e.g., Office Desktop"
              value={name}
              onChange={(e) => setName(e.target.value)}
              disabled={saving}
            />
          </div>

          {/* Test Connection Button */}
          <Button
            variant="outline"
            className="w-full"
            onClick={handleTestConnection}
            disabled={!host.trim() || testStatus === "testing" || saving}
          >
            {testStatus === "testing" ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                Testing...
              </>
            ) : (
              "Test Connection"
            )}
          </Button>

          {/* Test Result - compact */}
          {testStatus === "success" && testResult && (
            <div className="rounded-md border border-green-500/30 bg-green-500/10 px-3 py-2">
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-1.5 text-green-700 dark:text-green-400">
                  <CheckCircle2 className="h-3.5 w-3.5" />
                  <span className="text-xs font-medium">Connected</span>
                </div>
                <span className="text-xs text-muted-foreground">
                  {testResult.name} • {testResult.model}
                </span>
              </div>
            </div>
          )}

          {testStatus === "error" && testError && (
            <div className={`rounded-md border px-3 py-2 ${
              isSelfConnection
                ? "border-amber-500/30 bg-amber-500/10"
                : "border-red-500/30 bg-red-500/10"
            }`}>
              <div className={`flex items-center gap-1.5 ${
                isSelfConnection
                  ? "text-amber-700 dark:text-amber-400"
                  : "text-red-700 dark:text-red-400"
              }`}>
                <XCircle className="h-3.5 w-3.5" />
                <span className="text-xs font-medium">
                  {isSelfConnection ? "Self-connection detected" : "Connection failed"}
                </span>
                <span className="text-xs text-muted-foreground">– {testError}</span>
              </div>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={handleClose} disabled={saving}>
            Cancel
          </Button>
          <Button
            onClick={handleSaveServer}
            disabled={!host.trim() || saving || isSelfConnection}
          >
            {saving ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                {isEditMode ? "Saving..." : "Adding..."}
              </>
            ) : isEditMode ? (
              "Save Changes"
            ) : (
              "Add Server"
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
