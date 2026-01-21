import { ScrollArea } from "@/components/ui/scroll-area";
import { Button } from "@/components/ui/button";
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
import { useCanRecord, useCanAutoInsert } from "@/contexts/ReadinessContext";
import { useModelManagementContext } from "@/contexts/ModelManagementContext";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { AlertCircle, AlertTriangle, Mic, Trash2, Search, Copy, Calendar, Download, RotateCcw, Loader2, FolderOpen, Server, Cpu } from "lucide-react";
import { useState, useMemo, useCallback, useEffect, useRef } from "react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";

// Interface for available transcription sources
interface TranscriptionSource {
  id: string;
  name: string;
  type: 'local' | 'remote';
}

// Interface for remote server connection
interface SavedConnection {
  id: string;
  name: string;
  host: string;
  port: number;
  model?: string;
}

// Interface for remote server status response
interface StatusResponse {
  model: string;
  engine: string;
}

// Static mapping for model display names
const MODEL_DISPLAY_NAMES: Record<string, string> = {
  // Turbo models
  'large-v3-turbo': 'Large v3 Turbo',
  'large-v3-turbo-q8_0': 'Large v3 Turbo (Q8)',
  // Large models
  'large-v3': 'Large v3',
  'large-v3-q5_0': 'Large v3 (Q5)',
  // Small models
  'small.en': 'Small (English)',
  'small': 'Small',
  // Base models
  'base.en': 'Base (English)',
  'base': 'Base',
  // Tiny models
  'tiny.en': 'Tiny (English)',
  'tiny': 'Tiny',
  // Parakeet models
  'parakeet-tdt-0.6b-v2': 'Parakeet V2 (English)',
  'parakeet-tdt-0.6b-v3': 'Parakeet V3',
};

interface RecentRecordingsProps {
  history: TranscriptionHistory[];
  hotkey?: string;
  onHistoryUpdate?: () => void;
}

export function RecentRecordings({ history, hotkey = "Cmd+Shift+Space", onHistoryUpdate }: RecentRecordingsProps) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [dropdownOpenId, setDropdownOpenId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [transcriptionSources, setTranscriptionSources] = useState<TranscriptionSource[]>([]);
  const [loadingSources, setLoadingSources] = useState(false);
  const [reTranscribingIds, setReTranscribingIds] = useState<Set<string>>(new Set());
  const [verifiedRecordings, setVerifiedRecordings] = useState<Set<string>>(new Set());
  // Track which items are being re-transcribed and with which model
  const { modelOrder } = useModelManagementContext();
  const [reTranscribingModels, setReTranscribingModels] = useState<Map<string, string>>(new Map());
  const onlineServersRef = useRef<Map<string, { name: string; model: string }>>(new Map()); // serverId -> { name, model }
  const canRecord = useCanRecord();
  const canAutoInsert = useCanAutoInsert();

  // Check remote server connectivity in the background (runs once on mount)
  useEffect(() => {
    const checkRemoteServers = async () => {
      try {
        const servers = await invoke<SavedConnection[]>("list_remote_servers");
        // Check each server in parallel
        const checks = servers.map(async (server) => {
          try {
            const status = await invoke<StatusResponse>("test_remote_server", { serverId: server.id });
            // Use server name or fallback to host:port
            const displayName = server.name || `${server.host}:${server.port}`;
            return { id: server.id, name: displayName, model: status.model, online: true };
          } catch {
            return { id: server.id, name: server.name || '', model: '', online: false };
          }
        });
        const results = await Promise.all(checks);
        // Update the cache with online servers
        const onlineMap = new Map<string, { name: string; model: string }>();
        for (const result of results) {
          if (result.online) {
            onlineMap.set(result.id, { name: result.name, model: result.model });
          }
        }
        onlineServersRef.current = onlineMap;
      } catch (error) {
        console.error("Failed to check remote servers:", error);
      }
    };
    checkRemoteServers();
  }, []);

  // Verify which recordings exist on filesystem
  useEffect(() => {
    const verifyRecordings = async () => {
      console.log("[RecentRecordings] Starting verification for", history.length, "items");
      const verified = new Set<string>();
      let itemsWithRecordingFile = 0;
      for (const item of history) {
        if (item.recording_file) {
          itemsWithRecordingFile++;
          console.log("[RecentRecordings] Checking recording:", item.recording_file, "for item:", item.id);
          try {
            const exists = await invoke<boolean>("check_recording_exists", {
              filename: item.recording_file
            });
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
      setVerifiedRecordings(verified);
    };
    verifyRecordings();
  }, [history]);

  // Fetch available transcription sources (local models and all remote servers)
  const fetchTranscriptionSources = useCallback(async () => {
    setLoadingSources(true);
    const sources: TranscriptionSource[] = [];

    try {
      // Fetch local models using get_model_status (this is fast)
      const response = await invoke<{models: {name: string; downloaded: boolean; engine: string}[]}>("get_model_status");
      // Filter to downloaded local models (whisper and parakeet, not cloud/soniox)
      const downloadedLocalModels = response.models.filter(m =>
        m.downloaded && (m.engine === 'whisper' || m.engine === 'parakeet')
      );
      for (const model of downloadedLocalModels) {
        sources.push({
          id: `local:${model.name}:${model.engine}`,
          name: MODEL_DISPLAY_NAMES[model.name] || model.name,
          type: 'local',
        });
      }
    } catch (error) {
      console.error("Failed to fetch local models:", error);
    }

    // Fetch all configured remote servers (not just cached online ones)
    try {
      const servers = await invoke<SavedConnection[]>("list_remote_servers");
      for (const server of servers) {
        // Use cached info if available (has model name), otherwise just show server name
        const cachedInfo = onlineServersRef.current.get(server.id);
        const displayName = cachedInfo?.model
          ? `${server.name || `${server.host}:${server.port}`} - ${cachedInfo.model}`
          : server.name || `${server.host}:${server.port}`;
        sources.push({
          id: `remote:${server.id}`,
          name: displayName,
          type: 'remote',
        });
      }
    } catch (error) {
      console.error("Failed to fetch remote servers:", error);
    }

    setTranscriptionSources(sources);
    setLoadingSources(false);
  }, []);

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

    // Get the display name for the model being used
    let displayModelName: string;
    if (sourceType === 'local') {
      displayModelName = MODEL_DISPLAY_NAMES[modelNameOrServerId] || modelNameOrServerId;
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

    try {
      // Get recordings directory to build full path
      const recordingsDir = await invoke<string>("get_recordings_directory");
      const separator = recordingsDir.includes('\\') ? '\\' : '/';
      const fullPath = `${recordingsDir}${separator}${item.recording_file}`;

      let result: string;
      let modelName: string;

      if (sourceType === 'local') {
        // Re-transcribe using local model (whisper or parakeet)
        result = await invoke<string>("transcribe_audio_file", {
          filePath: fullPath,
          modelName: modelNameOrServerId,
          modelEngine: engine || null,
        });
        modelName = modelNameOrServerId;
      } else if (sourceType === 'remote') {
        // Re-transcribe using remote server via dedicated command
        result = await invoke<string>("transcribe_remote", {
          serverId: modelNameOrServerId,
          audioPath: fullPath,
        });
        // Find the server name for the model display
        const server = transcriptionSources.find(s => s.id === sourceId);
        modelName = server ? `Remote: ${server.name}` : `Remote: ${modelNameOrServerId}`;
      } else {
        throw new Error(`Unknown source type: ${sourceType}`);
      }

      // Update the existing transcription entry in place
      await invoke("update_transcription", {
        timestamp: item.id,
        text: result,
        model: modelName,
      });

      // Clear the re-transcribing state
      cleanup();

      // Refresh history to show the updated item
      if (onHistoryUpdate) {
        onHistoryUpdate();
      }
    } catch (error) {
      console.error("Re-transcription failed:", error);
      toast.error("Re-transcription failed", {
        description: String(error)
      });
      // Clear pending on error too
      cleanup();
    }
  };

  // Filter history based on search query
  const filteredHistory = useMemo(() => {
    if (!searchQuery.trim()) return history;

    const query = searchQuery.toLowerCase();
    return history.filter(item =>
      item.text.toLowerCase().includes(query) ||
      (item.model && item.model.toLowerCase().includes(query))
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
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold">History</h1>
            <p className="text-sm text-muted-foreground mt-1">
              {history.length} total transcription{history.length !== 1 ? 's' : ''}
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
                      return (
                      <div
                        key={item.id}
                        className={cn(
                          "group relative rounded-lg cursor-pointer",
                          "bg-card border",
                          reTranscribingIds.has(item.id)
                            ? "border-primary/50"
                            : isFailed
                            ? "border-amber-500/50 bg-amber-500/5"
                            : "border-border/50 hover:bg-accent/30 hover:border-border",
                          "transition-all duration-200"
                        )}
                        onClick={() => !isFailed && handleCopy(item.text)}
                        onMouseEnter={() => setHoveredId(item.id)}
                        onMouseLeave={() => setHoveredId(null)}
                      >
                        {/* Re-transcribing status bar */}
                        {reTranscribingIds.has(item.id) && (
                          <div className="flex items-center gap-2 px-4 py-2 bg-primary/10 border-b border-primary/20 rounded-t-lg">
                            <Loader2 className="w-3.5 h-3.5 animate-spin text-primary" />
                            <span className="text-xs text-primary font-medium">
                              Re-transcribing with {reTranscribingModels.get(item.id)}...
                            </span>
                          </div>
                        )}
                        {/* Failed status bar */}
                        {isFailed && !reTranscribingIds.has(item.id) && (
                          <div className="flex items-center justify-between gap-2 px-4 py-2 bg-amber-500/10 border-b border-amber-500/20 rounded-t-lg">
                            <div className="flex items-center gap-2">
                              <AlertTriangle className="w-3.5 h-3.5 text-amber-500" />
                              <span className="text-xs text-amber-600 dark:text-amber-400 font-medium">
                                Transcription failed - recording preserved
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
                                      No models available
                                    </div>
                                  ) : (
                                    <>
                                      {/* Local models */}
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
                                      {/* Remote servers */}
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
                                <span>{MODEL_DISPLAY_NAMES[item.model] || item.model}</span>
                              )}
                              {item.model && <span className="text-muted-foreground/50">•</span>}
                              <span>
                                {new Date(item.timestamp).toLocaleDateString([], { month: 'short', day: 'numeric' })}{' '}
                                {new Date(item.timestamp).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}
                              </span>
                            </div>
                            <div className={cn(
                              "flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity",
                              (hoveredId === item.id || dropdownOpenId === item.id) && "opacity-100"
                            )}>
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
                              {/* Show in folder button - only show if recording file exists and verified */}
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
                                        reTranscribingIds.has(item.id) && "pointer-events-none"
                                      )}
                                      title="Re-transcribe"
                                      disabled={reTranscribingIds.has(item.id)}
                                    >
                                      {reTranscribingIds.has(item.id) ? (
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
                                        No models available
                                      </div>
                                    ) : (
                                      <>
                                        {/* Local models */}
                                        {transcriptionSources.filter(s => s.type === 'local').length > 0 && (
                                          <DropdownMenuGroup>
                                            <DropdownMenuLabel className="text-[10px] text-muted-foreground flex items-center gap-1 py-0.5">
                                              <Cpu className="w-2.5 h-2.5" />
                                              Local Models
                                            </DropdownMenuLabel>
                                            {transcriptionSources
                                              .filter(s => s.type === 'local')
                                              .sort((a, b) => {
                                                // Sort by modelOrder from context
                                                const aModelName = a.id.split(':')[1];
                                                const bModelName = b.id.split(':')[1];
                                                const aIndex = modelOrder.indexOf(aModelName);
                                                const bIndex = modelOrder.indexOf(bModelName);
                                                // If not found in modelOrder, put at end
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
                                        {/* Remote servers */}
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
                    Press {formatHotkey(hotkey)} to start recording
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
                  Check permissions and download a model in Settings
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