import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group";
import { Spinner } from "@/components/ui/spinner";
import { invoke } from "@tauri-apps/api/core";
import { CheckCircle2, Eye, EyeOff, Server, XCircle } from "lucide-react";
import React, { useState } from "react";
import { toast } from "sonner";

interface SavedConnection {
  id: string;
  host: string;
  port: number;
  password?: string | null;
  has_password?: boolean;
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
  const [clearSavedPassword, setClearSavedPassword] = useState(false);

  const isEditMode = !!editServer;
  const testRequiresReplacementPassword =
    isEditMode && !!editServer?.has_password && !password && !clearSavedPassword;

  // Update form when editServer changes
  React.useEffect(() => {
    if (editServer && open) {
      setHost(editServer.host);
      setPort(editServer.port.toString());
      setPassword(editServer.password || "");
      setName(editServer.name || "");
      setClearSavedPassword(false);
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
    setClearSavedPassword(false);
  };

  const handleClose = () => {
    resetForm();
    onOpenChange(false);
  };

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) {
      resetForm();
    }
    onOpenChange(nextOpen);
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
        const preservePassword = !!editServer.has_password && !password && !clearSavedPassword;
        // Update existing server
        server = await invoke<SavedConnection>("update_remote_server", {
          serverId: editServer.id,
          host: host.trim(),
          port: portNum,
          password: preservePassword ? null : password || null,
          preservePassword,
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
    <Dialog open={open} onOpenChange={handleOpenChange}>
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

        <FieldGroup className="py-4">
          <Field>
            <FieldLabel htmlFor="server-host">Host Address</FieldLabel>
            <Input
              id="server-host"
              placeholder="192.168.1.100 or hostname"
              value={host}
              onChange={(e) => setHost(e.target.value)}
              disabled={saving}
            />
            <FieldDescription>
              Use the host shown on the sharing Mac, or a stable LAN hostname.
            </FieldDescription>
          </Field>

          <Field>
            <FieldLabel htmlFor="server-port">Port</FieldLabel>
            <Input
              id="server-port"
              type="number"
              placeholder="47842"
              value={port}
              onChange={(e) => setPort(e.target.value)}
              disabled={saving}
              className="font-mono"
            />
          </Field>

          <Field>
            <FieldLabel htmlFor="server-password">Password (if required)</FieldLabel>
            <InputGroup>
              <InputGroupInput
                id="server-password"
                type={showPassword ? "text" : "password"}
                placeholder={isEditMode && editServer?.has_password ? "Leave empty to keep saved password" : "Leave empty if no password"}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                disabled={saving}
                className="[&::-ms-reveal]:hidden [&::-webkit-credentials-auto-fill-button]:hidden"
              />
              <InputGroupAddon align="inline-end">
                <InputGroupButton
                  type="button"
                  size="icon-xs"
                  onClick={() => setShowPassword(!showPassword)}
                  tabIndex={-1}
                  aria-label={showPassword ? "Hide password" : "Show password"}
                >
                  {showPassword ? (
                    <EyeOff className="size-4" />
                  ) : (
                    <Eye className="size-4" />
                  )}
                </InputGroupButton>
              </InputGroupAddon>
            </InputGroup>
            {isEditMode && editServer?.has_password && !password && (
              <div>
                <Button
                  type="button"
                  variant={clearSavedPassword ? "destructive" : "outline"}
                  size="sm"
                  onClick={() => setClearSavedPassword((value) => !value)}
                  disabled={saving}
                >
                  {clearSavedPassword ? "Password will be removed" : "Keep saved password"}
                </Button>
              </div>
            )}
          </Field>

          <Field>
            <FieldLabel htmlFor="server-name">Display Name (optional)</FieldLabel>
            <Input
              id="server-name"
              placeholder="e.g., Office Desktop"
              value={name}
              onChange={(e) => setName(e.target.value)}
              disabled={saving}
            />
          </Field>

          <Button
            variant="outline"
            className="w-full"
            onClick={handleTestConnection}
            disabled={!host.trim() || testStatus === "testing" || saving || testRequiresReplacementPassword}
          >
            {testStatus === "testing" ? (
              <>
                <Spinner className="size-4" />
                Testing...
              </>
            ) : (
              "Test Connection"
            )}
          </Button>
          {testRequiresReplacementPassword && (
            <p className="text-xs text-muted-foreground">
              Enter a replacement password to test this server. Saving with this field empty keeps the saved password.
            </p>
          )}

          {testStatus === "success" && testResult && (
            <div className="rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-3 py-2">
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-1.5 text-emerald-700 dark:text-emerald-400">
                  <CheckCircle2 className="size-3.5" />
                  <span className="text-xs font-medium">Connected</span>
                </div>
                <span className="text-xs text-muted-foreground">
                  {testResult.name} • {testResult.model}
                </span>
              </div>
            </div>
          )}

          {testStatus === "error" && testError && (
            <div className={`rounded-lg border px-3 py-2 ${
              isSelfConnection
                ? "border-amber-500/30 bg-amber-500/10"
                : "border-destructive/30 bg-destructive/10"
            }`}>
              <div className={`flex items-center gap-1.5 ${
                isSelfConnection
                  ? "text-amber-700 dark:text-amber-400"
                  : "text-destructive"
              }`}>
                <XCircle className="size-3.5" />
                <span className="text-xs font-medium">
                  {isSelfConnection ? "Self-connection detected" : "Connection failed"}
                </span>
                <span className="text-xs text-muted-foreground">– {testError}</span>
              </div>
            </div>
          )}
        </FieldGroup>

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
                <Spinner className="size-4" />
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
