import { invoke } from "@tauri-apps/api/core";
import { Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Switch } from "@/components/ui/switch";
import { createLogger } from "@/lib/logger";
import { toast } from "sonner";

const log = createLogger("telemetry");

interface TelemetryStatus {
  enabled: boolean;
  available: boolean;
}

interface ConsentResult {
  enabled: boolean;
  restart_required: boolean;
}

/**
 * Privacy-first diagnostics, on by default (opt-out). When enabled it sends only
 * anonymous crash/error reports. Disabling takes effect immediately, while
 * enabling needs an app restart to actually begin reporting. The backend's
 * `available` flag is false in dev builds and true in official releases.
 */
export function TelemetrySection() {
  const [status, setStatus] = useState<TelemetryStatus | null>(null);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    let cancelled = false;
    invoke<TelemetryStatus>("get_telemetry_status")
      .then((next) => {
        if (!cancelled) setStatus(next);
      })
      .catch((error) => {
        // Swallow: leave status null and keep the checking affordance.
        log.error("Failed to read telemetry status:", error);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const onCheckedChange = async (next: boolean) => {
    setPending(true);
    try {
      const res = await invoke<ConsentResult>("set_telemetry_consent", {
        enabled: next,
      });
      setStatus((prev) => (prev ? { ...prev, enabled: res.enabled } : prev));
      if (res.restart_required) {
        toast.info("Restart Voicetypr to start sending diagnostics.");
      } else {
        toast.success("Diagnostics turned off.");
      }
    } catch (error) {
      log.error("Failed to update telemetry consent:", error);
      toast.error("Could not update diagnostics setting.");
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="space-y-4">
      <h2 className="text-base font-semibold">Diagnostics</h2>

      <div className="rounded-lg border border-border/50 bg-card p-4 space-y-4">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <p className="text-sm font-medium">
              Help improve Voicetypr with anonymous diagnostics
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              On by default — turn it off anytime. Sends only anonymous crash
              reports to help us fix bugs, never your audio, transcripts, or
              personal data.
            </p>
          </div>

          <Switch
            className="shrink-0"
            checked={status?.enabled ?? true}
            disabled={pending || status === null || (!status.available && !status.enabled)}
            onCheckedChange={onCheckedChange}
            aria-label="Enable anonymous diagnostics"
          />
        </div>

        {status === null && (
          <div className="flex items-center gap-2 text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            <span className="text-xs">Checking…</span>
          </div>
        )}

        {status !== null && !status.available && (
          <p className="text-xs text-muted-foreground">
            Not available in this build — no reports are sent.
          </p>
        )}
      </div>
    </div>
  );
}
