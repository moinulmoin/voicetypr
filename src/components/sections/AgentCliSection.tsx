import { Button } from "@/components/ui/button";
import { invoke } from "@tauri-apps/api/core";
import { CheckCircle, Loader2, XCircle } from "lucide-react";
import { useEffect, useState } from "react";
import { createLogger } from "@/lib/logger";
import { toast } from "sonner";

const log = createLogger("cli-tool");

interface CliToolStatus {
  installed: boolean;
  manageable: boolean;
  path: string | null;
}

const RECIPES = [
  "voicetypr transcribe <file> --json",
  "voicetypr record --json",
  "voicetypr --help",
] as const;

/**
 * Surfaces the `voicetypr` command-line tool: install/remove status and a
 * small recipe of example invocations. Installability is driven entirely by
 * the backend's `manageable` flag, so this stays platform-agnostic.
 */
export function AgentCliSection() {
  const [status, setStatus] = useState<CliToolStatus | null>(null);
  const [pending, setPending] = useState<"install" | "uninstall" | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<CliToolStatus>("cli_tool_status")
      .then((next) => {
        if (!cancelled) setStatus(next);
      })
      .catch((error) => {
        // Swallow: we simply leave status null and show only the recipe.
        log.error("Failed to read CLI tool status:", error);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // NOTE: on macOS, install_cli_tool may prompt for an admin password and take
  // a few seconds; on Windows it is fast. The call resolves once complete.
  const install = async () => {
    setPending("install");
    try {
      const next = await invoke<CliToolStatus>("install_cli_tool");
      setStatus(next);
      if (next.installed) {
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
  const busy = pending !== null;

  return (
    <div className="space-y-4">
      <h2 className="text-base font-semibold">Command-line tool</h2>

      <div className="rounded-lg border border-border/50 bg-card p-4 space-y-4">
        <p className="text-sm text-muted-foreground">
          Run transcription from your terminal — and let AI agents and scripts
          drive VoiceTypr — with the{" "}
          <code className="rounded bg-muted px-1 py-0.5 font-mono text-[0.85em]">
            voicetypr
          </code>{" "}
          command.
        </p>

        {(status === null || manageable) && (
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              {status === null ? (
                <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
              ) : installed ? (
                <CheckCircle className="h-4 w-4 text-green-600" />
              ) : (
                <XCircle className="h-4 w-4 text-muted-foreground" />
              )}
              <div className="min-w-0">
                <p className="text-sm font-medium">
                  {status === null
                    ? "Checking…"
                    : installed
                      ? "Installed"
                      : "Not installed"}
                </p>
                {status?.path && (
                  <p className="truncate text-xs font-mono text-muted-foreground">
                    {status.path}
                  </p>
                )}
              </div>
            </div>

            {installed ? (
              <Button
                variant="ghost"
                size="sm"
                onClick={uninstall}
                disabled={busy}
              >
                {pending === "uninstall" ? (
                  <>
                    <Loader2 className="h-3 w-3 animate-spin" />
                    Removing…
                  </>
                ) : (
                  "Remove command"
                )}
              </Button>
            ) : (
              <Button size="sm" onClick={install} disabled={busy || status === null}>
                {pending === "install" ? (
                  <>
                    <Loader2 className="h-3 w-3 animate-spin" />
                    Installing…
                  </>
                ) : (
                  "Install command"
                )}
              </Button>
            )}
          </div>
        )}

        {status?.manageable === false && (
          <p className="text-xs text-muted-foreground">
            The{" "}
            <code className="font-mono">voicetypr</code> command ships with the
            app on this platform.
          </p>
        )}

        <div className="rounded-md border bg-muted/40 p-3 font-mono text-xs space-y-1">
          {RECIPES.map((line) => (
            <p key={line}>{line}</p>
          ))}
        </div>
      </div>
    </div>
  );
}
