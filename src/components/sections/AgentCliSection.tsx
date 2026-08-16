import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { createLogger } from "@/lib/logger";
import { invoke } from "@tauri-apps/api/core";
import { CheckCircle, CircleAlert, Loader2, RefreshCw, XCircle } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

const log = createLogger("cli-tool");

interface CliToolStatus {
  installed: boolean;
  manageable: boolean;
  path: string | null;
  app_version: string;
  command_version: string | null;
  compatible: boolean;
  detail: string | null;
}

const RECIPES = [
  "voicetypr transcribe <file> --json",
  "voicetypr --help",
] as const;

/**
 * Surfaces the `voicetypr` command-line tool: install/remove status and a
 * small recipe of example invocations. Installability is driven entirely by
 * the backend's `manageable` flag, so this stays platform-agnostic.
 */
export function AgentCliSection() {
  const [status, setStatus] = useState<CliToolStatus | null>(null);
  const [pending, setPending] = useState<"install" | "repair" | "uninstall" | "refresh" | null>(
    null,
  );

  const refresh = useCallback(async (showError = false) => {
    setPending("refresh");
    try {
      setStatus(await invoke<CliToolStatus>("cli_tool_status"));
    } catch (error) {
      log.error("Failed to read CLI tool status:", error);
      if (showError) toast.error("Failed to refresh command health.");
    } finally {
      setPending(null);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const install = async () => {
    setPending("install");
    try {
      const next = await invoke<CliToolStatus>("install_cli_tool");
      setStatus(next);
      if (next.installed && next.compatible) {
        toast.success("voicetypr command installed. Open a new terminal to use it.");
      } else {
        toast.error("Could not install the voicetypr command.");
      }
    } catch (error) {
      log.error("Failed to install CLI tool:", error);
      toast.error("Failed to install the voicetypr command.");
    } finally {
      setPending(null);
    }
  };

  const repair = async () => {
    setPending("repair");
    try {
      const next = await invoke<CliToolStatus>("repair_cli_tool");
      setStatus(next);
      if (next.compatible) {
        toast.success("voicetypr command now matches this app.");
      } else {
        toast.error("Could not repair the voicetypr command.");
      }
    } catch (error) {
      log.error("Failed to repair CLI tool:", error);
      toast.error("Failed to repair the voicetypr command.");
    } finally {
      setPending(null);
    }
  };

  const uninstall = async () => {
    setPending("uninstall");
    try {
      const next = await invoke<CliToolStatus>("uninstall_cli_tool");
      setStatus(next);
      if (!next.installed) {
        toast.success("voicetypr command removed.");
      } else {
        toast.error("Could not remove the voicetypr command.");
      }
    } catch (error) {
      log.error("Failed to uninstall CLI tool:", error);
      toast.error("Failed to remove the voicetypr command.");
    } finally {
      setPending(null);
    }
  };

  const manageable = status?.manageable ?? false;
  const installed = status?.installed ?? false;
  const compatible = status?.compatible ?? false;
  const busy = pending !== null;

  return (
    <div className="space-y-4">
      <h2 className="text-base font-semibold">Voicetypr command line</h2>

      <div className="space-y-4 rounded-lg border border-border/50 bg-card p-4">
        <p className="text-sm text-muted-foreground">
          Run transcription from your terminal and let AI agents or scripts use the same
          Voicetypr engine with the{" "}
          <code className="rounded bg-muted px-1 py-0.5 font-mono text-[0.85em]">
            voicetypr
          </code>{" "}
          command.
        </p>

        <div className="rounded-xl border border-border/70 bg-background/60 p-4">
          <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
            <div className="flex min-w-0 items-start gap-3">
              {status === null ? (
                <Loader2 className="mt-0.5 size-4 shrink-0 animate-spin text-muted-foreground" />
              ) : compatible ? (
                <CheckCircle className="mt-0.5 size-4 shrink-0 text-sage" />
              ) : installed ? (
                <CircleAlert className="mt-0.5 size-4 shrink-0 text-amber-600" />
              ) : (
                <XCircle className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
              )}
              <div className="min-w-0">
                <p className="text-sm font-medium">
                  {status === null
                    ? "Checking command health…"
                    : compatible
                      ? "Ready and compatible"
                      : installed
                        ? "Command needs attention"
                        : "Command not installed"}
                </p>
                {status?.path && (
                  <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
                    {status.path}
                  </p>
                )}
                {status?.detail && (
                  <p className="mt-2 max-w-xl text-xs leading-relaxed text-muted-foreground">
                    {status.detail}
                  </p>
                )}
              </div>
            </div>
            <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
              {manageable && (
                installed ? (
                  <>
                    <Button variant="outline" size="sm" onClick={repair} disabled={busy}>
                      {pending === "repair" && <Loader2 className="animate-spin" />}
                      {compatible ? "Repair command" : "Update command"}
                    </Button>
                    <Button variant="ghost" size="sm" onClick={uninstall} disabled={busy}>
                      {pending === "uninstall" && <Loader2 className="animate-spin" />}
                      Remove command
                    </Button>
                  </>
                ) : (
                  <Button size="sm" onClick={install} disabled={busy}>
                    {pending === "install" && <Loader2 className="animate-spin" />}
                    Install command
                  </Button>
                )
              )}
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Refresh command health"
                onClick={() => void refresh(true)}
                disabled={busy}
              >
                <RefreshCw className={pending === "refresh" ? "animate-spin" : ""} />
              </Button>
            </div>
          </div>

          {status && (
            <div className="mt-4 flex flex-wrap gap-2 border-t border-border/60 pt-3">
              <Badge variant="outline">App v{status.app_version}</Badge>
              <Badge variant={compatible ? "secondary" : "outline"}>
                Command {status.command_version ? `v${status.command_version}` : "version unknown"}
              </Badge>
            </div>
          )}
        </div>

        {status?.manageable === false && !status.detail && (
          <p className="text-xs text-muted-foreground">
            Command management is unavailable on this platform.
          </p>
        )}

        <div className="space-y-1 rounded-md border bg-muted/40 p-3 font-mono text-xs">
          {RECIPES.map((line) => (
            <p key={line}>{line}</p>
          ))}
        </div>
      </div>
    </div>
  );
}
