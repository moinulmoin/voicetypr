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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { formatHotkey } from "@/lib/hotkey-utils";
import { TranscriptionHistory } from "@/types";
import { useCanAutoInsert, useReadiness } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { invoke } from "@tauri-apps/api/core";
import { ask, save } from "@tauri-apps/plugin-dialog";
import { AlertCircle, AlertTriangle, Mic, Trash2, Search, Copy, Monitor, Globe, FileAudio, Terminal, Download, RotateCcw, Loader2, FolderOpen, HelpCircle, ShieldCheck } from "lucide-react";
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
    case 'audio_file':
    case 'audio_bytes': return 'Upload';
    case 'remote_server': return 'Remote';
    case 'cli': return 'CLI';
    case 'desktop_recording':
    default: return 'This device';
  }
}

/** Format milliseconds as m:ss (e.g. 90 000 ms → "1:30"). */
export function formatDurationMs(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  return `${min}:${sec.toString().padStart(2, '0')}`;
}

/** Lucide icon for where a transcript came from (its source). */
function sourceIcon(source: string | undefined) {
  switch (source) {
    case 'audio_file':
    case 'audio_bytes':
      return FileAudio;
    case 'remote_server':
      return Globe;
    case 'cli':
      return Terminal;
    default:
      return Monitor;
  }
}

/** Build a plain-text export of transcript history. */
function buildPlainHistory(items: TranscriptionHistory[]): string {
  return items
    .map((item) => {
      const when = new Date(item.timestamp).toLocaleString();
      const model = getModelDisplayName(item.model) ?? item.model ?? "";
      return `[${when}]${model ? ` ${model}` : ""}\n${item.text}\n`;
    })
    .join("\n");
}

/** Build a Markdown export of transcript history. */
function buildMarkdownHistory(items: TranscriptionHistory[]): string {
  const lines: string[] = ["# Voicetypr transcript history", ""];
  for (const item of items) {
    const when = new Date(item.timestamp).toLocaleString();
    const model = getModelDisplayName(item.model) ?? item.model ?? "";
    lines.push(`## ${when}${model ? ` · ${model}` : ""}`, "", item.text, "");
  }
  return lines.join("\n");
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
      if (sourceFilter === 'cli' && src !== 'cli') return false;
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
  const [showOriginalIds, setShowOriginalIds] = useState<Set<string>>(new Set());
  const [visibleCount, setVisibleCount] = useState(60);
  const { settings } = useSettings();
  const readiness = useReadiness();
  const canRecord = readiness.canRecord;
  const canAutoInsert = useCanAutoInsert();
  const unavailableMessage =
    readiness.licenseStatus === "expired" || readiness.licenseStatus === "none"
      ? "Activate a license to record again."
      : readiness.hasModels === false || readiness.selectedModelAvailable === false
        ? "Choose a ready local model, cloud provider, or remote Voicetypr source in Models."
        : isMacOS && readiness.hasMicrophonePermission === false
          ? "Allow microphone access in macOS Settings."
          : "Finish setup in Settings before recording.";

  const resolveCurrentTranscriptionSource = useCallback(async (): Promise<CurrentTranscriptionSource | null> => {
    const activeRemoteServerId = await invoke<string | null>("get_active_remote_server").catch((error) => {
      log.error("Failed to resolve active remote Voicetypr:", error);
      return null;
    });

    if (activeRemoteServerId) {
      let displayBase = "Remote Voicetypr";
      let remoteModel = "";

      try {
        const servers = await invoke<SavedConnection[]>("list_remote_servers");
        const server = servers.find((candidate) => candidate.id === activeRemoteServerId);
        if (server) {
          displayBase = server.name || `${server.host}:${server.port}`;
          remoteModel = server.model ?? "";
        }
      } catch (error) {
        log.error("Failed to load active remote Voicetypr label:", error);
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

  // Reset the visible page whenever the result set changes via search/filter.
  useEffect(() => {
    setVisibleCount(60);
  }, [searchQuery, sourceFilter, dateFilter, appFilter]);

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
          throw new Error("No active remote Voicetypr source selected");
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

    filteredHistory.slice(0, visibleCount).forEach(item => {
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
  }, [filteredHistory, visibleCount]);

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

  const handleExportText = async (format: "txt" | "md") => {
    if (history.length === 0) return;
    try {
      const path = await save({
        defaultPath: `voicetypr-history.${format}`,
        filters: [{ name: format === "md" ? "Markdown" : "Text", extensions: [format] }],
      });
      if (!path) return;
      const content = format === "md" ? buildMarkdownHistory(history) : buildPlainHistory(history);
      await invoke("save_transcript_file", { path, content });
      toast.success(`Exported ${history.length} transcript${history.length === 1 ? "" : "s"}`, {
        description: format === "md" ? "Saved as Markdown" : "Saved as plain text",
      });
    } catch (error) {
      log.error("Failed to export transcripts:", error);
      toast.error("Failed to export transcripts");
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
      <div className="px-6 py-5 md:px-8">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h1 className="text-2xl font-semibold tracking-tight">History</h1>
              <Dialog>
                <DialogTrigger asChild>
                  <Button type="button" variant="ghost" size="icon-sm" aria-label="History guide" className="size-7 rounded-full text-muted-foreground">
                    <HelpCircle className="h-4 w-4" />
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
            <p className="mt-0.5 text-sm text-muted-foreground">
              {history.length > 0
                ? `${history.length} transcript${history.length === 1 ? "" : "s"} · stored on this ${isMacOS ? "Mac" : "PC"}`
                : "Your transcripts stay on this device — nothing syncs to a cloud."}
            </p>
          </div>
          <div className="flex items-center gap-2">
            {history.length > 0 && (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="secondary" size="sm" title="Export transcripts">
                    <Download className="h-3.5 w-3.5" />
                    Export
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onClick={handleExport}>JSON (.json)</DropdownMenuItem>
                  <DropdownMenuItem onClick={() => handleExportText("txt")}>Plain text (.txt)</DropdownMenuItem>
                  <DropdownMenuItem onClick={() => handleExportText("md")}>Markdown (.md)</DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
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
                Clear all
              </Button>
            )}
          </div>
        </div>
      </div>

      {/* Search + Filters */}
      {history.length > 0 && (
        <div className="px-6 md:px-8 py-3 space-y-2.5">
          <div className="relative">
            <Search className="absolute left-3.5 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <input
              type="text"
              placeholder="Search transcripts…"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="h-10 w-full rounded-xl border border-border bg-card pl-10 pr-4 text-sm transition-colors focus:border-sage/50 focus:outline-none focus:ring-2 focus:ring-sage/25"
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
            <Select value={sourceFilter} onValueChange={setSourceFilter}>
              <SelectTrigger className="h-9 w-auto gap-1.5 rounded-lg border-border bg-card text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All sources</SelectItem>
                <SelectItem value="desktop_recording">This device</SelectItem>
                <SelectItem value="audio_file">Upload</SelectItem>
                <SelectItem value="remote_server">Remote</SelectItem>
                <SelectItem value="cli">CLI</SelectItem>
              </SelectContent>
            </Select>
            <Select value={dateFilter} onValueChange={setDateFilter}>
              <SelectTrigger className="h-9 w-auto gap-1.5 rounded-lg border-border bg-card text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All time</SelectItem>
                <SelectItem value="today">Today</SelectItem>
                <SelectItem value="last7">Last 7 days</SelectItem>
              </SelectContent>
            </Select>
            {distinctAppNames.length > 0 && (
              <Select value={appFilter} onValueChange={setAppFilter}>
                <SelectTrigger className="h-9 w-auto gap-1.5 rounded-lg border-border bg-card text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All apps</SelectItem>
                  {distinctAppNames.map((name) => (
                    <SelectItem key={name} value={name}>{name}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
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
            <div className="px-6 md:px-8 py-4 space-y-6">
              {Object.entries(groupedHistory).map(([date, items]) => (
                <div key={date} className="space-y-2.5">
                  <p className="px-1 text-xs font-medium text-muted-foreground">
                    {date} <span className="text-muted-foreground/50">· {items.length}</span>
                  </p>
                  <div className="overflow-hidden rounded-2xl border border-border bg-card">
                    {items.map((item) => {
                      const isFailed = item.status === 'failed';
                      // Persisted in_progress rows should already have been backend-reconciled before render.
                      const isPersistedInProgress = item.status === 'in_progress';
                      const isInProgress = reTranscribingIds.has(item.id) || isPersistedInProgress;
                      const hasOriginal = Boolean(item.writing?.ai_applied && item.writing?.original_text && item.writing.original_text !== item.text);
                      const showOriginal = hasOriginal && showOriginalIds.has(item.id);
                      const displayText = showOriginal ? item.writing!.original_text! : item.text;
                      const wordCount = displayText.trim() ? displayText.trim().split(/\s+/).length : 0;
                      const SourceIcon = sourceIcon(item.writing?.source);
                      return (
                      <div
                        key={item.id}
                        className={cn(
                          "group relative flex cursor-pointer gap-3.5 border-t border-border px-5 py-4 transition-colors first:border-t-0",
                          isFailed ? "bg-amber-500/[0.04]" : "hover:bg-muted/40",
                        )}
                        onClick={() => !isFailed && !isInProgress && handleCopy(displayText)}
                      >
                        <div
                          className={cn(
                            "mt-0.5 grid size-9 shrink-0 place-items-center rounded-xl border",
                            isInProgress
                              ? "border-sage/30 bg-sage-bg text-sage"
                              : isFailed
                              ? "border-amber-500/30 bg-amber-500/10 text-amber-500"
                              : "border-border bg-muted text-muted-foreground",
                          )}
                          title={sourceLabel(item.writing?.source)}
                        >
                          {isInProgress ? (
                            <Loader2 className="h-4 w-4 animate-spin" />
                          ) : isFailed ? (
                            <AlertTriangle className="h-4 w-4" />
                          ) : (
                            <SourceIcon className="h-4 w-4" />
                          )}
                        </div>
                        <div className="min-w-0 flex-1">
                          {isInProgress && (
                            <p className="mb-1 text-xs font-medium text-sage">
                              {isPersistedInProgress && !reTranscribingIds.has(item.id)
                                ? `Re-transcribing with ${getModelDisplayName(item.model) ?? item.model}…`
                                : `Re-transcribing with ${reTranscribingModels.get(item.id)}…`}
                            </p>
                          )}
                          {isFailed && !isInProgress && (
                            <div className="mb-1 flex flex-wrap items-center gap-2">
                              <span className="text-xs font-medium text-amber-600 dark:text-amber-400">
                                {verifiedRecordings.has(item.id)
                                  ? 'Transcription failed — recording preserved'
                                  : checkedRecordings.has(item.id)
                                    ? 'Transcription failed — recording unavailable for retry'
                                    : 'Transcription failed'}
                              </span>
                              {verifiedRecordings.has(item.id) && (
                                <button
                                  onClick={(e) => { e.stopPropagation(); void handleReTranscribe(item); }}
                                  className="inline-flex items-center gap-1 rounded-md bg-amber-500/15 px-2 py-0.5 text-xs font-medium text-amber-600 transition-colors hover:bg-amber-500/25 dark:text-amber-400"
                                >
                                  <RotateCcw className="h-3 w-3" /> Re-transcribe
                                </button>
                              )}
                            </div>
                          )}
                          {item.writing?.translation_failed && !isFailed && !isInProgress && (
                            <p className="mb-1 text-xs font-medium text-amber-600 dark:text-amber-400">
                              {item.writing.target_language
                                ? `Translation to ${item.writing.target_language} failed — saved untranslated`
                                : 'Translation failed — saved untranslated'}
                            </p>
                          )}

                          <p className={cn(
                            "text-sm leading-relaxed line-clamp-3",
                            isFailed ? "italic text-muted-foreground" : "text-foreground",
                          )}>
                            {displayText}
                          </p>

                          <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
                            <span className="font-medium text-foreground/80">{sourceLabel(item.writing?.source)}</span>
                            {item.model && (
                              <>
                                <span className="text-muted-foreground/40">·</span>
                                <span>{getModelDisplayName(item.model) ?? item.model}</span>
                              </>
                            )}
                            <span className="text-muted-foreground/40">·</span>
                            <span>
                              {new Date(item.timestamp).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}
                            </span>
                            {wordCount > 0 && (
                              <>
                                <span className="text-muted-foreground/40">·</span>
                                <span>{wordCount} words</span>
                              </>
                            )}
                            {item.writing?.audio_duration_ms != null && (
                              <>
                                <span className="text-muted-foreground/40">·</span>
                                <span>{formatDurationMs(item.writing.audio_duration_ms)}</span>
                              </>
                            )}
                            {item.writing?.diarized && (
                              <span className="rounded-md bg-muted px-1.5 py-0.5 text-[11px] font-medium">Speakers</span>
                            )}
                            {item.writing?.context_hint?.app_name && (
                              <span className="rounded-md bg-muted px-1.5 py-0.5 text-[11px] font-medium">
                                {item.writing.context_hint.app_name}
                              </span>
                            )}
                            {hasOriginal && (
                              <button
                                onClick={(e) => {
                                  e.stopPropagation();
                                  setShowOriginalIds(prev => {
                                    const next = new Set(prev);
                                    if (next.has(item.id)) { next.delete(item.id); } else { next.add(item.id); }
                                    return next;
                                  });
                                }}
                                className="text-[11px] font-medium text-sage hover:underline"
                                title={showOriginal ? "Show AI-formatted text" : "Show original text before AI formatting"}
                              >
                                {showOriginal ? "Show formatted" : "Show original"}
                              </button>
                            )}
                          </div>
                        </div>

                        <div className="flex shrink-0 items-start gap-1 opacity-0 transition-opacity group-hover:opacity-100">
                          {!isInProgress && (
                            <button
                              onClick={(e) => { e.stopPropagation(); handleCopy(displayText); }}
                              className="grid size-7 place-items-center rounded-md border border-border bg-card text-muted-foreground transition-colors hover:text-foreground"
                              title="Copy"
                            >
                              <Copy className="h-3.5 w-3.5" />
                            </button>
                          )}
                          {verifiedRecordings.has(item.id) && (
                            <button
                              onClick={(e) => { e.stopPropagation(); handleShowInFolder(item); }}
                              className="grid size-7 place-items-center rounded-md border border-border bg-card text-muted-foreground transition-colors hover:text-foreground"
                              title="Show recording in folder"
                            >
                              <FolderOpen className="h-3.5 w-3.5" />
                            </button>
                          )}
                          {verifiedRecordings.has(item.id) && (
                            <button
                              onClick={(e) => { e.stopPropagation(); void handleReTranscribe(item); }}
                              disabled={isInProgress}
                              className={cn(
                                "grid size-7 place-items-center rounded-md border border-border bg-card text-muted-foreground transition-colors hover:text-foreground",
                                isInProgress && "pointer-events-none",
                              )}
                              title="Re-transcribe with current source"
                            >
                              {isInProgress ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RotateCcw className="h-3.5 w-3.5" />}
                            </button>
                          )}
                          <button
                            onClick={(e) => handleDelete(e, item.id)}
                            className="grid size-7 place-items-center rounded-md border border-border bg-card text-muted-foreground transition-colors hover:border-destructive/40 hover:text-destructive"
                            title="Delete"
                          >
                            <Trash2 className="h-3.5 w-3.5" />
                          </button>
                        </div>
                      </div>
                    )})}
                  </div>
                </div>
              ))}
              {filteredHistory.length > visibleCount && (
                <div className="flex justify-center pt-1">
                  <Button variant="secondary" size="sm" onClick={() => setVisibleCount((c) => c + 60)}>
                    Load more · showing {visibleCount} of {filteredHistory.length}
                  </Button>
                </div>
              )}
              <p className="flex items-center gap-2 pt-1 text-xs text-muted-foreground">
                <ShieldCheck className="h-3.5 w-3.5 text-sage" />
                Every transcript stays on this {isMacOS ? "Mac" : "PC"}. Nothing syncs to a cloud.
              </p>
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
