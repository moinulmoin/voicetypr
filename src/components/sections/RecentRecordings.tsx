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
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { formatHotkey } from "@/lib/hotkey-utils";
import { TranscriptionHistory } from "@/types";
import { useCanAutoInsert, useReadiness } from "@/contexts/ReadinessContext";
import { useModelManagementContext } from "@/contexts/ModelManagementContext";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { AlertCircle, AlertTriangle, Mic, Trash2, Search, Copy, Calendar, Download, RotateCcw, Loader2, FolderOpen, Server, Cpu, Cloud, HelpCircle } from "lucide-react";
import { useState, useMemo, useCallback, useEffect, useRef } from "react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { getModelDisplayName } from "@/lib/model-display";
import { isMacOS } from "@/lib/platform";

// Interface for available transcription sources
interface TranscriptionSource {
  id: string;
  name: string;
  type: 'local' | 'cloud' | 'remote';
}

// Interface for remote server connection
interface SavedConnection {
  id: string;
  name: string;
  host: string;
  port: number;
  model?: string;
  status?: "Unknown" | "Online" | "Offline" | "AuthFailed" | "SelfConnection";
}


interface RecentRecordingsProps {
  history: TranscriptionHistory[];
  hotkey?: string;
  onHistoryUpdate?: () => void;
}

export function RecentRecordings({ history, hotkey = "Cmd+Shift+Space", onHistoryUpdate }: RecentRecordingsProps) {
  const [, setDropdownOpenId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [transcriptionSources, setTranscriptionSources] = useState<TranscriptionSource[]>([]);
  const [loadingSources, setLoadingSources] = useState(false);
  const [reTranscribingIds, setReTranscribingIds] = useState<Set<string>>(new Set());
  const [verifiedRecordings, setVerifiedRecordings] = useState<Set<string>>(new Set());
  const [checkedRecordings, setCheckedRecordings] = useState<Set<string>>(new Set());
  // Track which items are being re-transcribed and with which model
  const { modelOrder } = useModelManagementContext();
  const [reTranscribingModels, setReTranscribingModels] = useState<Map<string, string>>(new Map());
  const onlineServersRef = useRef<Map<string, { name: string; model: string }>>(new Map()); // serverId -> { name, model }
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

  const refreshOnlineRemoteServers = useCallback(async () => {
    try {
      const servers = await invoke<SavedConnection[]>("list_remote_servers");
      const checks = servers.map(async (server) => {
        try {
          const updated = await invoke<SavedConnection>("check_remote_server_status", { serverId: server.id });
          const displayName = server.name || `${server.host}:${server.port}`;
          return {
            id: server.id,
            name: displayName,
            model: updated.model || "",
            online: updated.status === "Online",
          };
        } catch {
          return { id: server.id, name: server.name || `${server.host}:${server.port}`, model: '', online: false };
        }
      });

      const results = await Promise.all(checks);
      const onlineMap = new Map<string, { name: string; model: string }>();
      const onlineSources: TranscriptionSource[] = [];

      for (const result of results) {
        if (!result.online) continue;

        const modelDisplayName = getModelDisplayName(result.model) ?? result.model;
        onlineMap.set(result.id, { name: result.name, model: result.model });
        onlineSources.push({
          id: `remote:${result.id}`,
          name: modelDisplayName ? `${result.name} - ${modelDisplayName}` : result.name,
          type: 'remote',
        });
      }

      onlineServersRef.current = onlineMap;
      return onlineSources;
    } catch (error) {
      console.error("Failed to check remote servers:", error);
      onlineServersRef.current = new Map();
      return [] as TranscriptionSource[];
    }
  }, []);

  // Check remote server connectivity in the background (runs once on mount)
  useEffect(() => {
    refreshOnlineRemoteServers();
  }, [refreshOnlineRemoteServers]);

  // Verify which recordings exist on filesystem
  useEffect(() => {
    const verifyRecordings = async () => {
      console.log("[RecentRecordings] Starting verification for", history.length, "items");
      const verified = new Set<string>();
      const checked = new Set<string>();
      let itemsWithRecordingFile = 0;
      for (const item of history) {
        if (item.recording_file) {
          itemsWithRecordingFile++;
          console.log("[RecentRecordings] Checking recording:", item.recording_file, "for item:", item.id);
          try {
            const exists = await invoke<boolean>("check_recording_exists", {
              filename: item.recording_file
            });
            checked.add(item.id);
            console.log("[RecentRecordings] Recording", item.recording_file, "exists:", exists);
            if (exists) {
              verified.add(item.id);
            }
          } catch (error) {
            console.error(`Failed to verify recording ${item.recording_file}:`, error);
          }
        }
      }
      console.log("[RecentRecordings] Verification complete. Items with recording_file:", itemsWithRecordingFile, "Verified:", verified.size);
      setCheckedRecordings(checked);
      setVerifiedRecordings(verified);
    };
    verifyRecordings();
  }, [history]);

  // Fetch available transcription sources (local models and all remote servers)
  const fetchTranscriptionSources = useCallback(async () => {
    setLoadingSources(true);
    const sources: TranscriptionSource[] = [];

    try {
      const response = await invoke<{models: {name: string; downloaded: boolean; engine: string; kind?: string; requires_setup?: boolean; display_name?: string}[]}>('get_model_status');
      const availableSources = response.models.filter(m =>
        m.downloaded && !(m.requires_setup ?? false)
      );
      for (const model of availableSources) {
        const sourceType = model.kind === 'cloud' ? 'cloud' : 'local';
        const fallbackName = getModelDisplayName(model.name) ?? model.name;
        const sourceName = model.display_name || (sourceType === 'cloud' ? `${fallbackName} (Cloud)` : fallbackName);
        sources.push({
          id: `${sourceType}:${model.name}:${model.engine}`,
          name: sourceName,
          type: sourceType,
        });
      }
    } catch (error) {
      console.error("Failed to fetch local models:", error);
    }

    sources.push(...await refreshOnlineRemoteServers());

    setTranscriptionSources(sources);
    setLoadingSources(false);
  }, [refreshOnlineRemoteServers]);

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
      console.error("Failed to show recording in folder:", error);
      toast.error("Failed to open file location");
    }
  }, []);

  // Handle re-transcription
  const handleReTranscribe = async (item: TranscriptionHistory, sourceId: string) => {
    if (!item.recording_file) {
      toast.error("No recording file available for re-transcription");
      return;
    }

    // Parse source ID - format is "local:modelName:engine" or "remote:serverId"
    const parts = sourceId.split(':');
    const sourceType = parts[0];
    const modelNameOrServerId = parts[1];
    const engine = parts[2]; // undefined for remote

    let displayModelName: string;
    if (sourceType === 'local') {
      displayModelName = getModelDisplayName(modelNameOrServerId) ?? modelNameOrServerId;
    } else if (sourceType === 'cloud') {
      const source = transcriptionSources.find(s => s.id === sourceId);
      displayModelName = source ? source.name : `${getModelDisplayName(modelNameOrServerId) ?? modelNameOrServerId} (Cloud)`;
    } else {
      const server = transcriptionSources.find(s => s.id === sourceId);
      displayModelName = server ? server.name : modelNameOrServerId;
    }

    // Mark this item as re-transcribing with the model name
    setReTranscribingIds(prev => new Set(prev).add(item.id));
    setReTranscribingModels(prev => new Map(prev).set(item.id, displayModelName));

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

    const pendingModelName = sourceType === 'remote'
      ? (() => {
          const server = transcriptionSources.find(s => s.id === sourceId);
          return server ? `Remote: ${server.name}` : `Remote: ${modelNameOrServerId}`;
        })()
      : displayModelName;
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

      if (sourceType === 'local' || sourceType === 'cloud') {
        result = await invoke<string>('transcribe_audio_file', {
          filePath: fullPath,
          modelName: modelNameOrServerId,
          modelEngine: engine || null,
        });
        modelName = sourceType === 'cloud' ? displayModelName : modelNameOrServerId;
      } else if (sourceType === 'remote') {
        result = await invoke<string>('transcribe_remote', {
          serverId: modelNameOrServerId,
          audioPath: fullPath,
        });
        const server = transcriptionSources.find(s => s.id === sourceId);
        modelName = server ? `Remote: ${server.name}` : `Remote: ${modelNameOrServerId}`;
      } else {
        throw new Error(`Unknown source type: ${sourceType}`);
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
      console.error("Re-transcription failed:", error);
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
        console.error("Failed to persist retranscription error state:", updateError);
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

  // Filter history based on search query
  const filteredHistory = useMemo(() => {
    if (!searchQuery.trim()) return history;

    const query = searchQuery.toLowerCase();
    return history.filter(item =>
      item.text.toLowerCase().includes(query) ||
      (item.model && item.model.toLowerCase().includes(query)) ||
      (item.model && (getModelDisplayName(item.model) ?? item.model).toLowerCase().includes(query))
    );
  }, [history, searchQuery]);

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
      console.error("Failed to delete transcription:", error);
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
      console.error("Failed to clear all transcriptions:", error);
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
      console.error("Failed to export transcriptions:", error);
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
                    <p><strong className="text-foreground">Re-transcribe</strong> reruns a saved audio take with the selected local, cloud, or remote source when the audio file is still available.</p>
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

      {/* Search Bar */}
      {history.length > 0 && (
        <div className="px-6 py-3">
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
          {searchQuery && (
            <p className="text-xs text-muted-foreground mt-2">
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
                              <DropdownMenu onOpenChange={(open) => {
                                setDropdownOpenId(open ? item.id : null);
                                if (open) fetchTranscriptionSources();
                              }}>
                                <DropdownMenuTrigger asChild>
                                  <button
                                    onClick={(e) => e.stopPropagation()}
                                    className="flex items-center gap-1 px-2 py-1 text-xs font-medium text-amber-600 dark:text-amber-400 bg-amber-500/20 rounded hover:bg-amber-500/30 transition-colors"
                                  >
                                    <RotateCcw className="w-3 h-3" />
                                    Re-transcribe
                                  </button>
                                </DropdownMenuTrigger>
                                <DropdownMenuContent
                                  align="end"
                                  className="w-44 max-h-72 overflow-y-auto text-xs"
                                >
                                  <DropdownMenuLabel className="text-xs py-1.5 px-2 -mx-1 bg-zinc-100 dark:bg-zinc-800 font-medium">Re-transcribe using...</DropdownMenuLabel>
                                  <DropdownMenuSeparator />
                                  {loadingSources ? (
                                    <div className="flex items-center justify-center py-2">
                                      <Loader2 className="w-3 h-3 animate-spin text-muted-foreground" />
                                    </div>
                                  ) : transcriptionSources.length === 0 ? (
                                    <div className="py-2 text-center text-xs text-muted-foreground">
                                      No transcription sources available
                                    </div>
                                  ) : (
                                    <>
                                      {transcriptionSources.filter(s => s.type === 'local').length > 0 && (
                                        <DropdownMenuGroup>
                                          <DropdownMenuLabel className="text-[10px] text-muted-foreground flex items-center gap-1 py-0.5">
                                            <Cpu className="w-2.5 h-2.5" />
                                            Local Models
                                          </DropdownMenuLabel>
                                          {transcriptionSources
                                            .filter(s => s.type === 'local')
                                            .sort((a, b) => {
                                              const aModelName = a.id.split(':')[1];
                                              const bModelName = b.id.split(':')[1];
                                              const aIndex = modelOrder.indexOf(aModelName);
                                              const bIndex = modelOrder.indexOf(bModelName);
                                              const aOrder = aIndex === -1 ? 999 : aIndex;
                                              const bOrder = bIndex === -1 ? 999 : bIndex;
                                              return aOrder - bOrder;
                                            })
                                            .map(source => (
                                              <DropdownMenuItem
                                                key={source.id}
                                                className="text-xs py-1"
                                                onClick={(e) => {
                                                  e.stopPropagation();
                                                  handleReTranscribe(item, source.id);
                                                }}
                                              >
                                                {source.name}
                                              </DropdownMenuItem>
                                            ))}
                                        </DropdownMenuGroup>
                                      )}
                                      {transcriptionSources.filter(s => s.type === 'cloud').length > 0 && (
                                        <>
                                          <DropdownMenuSeparator />
                                          <DropdownMenuGroup>
                                            <DropdownMenuLabel className="text-[10px] text-muted-foreground flex items-center gap-1 py-0.5">
                                              <Cloud className="w-2.5 h-2.5" />
                                              Cloud Providers
                                            </DropdownMenuLabel>
                                            {transcriptionSources
                                              .filter(s => s.type === 'cloud')
                                              .map(source => (
                                                <DropdownMenuItem
                                                  key={source.id}
                                                  className="text-xs py-1"
                                                  onClick={(e) => {
                                                    e.stopPropagation();
                                                    handleReTranscribe(item, source.id);
                                                  }}
                                                >
                                                  {source.name}
                                                </DropdownMenuItem>
                                              ))}
                                          </DropdownMenuGroup>
                                        </>
                                      )}
                                      {transcriptionSources.filter(s => s.type === 'remote').length > 0 && (
                                        <>
                                          <DropdownMenuSeparator />
                                          <DropdownMenuGroup>
                                            <DropdownMenuLabel className="text-[10px] text-muted-foreground flex items-center gap-1 py-0.5">
                                              <Server className="w-2.5 h-2.5" />
                                              Remote Servers
                                            </DropdownMenuLabel>
                                            {transcriptionSources
                                              .filter(s => s.type === 'remote')
                                              .map(source => (
                                                <DropdownMenuItem
                                                  key={source.id}
                                                  className="text-xs py-1"
                                                  onClick={(e) => {
                                                    e.stopPropagation();
                                                    handleReTranscribe(item, source.id);
                                                  }}
                                                >
                                                  {source.name}
                                                </DropdownMenuItem>
                                              ))}
                                          </DropdownMenuGroup>
                                        </>
                                      )}
                                    </>
                                  )}
                                </DropdownMenuContent>
                              </DropdownMenu>
                            )}
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
                                <DropdownMenu onOpenChange={(open) => {
                                  setDropdownOpenId(open ? item.id : null);
                                  if (open) fetchTranscriptionSources();
                                }}>
                                  <DropdownMenuTrigger asChild>
                                    <button
                                      onClick={(e) => e.stopPropagation()}
                                      className={cn(
                                        "p-1.5 rounded hover:bg-accent transition-colors",
                                        isInProgress && "pointer-events-none"
                                      )}
                                      title="Re-transcribe"
                                      disabled={isInProgress}
                                    >
                                      {isInProgress ? (
                                        <Loader2 className="w-4 h-4 text-muted-foreground animate-spin" />
                                      ) : (
                                        <RotateCcw className="w-4 h-4 text-muted-foreground" />
                                      )}
                                    </button>
                                  </DropdownMenuTrigger>
                                  <DropdownMenuContent
                                    align="end"
                                    className="w-44 max-h-72 overflow-y-auto text-xs"
                                  >
                                    <DropdownMenuLabel className="text-xs py-1.5 px-2 -mx-1 bg-zinc-100 dark:bg-zinc-800 font-medium">Re-transcribe using...</DropdownMenuLabel>
                                    <DropdownMenuSeparator />
                                    {loadingSources ? (
                                      <div className="flex items-center justify-center py-2">
                                        <Loader2 className="w-3 h-3 animate-spin text-muted-foreground" />
                                      </div>
                                    ) : transcriptionSources.length === 0 ? (
                                      <div className="py-2 text-center text-xs text-muted-foreground">
                                        No transcription sources available
                                      </div>
                                    ) : (
                                      <>
                                        {transcriptionSources.filter(s => s.type === 'local').length > 0 && (
                                          <DropdownMenuGroup>
                                            <DropdownMenuLabel className="text-[10px] text-muted-foreground flex items-center gap-1 py-0.5">
                                              <Cpu className="w-2.5 h-2.5" />
                                              Local Models
                                            </DropdownMenuLabel>
                                            {transcriptionSources
                                              .filter(s => s.type === 'local')
                                              .sort((a, b) => {
                                                const aModelName = a.id.split(':')[1];
                                                const bModelName = b.id.split(':')[1];
                                                const aIndex = modelOrder.indexOf(aModelName);
                                                const bIndex = modelOrder.indexOf(bModelName);
                                                const aOrder = aIndex === -1 ? 999 : aIndex;
                                                const bOrder = bIndex === -1 ? 999 : bIndex;
                                                return aOrder - bOrder;
                                              })
                                              .map(source => (
                                                <DropdownMenuItem
                                                  key={source.id}
                                                  className="text-xs py-1"
                                                  onClick={(e) => {
                                                    e.stopPropagation();
                                                    handleReTranscribe(item, source.id);
                                                  }}
                                                >
                                                  {source.name}
                                                </DropdownMenuItem>
                                              ))}
                                          </DropdownMenuGroup>
                                        )}
                                        {transcriptionSources.filter(s => s.type === 'cloud').length > 0 && (
                                          <>
                                            <DropdownMenuSeparator />
                                            <DropdownMenuGroup>
                                              <DropdownMenuLabel className="text-[10px] text-muted-foreground flex items-center gap-1 py-0.5">
                                                <Cloud className="w-2.5 h-2.5" />
                                                Cloud Providers
                                              </DropdownMenuLabel>
                                              {transcriptionSources
                                                .filter(s => s.type === 'cloud')
                                                .map(source => (
                                                  <DropdownMenuItem
                                                    key={source.id}
                                                    className="text-xs py-1"
                                                    onClick={(e) => {
                                                      e.stopPropagation();
                                                      handleReTranscribe(item, source.id);
                                                    }}
                                                  >
                                                    {source.name}
                                                  </DropdownMenuItem>
                                                ))}
                                            </DropdownMenuGroup>
                                          </>
                                        )}
                                        {transcriptionSources.filter(s => s.type === 'remote').length > 0 && (
                                          <>
                                            <DropdownMenuSeparator />
                                            <DropdownMenuGroup>
                                              <DropdownMenuLabel className="text-[10px] text-muted-foreground flex items-center gap-1 py-0.5">
                                                <Server className="w-2.5 h-2.5" />
                                                Remote Servers
                                              </DropdownMenuLabel>
                                              {transcriptionSources
                                                .filter(s => s.type === 'remote')
                                                .map(source => (
                                                  <DropdownMenuItem
                                                    key={source.id}
                                                    className="text-xs py-1"
                                                    onClick={(e) => {
                                                      e.stopPropagation();
                                                      handleReTranscribe(item, source.id);
                                                    }}
                                                  >
                                                    {source.name}
                                                  </DropdownMenuItem>
                                                ))}
                                            </DropdownMenuGroup>
                                          </>
                                        )}
                                      </>
                                    )}
                                  </DropdownMenuContent>
                                </DropdownMenu>
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
                Try adjusting your search query
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
