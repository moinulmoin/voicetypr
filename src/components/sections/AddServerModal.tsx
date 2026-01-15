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
import { CheckCircle2, Loader2, Server, XCircle } from "lucide-react";
import { useState } from "react";
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
}

interface AddServerModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onServerAdded?: (server: SavedConnection) => void;
}

type TestStatus = "idle" | "testing" | "success" | "error";

export function AddServerModal({
  open,
  onOpenChange,
  onServerAdded,
}: AddServerModalProps) {
  const [host, setHost] = useState("");
  const [port, setPort] = useState("47842");
  const [password, setPassword] = useState("");
  const [name, setName] = useState("");
  const [testStatus, setTestStatus] = useState<TestStatus>("idle");
  const [testResult, setTestResult] = useState<StatusResponse | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const resetForm = () => {
    setHost("");
    setPort("47842");
    setPassword("");
    setName("");
    setTestStatus("idle");
    setTestResult(null);
    setTestError(null);
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

    try {
      // We need to test the connection before adding
      // Use a temporary test - add then remove, or just make an HTTP request
      const portNum = parseInt(port, 10) || 47842;

      // Create a temporary connection to test
      const response = await fetch(
        `http://${host.trim()}:${portNum}/api/v1/status`,
        {
          method: "GET",
          headers: password ? { "X-VoiceTypr-Key": password } : {},
        }
      );

      if (response.status === 401) {
        throw new Error("Authentication failed - check password");
      }

      if (!response.ok) {
        throw new Error(`Server returned ${response.status}`);
      }

      const data = (await response.json()) as StatusResponse;
      setTestResult(data);
      setTestStatus("success");

      // Auto-fill name if empty
      if (!name.trim() && data.name) {
        setName(data.name);
      }
    } catch (error) {
      console.error("Connection test failed:", error);
      const errorMessage =
        error instanceof Error ? error.message : "Connection failed";
      setTestError(errorMessage);
      setTestStatus("error");
    }
  };

  const handleAddServer = async () => {
    if (!host.trim()) {
      toast.error("Please enter a host address");
      return;
    }

    setSaving(true);
    try {
      const portNum = parseInt(port, 10) || 47842;
      const server = await invoke<SavedConnection>("add_remote_server", {
        host: host.trim(),
        port: portNum,
        password: password || null,
        name: name.trim() || null,
      });

      toast.success(`Server "${server.name || server.host}" added`);
      onServerAdded?.(server);
      handleClose();
    } catch (error) {
      console.error("Failed to add server:", error);
      const errorMessage =
        error instanceof Error ? error.message : "Failed to add server";
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
            Add Remote Server
          </DialogTitle>
          <DialogDescription>
            Connect to another VoiceTypr instance on your network
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
            <Input
              id="server-password"
              type="password"
              placeholder="Leave empty if no password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              disabled={saving}
            />
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

          {/* Test Result */}
          {testStatus === "success" && testResult && (
            <div className="rounded-lg border border-green-500/30 bg-green-500/10 p-3">
              <div className="flex items-center gap-2 text-green-700 dark:text-green-400">
                <CheckCircle2 className="h-4 w-4" />
                <span className="font-medium">Connection successful!</span>
              </div>
              <div className="mt-2 space-y-1 text-sm text-muted-foreground">
                <p>Server: {testResult.name}</p>
                <p>Model: {testResult.model}</p>
                <p>Version: {testResult.version}</p>
              </div>
            </div>
          )}

          {testStatus === "error" && testError && (
            <div className="rounded-lg border border-red-500/30 bg-red-500/10 p-3">
              <div className="flex items-center gap-2 text-red-700 dark:text-red-400">
                <XCircle className="h-4 w-4" />
                <span className="font-medium">Connection failed</span>
              </div>
              <p className="mt-1 text-sm text-muted-foreground">{testError}</p>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={handleClose} disabled={saving}>
            Cancel
          </Button>
          <Button
            onClick={handleAddServer}
            disabled={!host.trim() || saving}
          >
            {saving ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                Adding...
              </>
            ) : (
              "Add Server"
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
