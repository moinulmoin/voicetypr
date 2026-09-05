import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { getErrorMessage } from "@/utils/error";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { HardDrive } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

interface SonioxStorageCounts {
  filesTotal: number;
  transcriptionsTotal: number;
}

interface SonioxCleanupResult {
  deletedTranscriptions: number;
  deletedFiles: number;
  skippedProcessing: number;
  errors: string[];
}

/**
 * Soniox retains every dictation as a stored file + transcription record
 * against org caps (1,000 files / 2,000 transcriptions). Voicetypr deletes
 * each dictation's records automatically after transcription; this card
 * shows the remaining backlog and drains it.
 */
export function SonioxStorageCard() {
  const [counts, setCounts] = useState<SonioxStorageCounts | null>(null);
  const [countError, setCountError] = useState<string | null>(null);
  const [cleaning, setCleaning] = useState(false);
  const [progress, setProgress] = useState<{ deleted: number; total: number } | null>(
    null
  );

  const loadCounts = useCallback(async () => {
    setCountError(null);
    try {
      setCounts(await invoke<SonioxStorageCounts>("get_soniox_storage_counts"));
    } catch (error) {
      setCounts(null);
      setCountError(error instanceof Error ? error.message : String(error));
    }
  }, []);

  useEffect(() => {
    void loadCounts();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<{ deleted: number; total: number }>("soniox-cleanup-progress", (event) => {
      if (!disposed) setProgress(event.payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [loadCounts]);

  const handleCleanup = async () => {
    setCleaning(true);
    setProgress(null);
    try {
      const result = await invoke<SonioxCleanupResult>("cleanup_soniox_storage");
      const deleted = result.deletedTranscriptions + result.deletedFiles;
      const skipped = result.skippedProcessing;
      toast.success(
        `Deleted ${deleted} stored record${deleted === 1 ? "" : "s"}${
          skipped > 0 ? ` (${skipped} still processing)` : ""
        }${result.errors.length > 0 ? ` — ${result.errors.length} failed` : ""}`
      );
      await loadCounts();
    } catch (error) {
      toast.error(getErrorMessage(error, "Failed to clean up stored files"));
    } finally {
      setCleaning(false);
      setProgress(null);
    }
  };

  return (
    <Card className="rounded-xl border border-border bg-card p-4">
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <HardDrive className="size-4 shrink-0 text-sage" />
            <h3 className="text-sm font-semibold tracking-tight">Soniox stored files</h3>
          </div>
          <p className="mt-2.5 text-xs text-muted-foreground">
            Soniox caps stored files (1,000) and transcription records (2,000) per
            account. Voicetypr now deletes each dictation's records automatically
            after transcription — clear the backlog from before that existed.
          </p>
          <p className="mt-2 text-xs text-muted-foreground" data-testid="soniox-storage-counts">
            {counts
              ? `Stored files: ${counts.filesTotal} · Stored transcriptions: ${counts.transcriptionsTotal}`
              : countError
                ? `Could not read storage usage: ${countError}`
                : "Reading storage usage…"}
          </p>
        </div>
        <Button
          size="sm"
          className="shrink-0"
          onClick={() => void handleCleanup()}
          disabled={cleaning}
        >
          {cleaning ? (
            <>
              <Spinner className="size-4" />
              {progress ? ` ${progress.deleted}/${progress.total}` : " Cleaning…"}
            </>
          ) : (
            "Clean up stored files"
          )}
        </Button>
      </div>
    </Card>
  );
}
