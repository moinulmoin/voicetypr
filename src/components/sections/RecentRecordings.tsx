import { ScrollArea } from "@/components/ui/scroll-area";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { formatHotkey } from "@/lib/hotkey-utils";
import { TranscriptionHistory } from "@/types";
import { useCanAutoInsert, useReadiness } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { AlertCircle, AlertTriangle, Mic, Trash2, Search, Copy, Calendar, Download, RotateCcw, Loader2, FolderOpen, HelpCircle } from "lucide-react";
import { useState, useMemo, useCallback, useEffect } from "react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { getModelDisplayName } from "@/lib/model-display";
import { isCloudEngine } from "@/lib/cloudProviders";
import { isMacOS } from "@/lib/platform";
import { createLogger } from "@/lib/logger";

const log = createLogger("recordings");

interface SavedConnection {
  id: string;
  name: string;
  host: string;
  port: number;
  model?: string;
}

interface CurrentTranscriptionSource {
  type: 'local' | 'cloud' | 'remote';
  displayName: string;
  historyModelName: string;
  modelName?: string;
  modelEngine?: string;
  serverId?: string;
}


interface RecentRecordingsProps {
  history: TranscriptionHistory[];
  hotkey?: string;
  onHistoryUpdate?: () => void;
}

// ---------------------------------------------------------------------------
// Pure helpers — exported so tests can drive them directly
// ---------------------------------------------------------------------------

/** Map raw source values to user-facing labels. */
export function sourceLabel(source: string | undefined): string {
  switch (source) {
    case 'desktop_recording': return 'Dictation';
    case 'audio_file':
    case 'audio_bytes': return 'Upload';
    case 'remote_server': return 'Remote';
    default: return '';
  }
}

/** Format milliseconds as m:ss (e.g. 90 000 ms → "1:30"). */
export function formatDurationMs(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  return `${min}:${sec.toString().padStart(2, '0')}`;
}

/**
 * Structural filters for the history list (source, app, date).
 * Text search is handled separately in the component to support model display-name matching.
 * Under a specific source filter, only rows whose writing.source maps to that source pass;
 * rows with no/unknown source are excluded (they appear only under 'all').
 */
export function applyHistoryFilters(
  history: TranscriptionHistory[],
  sourceFilter: string,
  appFilter: string,
  dateFilter: string,
  now?: Date,
): TranscriptionHistory[] {
  const todayBase = now ? new Date(now) : new Date();
  todayBase.setHours(0, 0, 0, 0);
  const sevenDaysAgo = new Date(todayBase);
  sevenDaysAgo.setDate(sevenDaysAgo.getDate() - 6);

  return history.filter(item => {
    // Source filter — requires an exact match; rows with no/unknown source are excluded
    if (sourceFilter !== 'all') {
      const src = item.writing?.source;
      if (sourceFilter === 'desktop_recording' && src !== 'desktop_recording') return false;
      if (sourceFilter === 'audio_file' && src !== 'audio_file' && src !== 'audio_bytes') return false;
      if (sourceFilter === 'remote_server' && src !== 'remote_server') return false;
    }

    // App filter
    if (appFilter !== 'all' && item.writing?.context_hint?.app_name !== appFilter) return false;

    // Date filter
    if (dateFilter !== 'all') {
      const itemDate = new Date(item.timestamp);
      itemDate.setHours(0, 0, 0, 0);
      if (dateFilter === 'today' && itemDate.getTime() !== todayBase.getTime()) return false;
      if (dateFilter === 'last7' && itemDate < sevenDaysAgo) return false;
    }

    return true;
  });
}

export function RecentRecordings({ history, hotkey = "Cmd+Shift+Space", onHistoryUpdate }: RecentRecordingsProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [reTranscribingIds, setReTranscribingIds] = useState<Set<string>>(new Set());
  const [verifiedRecordings, setVerifiedRecordings] = useState<Set<string>>(new Set());
  const [checkedRecordings, setCheckedRecordings] = useState<Set<string>>(new Set());
  const [reTranscribingModels, setReTranscribingModels] = useState<Map<string, string>>(new Map());
  const [sourceFilter, setSourceFilter] = useState("all");
  const [appFilter, setAppFilter] = useState("all");
  const [dateFilter, setDateFilter] = useState("all");
  const { settings } = useSettings();
  const readiness = useReadiness();
  const canRecord = readiness.canRecord;
  const canAutoInsert = useCanAutoInsert();
  const unavailableMessage =
    readiness.licenseStatus === "expired" || readiness.licenseStatus === "none"
      ? "Activate a license to record again."
      : readiness.hasModels === false || readiness.selectedModelAvailable === false
        ? "Choose a ready local model, cloud provider, or remote VoiceTypr source in Models."
        : isMacOS && readiness.hasMicrophonePermission === false
          ? "Allow microphone access in macOS Settings."
          : "Finish setup in Settings before recording.";

  const resolveCurrentTranscriptionSource = useCallback(async (): Promise<CurrentTranscriptionSource | null> => {
    const activeRemoteServerId = await invoke<string | null>("get_active_remote_server").catch((error) => {
      log.error("Failed to resolve active remote VoiceTypr:", error);
      return null;
    });

    if (activeRemoteServerId) {
      let displayBase = "Remote VoiceTypr";
      let remoteModel = "";

      try {
        const servers = await invoke<SavedConnection[]>("list_remote_servers");
        const server = servers.find((candidate) => candidate.id === activeRemoteServerId);
        if (server) {
          displayBase = server.name || `${server.host}:${server.port}`;
          remoteModel = server.model ?? "";
        }
      } catch (error) {
        log.error("Failed to load active remote VoiceTypr label:", error);
      }

      const modelDisplayName = getModelDisplayName(remoteModel) ?? remoteModel;
      return {
        type: 'remote',
        serverId: activeRemoteServerId,
        displayName: modelDisplayName ? `${displayBase} - ${modelDisplayName}` : displayBase,
        historyModelName: `Remote: ${displayBase}`,
      };
    }

    const modelName = settings?.current_model?.trim();
    if (!modelName) {
      return null;
    }

    const modelEngine = settings?.current_model_engine ?? 'whisper';
    const isCloud = isCloudEngine(modelEngine);
    const displayName = getModelDisplayName(modelName) ?? modelName;

    return {
      type: isCloud ? 'cloud' : 'local',
      modelName,
      modelEngine,
      displayName,
      historyModelName: displayName,
    };
  }, [settings?.current_model, settings?.current_model_engine]);

  // Verify which recordings exist on filesystem
  useEffect(() => {
    const verifyRecordings = async () => {
      log.debug("[RecentRecordings] Starting verification for", history.length, "items");
      const verified = new Set<string>();
      const checked = new Set<string>();
      let itemsWithRecordingFile = 0;
      for (const item of history) {
        if (item.recording_file) {
          itemsWithRecordingFile++;
          log.debug("[RecentRecordings] Checking recording:", item.recording_file, "for item:", item.id);
          try {
            const exists = await invoke<boolean>("check_recording_exists", {
              filename: item.recording_file
            });
            checked.add(item.id);
            log.debug("[RecentRecordings] Recording", item.recording_file, "exists:", exists);
            if (exists) {
              verified.add(item.id);
            }
          } catch (error) {
            log.error(`Failed to verify recording ${item.recording_file}:`, error);
          }
        }
      }
      log.debug("[RecentRecordings] Verification complete. Items with recording_file:", itemsWithRecordingFile, "Verified:", verified.size);
      setCheckedRecordings(checked);
      setVerifiedRecordings(verified);
    };
    verifyRecordings();
  }, [history]);

  // Collect distinct app names from history for the app filter dropdown
  const distinctAppNames = useMemo(() => {
    const names = new Set<string>();
    for (const item of history) {
      const app = item.writing?.context_hint?.app_name;
      if (app) names.add(app);
    }
    return [...names].sort();
  }, [history]);


  // Handle showing recording in folder
  const handleShowInFolder = useCallback(async (item: TranscriptionHistory) => {
    if (!item.recording_file) return;

    try {
      // Get the full path and reveal in file explorer
      const fullPath = await invoke<string>("get_recording_path", {
        filename: item.recording_file
      });
      await invoke("show_in_folder", { path: fullPath });
    } catch (error) {
      log.error("Failed to show recording in folder:", error);
      toast.error("Failed to open file location");
    }
  }, []);

  // Handle re-transcription
  const handleReTranscribe = async (item: TranscriptionHistory) => {
    if (!item.recording_file) {
      toast.error("Re-transcription needs a saved audio file", {
        description: "Enable Save recordings for future takes you may want to re-transcribe.",
      });
      return;
    }

    const currentSource = await resolveCurrentTranscriptionSource();
    if (!currentSource) {
      toast.error("Choose a ready transcription source in Models before re-transcribing.");
      return;
    }

    // Mark this item as re-transcribing with the current source name
    setReTranscribingIds(prev => new Set(prev).add(item.id));
    setReTranscribingModels(prev => new Map(prev).set(item.id, currentSource.displayName));

    // Helper to clear the transcribing state for this item
    const cleanup = () => {
      setReTranscribingIds(prev => {
        const next = new Set(prev);
        next.delete(item.id);
        return next;
      });
      setReTranscribingModels(prev => {
        const next = new Map(prev);
        next.delete(item.id);
        return next;
      });
    };

    const pendingModelName = currentSource.historyModelName;
    let retryTimestamp: string | null = null;

    try {
      // Get recordings directory to build full path
      const recordingsDir = await invoke<string>("get_recordings_directory");
      const separator = recordingsDir.includes('\\') ? '\\' : '/';
      const fullPath = `${recordingsDir}${separator}${item.recording_file}`;
      retryTimestamp = await invoke<string>("save_retranscription", {
        text: "In progress...",
        model: pendingModelName,
        recordingFile: item.recording_file,
        sourceRecordingId: item.id,
        status: 'in_progress',
      });

      let result: string;
      let modelName: string;

      if (currentSource.type === 'remote') {
        if (!currentSource.serverId) {
          throw new Error("No active remote VoiceTypr source selected");
        }

        result = await invoke<string>('transcribe_remote', {
          serverId: currentSource.serverId,
          audioPath: fullPath,
        });
        modelName = currentSource.historyModelName;
      } else {
        if (!currentSource.modelName) {
          throw new Error("No local or cloud transcription model selected");
        }

        result = (await invoke<{ text: string; words: Array<{ text: string; start_ms?: number; end_ms?: number; speaker_id?: string; confidence?: number }> | null }>('transcribe_audio_file', {
          filePath: fullPath,
          modelName: currentSource.modelName,
          modelEngine: currentSource.modelEngine ?? null,
        })).text;
        modelName = currentSource.type === 'cloud' ? currentSource.displayName : currentSource.modelName;
      }

      await invoke("update_transcription", {
        timestamp: retryTimestamp,
        text: result,
        model: modelName,
        status: 'completed',
      });

      // Clear the re-transcribing state
      cleanup();

      // Refresh history to show the updated item
      if (onHistoryUpdate) {
        onHistoryUpdate();
      }
    } catch (error) {
      log.error("Re-transcription failed:", error);
      const failureMessage = `Re-transcription failed: ${String(error)}`;
      try {
        if (!retryTimestamp) {
          throw error;
        }
        await invoke("update_transcription", {
          timestamp: retryTimestamp,
          text: failureMessage,
          model: pendingModelName,
          status: 'failed',
        });
      } catch (updateError) {
        log.error("Failed to persist retranscription error state:", updateError);
      }
      toast.error("Re-transcription failed", {
        description: String(error)
      });
      // Clear pending on error too
      cleanup();
      if (onHistoryUpdate) {
        onHistoryUpdate();
      }
    }
  };

  // Filter history by source/app/date (structural) then by text search
  const filteredHistory = useMemo(() => {
    const structural = applyHistoryFilters(history, sourceFilter, appFilter, dateFilter);
    if (!searchQuery.trim()) return structural;
    const q = searchQuery.trim().toLowerCase();
    return structural.filter(item =>
      item.text.toLowerCase().includes(q) ||
      (item.model && item.model.toLowerCase().includes(q)) ||
      (item.model && (getModelDisplayName(item.model) ?? '').toLowerCase().includes(q)),
    );
  }, [history, searchQuery, sourceFilter, appFilter, dateFilter]);

  // Group history by date
  const groupedHistory = useMemo(() => {
    const groups: Record<string, TranscriptionHistory[]> = {};
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);

    filteredHistory.forEach(item => {
      const itemDate = new Date(item.timestamp);
      itemDate.setHours(0, 0, 0, 0);

      let groupKey: string;
      if (itemDate.getTime() === today.getTime()) {
        groupKey = "Today";
      } else if (itemDate.getTime() === yesterday.getTime()) {
        groupKey = "Yesterday";
      } else {
        groupKey = itemDate.toLocaleDateString('en-US', {
          weekday: 'long',
          month: 'short',
          day: 'numeric',
          year: itemDate.getFullYear() !== today.getFullYear() ? 'numeric' : undefined
        });
      }

      if (!groups[groupKey]) {
        groups[groupKey] = [];
      }
      groups[groupKey].push(item);
    });

    return groups;
  }, [filteredHistory]);

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    toast.success("Copied to clipboard");
  };

  const handleDelete = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();

    try {
      // Show confirmation dialog
      const confirmed = await ask("Are you sure you want to delete this transcription?", {
        title: "Delete Transcription",
        kind: "warning"
      });

      if (!confirmed) return;

      // Call the delete command with the timestamp (id)
      await invoke("delete_transcription_entry", { timestamp: id });

      toast.success("Transcription deleted");

      // Refresh the history
      if (onHistoryUpdate) {
        onHistoryUpdate();
      }
    } catch (error) {
      log.error("Failed to delete transcription:", error);
      toast.error("Failed to delete transcription");
    }
  };

  const handleClearAll = async () => {
    if (history.length === 0) return;

    try {
      // Show confirmation dialog
      const confirmed = await ask(`Are you sure you want to delete all ${history.length} transcriptions? This action cannot be undone.`, {
        title: "Clear All Transcriptions",
        kind: "warning"
      });

      if (!confirmed) return;

      // Call the clear all command
      await invoke("clear_all_transcriptions");

      toast.success("All transcriptions cleared");

      // Refresh the history
      if (onHistoryUpdate) {
        onHistoryUpdate();
      }
    } catch (error) {
      log.error("Failed to clear all transcriptions:", error);
      toast.error("Failed to clear all transcriptions");
    }
  };

  const handleExport = async () => {
    if (history.length === 0) return;

    try {
      // Show confirmation dialog with location info
      const confirmed = await ask(
        `Export ${history.length} transcription${history.length !== 1 ? 's' : ''} to JSON?\n\nThe file will be saved to your Downloads folder.`, 
        {
          title: "Export Transcriptions",
          kind: "info"
        }
      );

      if (!confirmed) return;

      // Call the backend export command
      await invoke<string>("export_transcriptions");
      
      toast.success(`Exported ${history.length} transcriptions`, {
        description: `Saved to Downloads folder`
      });
    } catch (error) {
      log.error("Failed to export transcriptions:", error);
      toast.error("Failed to export transcriptions");
    }
  };

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="px-6 py-4 border-b border-border/40">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h1 className="text-2xl font-semibold">History</h1>
              <Dialog>
                <DialogTrigger asChild>
                  <Button type="button" variant="secondary" size="icon" aria-label="History guide" className="rounded-full">
                    <HelpCircle className="h-4.5 w-4.5" />
                  </Button>
                </DialogTrigger>
                <DialogContent className="sm:max-w-lg">
                  <DialogHeader>
                    <DialogTitle>History guide</DialogTitle>
                    <DialogDescription>
                      History stores completed transcripts so you can reuse, export, delete, or re-transcribe them.
                    </DialogDescription>
                  </DialogHeader>
                  <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                    <p><strong className="text-foreground">Search</strong> filters saved transcripts by text and source metadata.</p>
                    <p><strong className="text-foreground">Re-transcribe</strong> reruns a saved audio take with your current transcription source. It only appears when the original audio file was saved.</p>
                    <p><strong className="text-foreground">Export</strong> saves transcript history as JSON for backup or review.</p>
                  </div>
                </DialogContent>
              </Dialog>
            </div>
            <p className="mt-1 text-sm text-muted-foreground">
              Search, copy, export, delete, and re-transcribe saved audio takes.
              {history.length > 0 ? ` ${history.length} total.` : ""}
            </p>
          </div>
          <div className="flex items-center gap-3">
            {history.length > 0 && (
              <Button
                onClick={handleExport}
                size="sm"
                title="Export transcriptions to JSON"
              >
                <Download className="h-3.5 w-3.5" />
                Export
              </Button>
            )}
            {history.length > 5 && (
              <Button
                onClick={handleClearAll}
                variant="ghost"
                size="sm"
                className="text-destructive hover:text-destructive"
                title="Clear all transcriptions"
              >
                <Trash2 className="h-3.5 w-3.5" />
                Clear All
              </Button>
            )}
          </div>
        </div>
      </div>

      {/* Search + Filters */}
      {history.length > 0 && (
        <div className="px-6 py-3 space-y-2">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <input
              type="text"
              placeholder="Search transcriptions..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full pl-10 pr-4 py-2 text-sm bg-background border border-border/50 rounded-lg focus:outline-none focus:border-primary/50 transition-colors"
            />
            {searchQuery && (
              <button
                onClick={() => setSearchQuery("")}
                className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
              >
                ×
              </button>
            )}
          </div>
          {/* Filter row */}
          <div className="flex flex-wrap items-center gap-2">
            <select
              value={sourceFilter}
              onChange={(e) => setSourceFilter(e.target.value)}
              className="text-xs bg-background border border-border/50 rounded-md px-2 py-1 text-muted-foreground focus:outline-none focus:border-primary/50 transition-colors"
            >
              <option value="all">All sources</option>
              <option value="desktop_recording">Dictation</option>
              <option value="audio_file">Upload</option>
              <option value="remote_server">Remote</option>
            </select>
            <select
              value={dateFilter}
              onChange={(e) => setDateFilter(e.target.value)}
              className="text-xs bg-background border border-border/50 rounded-md px-2 py-1 text-muted-foreground focus:outline-none focus:border-primary/50 transition-colors"
            >
              <option value="all">All time</option>
              <option value="today">Today</option>
              <option value="last7">Last 7 days</option>
            </select>
            {distinctAppNames.length > 0 && (
              <select
                value={appFilter}
                onChange={(e) => setAppFilter(e.target.value)}
                className="text-xs bg-background border border-border/50 rounded-md px-2 py-1 text-muted-foreground focus:outline-none focus:border-primary/50 transition-colors"
              >
                <option value="all">All apps</option>
                {distinctAppNames.map((name) => (
                  <option key={name} value={name}>{name}</option>
                ))}
              </select>
            )}
            {(sourceFilter !== 'all' || appFilter !== 'all' || dateFilter !== 'all' || searchQuery) && (
              <button
                onClick={() => { setSearchQuery(""); setSourceFilter("all"); setAppFilter("all"); setDateFilter("all"); }}
                className="text-xs text-muted-foreground hover:text-foreground transition-colors"
              >
                Clear filters
              </button>
            )}
          </div>
          {(searchQuery || sourceFilter !== 'all' || appFilter !== 'all' || dateFilter !== 'all') && (
            <p className="text-xs text-muted-foreground">
              Found {filteredHistory.length} result{filteredHistory.length !== 1 ? 's' : ''}
            </p>
          )}
        </div>
      )}
      <div className="flex-1 min-h-0 overflow-hidden">
      {history.length > 0 ? (
        filteredHistory.length > 0 ? (
          <ScrollArea className="h-full">
            <div className="px-6 py-4 space-y-6">
              {Object.entries(groupedHistory).map(([date, items]) => (
                <div key={date} className="space-y-3">
                  <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                    <Calendar className="h-3 w-3" />
                    {date}
                    <span className="text-muted-foreground/50">({items.length})</span>
                  </div>
                  <div className="space-y-2">
                    {items.map((item) => {
                      const isFailed = item.status === 'failed';
                      // Persisted in_progress rows should already have been backend-reconciled before render.
                      const isPersistedInProgress = item.status === 'in_progress';
                      const isInProgress = reTranscribingIds.has(item.id) || isPersistedInProgress;
                      return (
                      <div
                        key={item.id}
                        className={cn(
                          "group relative rounded-lg cursor-pointer",
                          "bg-card border",
                          isInProgress
                            ? "border-primary/50"
                            : isFailed
                            ? "border-amber-500/50 bg-amber-500/5"
                            : "border-border/50 hover:bg-accent/30 hover:border-border",
                          "transition-all duration-200"
                        )}
                        onClick={() => !isFailed && !isInProgress && handleCopy(item.text)}
                      >
                        {/* Re-transcribing status bar */}
                        {isInProgress && (
                          <div className="flex items-center gap-2 px-4 py-2 bg-primary/10 border-b border-primary/20 rounded-t-lg">
                            <Loader2 className="w-3.5 h-3.5 animate-spin text-primary" />
                            <span className="text-xs text-primary font-medium">
                              {isPersistedInProgress && !reTranscribingIds.has(item.id)
                                ? `Re-transcription in progress with ${getModelDisplayName(item.model) ?? item.model}...`
                                : `Re-transcribing with ${reTranscribingModels.get(item.id)}...`}
                            </span>
                          </div>
                        )}
                        {/* Failed status bar */}
                        {isFailed && !isInProgress && (
                          <div className="flex items-center justify-between gap-2 px-4 py-2 bg-amber-500/10 border-b border-amber-500/20 rounded-t-lg">
                            <div className="flex items-center gap-2">
                              <AlertTriangle className="w-3.5 h-3.5 text-amber-500" />
                              <span className="text-xs text-amber-600 dark:text-amber-400 font-medium">
                                {verifiedRecordings.has(item.id)
                                  ? 'Transcription failed - recording preserved'
                                  : checkedRecordings.has(item.id)
                                    ? 'Transcription failed - recording unavailable for retry'
                                    : 'Transcription failed'}
                              </span>
                            </div>
                            {verifiedRecordings.has(item.id) && (
                              <button
                                onClick={(e) => {
                                  e.stopPropagation();
                                  void handleReTranscribe(item);
                                }}
                                className="flex items-center gap-1 px-2 py-1 text-xs font-medium text-amber-600 dark:text-amber-400 bg-amber-500/20 rounded hover:bg-amber-500/30 transition-colors"
                              >
                                <RotateCcw className="w-3 h-3" />
                                Re-transcribe
                              </button>
                            )}
                          </div>
                        )}
                        {/* Translation-failed status bar: saved text is the raw, untranslated transcript */}
                        {item.writing?.translation_failed && !isFailed && !isInProgress && (
                          <div className="flex items-center gap-2 px-4 py-2 bg-amber-500/10 border-b border-amber-500/20 rounded-t-lg">
                            <AlertTriangle className="w-3.5 h-3.5 text-amber-500" />
                            <span className="text-xs text-amber-600 dark:text-amber-400 font-medium">
                              {item.writing.target_language
                                ? `Translation to ${item.writing.target_language} failed - saved untranslated`
                                : 'Translation failed - saved untranslated'}
                            </span>
                          </div>
                        )}
                        <div className="p-4">
                          {/* Text content */}
                          <p className={cn(
                            "text-sm leading-relaxed line-clamp-5",
                            isFailed ? "text-muted-foreground italic" : "text-foreground"
                          )}>
                            {item.text}
                          </p>
                          {/* Bottom row: model name, time + action buttons */}
                          <div className="flex items-center justify-between mt-2">
                            <div className="flex items-center gap-2 text-xs text-muted-foreground">
                              {item.model && (
                                <span>{getModelDisplayName(item.model) ?? item.model}</span>
                              )}
                              {item.model && <span className="text-muted-foreground/50">•</span>}
                              <span>
                                {new Date(item.timestamp).toLocaleDateString([], { month: 'short', day: 'numeric' })}{' '}
                                {new Date(item.timestamp).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}
                              </span>
                            </div>
                            <div className="flex items-center gap-1 transition-opacity">
                              {!isInProgress && (
                                <>
                                  <button
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      handleCopy(item.text);
                                    }}
                                    className="p-1.5 rounded hover:bg-accent transition-colors"
                                    title="Copy"
                                  >
                                    <Copy className="w-4 h-4 text-muted-foreground" />
                                  </button>
                                  {verifiedRecordings.has(item.id) && (
                                    <button
                                      onClick={(e) => {
                                        e.stopPropagation();
                                        handleShowInFolder(item);
                                      }}
                                      className="p-1.5 rounded hover:bg-accent transition-colors"
                                      title="Show recording in folder"
                                    >
                                      <FolderOpen className="w-4 h-4 text-muted-foreground" />
                                    </button>
                                  )}
                                </>
                              )}
                              {/* Re-transcribe button - only show if recording file exists and verified */}
                              {verifiedRecordings.has(item.id) && (
                                <button
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    void handleReTranscribe(item);
                                  }}
                                  className={cn(
                                    "p-1.5 rounded hover:bg-accent transition-colors",
                                    isInProgress && "pointer-events-none"
                                  )}
                                  title="Re-transcribe with current source"
                                  disabled={isInProgress}
                                >
                                  {isInProgress ? (
                                    <Loader2 className="w-4 h-4 text-muted-foreground animate-spin" />
                                  ) : (
                                    <RotateCcw className="w-4 h-4 text-muted-foreground" />
                                  )}
                                </button>
                              )}
                              <button
                                onClick={(e) => handleDelete(e, item.id)}
                                className="p-1.5 rounded hover:bg-destructive/10 transition-colors"
                                title="Delete"
                              >
                                <Trash2 className="w-4 h-4 text-destructive" />
                              </button>
                            </div>
                          </div>
                          {/* Metadata badges: source, duration, diarized, app */}
                          {item.writing && (sourceLabel(item.writing.source) || item.writing.audio_duration_ms != null || item.writing.diarized || item.writing.context_hint?.app_name) && (
                            <div className="flex flex-wrap items-center gap-1 mt-1.5">
                              {sourceLabel(item.writing.source) && (
                                <span className="px-1.5 py-0.5 text-xs rounded bg-muted text-muted-foreground">
                                  {sourceLabel(item.writing.source)}
                                </span>
                              )}
                              {item.writing.audio_duration_ms != null && (
                                <span className="px-1.5 py-0.5 text-xs rounded bg-muted text-muted-foreground">
                                  {formatDurationMs(item.writing.audio_duration_ms)}
                                </span>
                              )}
                              {item.writing.diarized && (
                                <span className="px-1.5 py-0.5 text-xs rounded bg-muted text-muted-foreground">
                                  Speakers
                                </span>
                              )}
                              {item.writing.context_hint?.app_name && (
                                <span className="px-1.5 py-0.5 text-xs rounded bg-muted/60 text-muted-foreground">
                                  {item.writing.context_hint.app_name}
                                </span>
                              )}
                            </div>
                          )}
                        </div>
                      </div>
                    )})}
                  </div>
                </div>
              ))}
            </div>
          </ScrollArea>
        ) : (
          <div className="flex-1 flex items-center justify-center">
            <div className="text-center">
              <Search className="w-12 h-12 text-muted-foreground/30 mx-auto mb-4" />
              <p className="text-sm text-muted-foreground">No transcriptions found</p>
              <p className="text-xs text-muted-foreground/70 mt-2">
                Try adjusting your search or filters
              </p>
            </div>
          </div>
        )
      ) : (
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center">
            {canRecord ? (
              <>
                <Mic className="w-12 h-12 text-muted-foreground/30 mx-auto mb-4" />
                <p className="text-sm text-muted-foreground">No recordings yet</p>
                {canAutoInsert ? (
                  <p className="text-xs text-muted-foreground/70 mt-2">
                    Press {formatHotkey(hotkey)} to record. Save recordings in Settings to enable re-transcription.
                  </p>
                ) : (
                  <p className="text-xs text-amber-600 mt-2">
                    Recording available but accessibility permission needed for hotkeys
                  </p>
                )}
              </>
            ) : (
              <>
                <AlertCircle className="w-12 h-12 text-amber-500/50 mx-auto mb-4" />
                <p className="text-sm text-muted-foreground">Cannot record yet</p>
                <p className="text-xs text-amber-600 mt-2">
                  {unavailableMessage}
                </p>
              </>
            )}
          </div>
        </div>
      )}
      </div>
    </div>
  );
}
