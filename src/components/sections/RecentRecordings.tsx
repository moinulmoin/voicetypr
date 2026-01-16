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
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { AlertCircle, Mic, Trash2, Search, Copy, Calendar, Download, RotateCcw, Loader2, Server, Cpu, Volume2, VolumeX } from "lucide-react";
import { useState, useMemo, useCallback, useEffect, useRef } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { cn } from "@/lib/utils";

// Interface for available transcription sources
interface TranscriptionSource {
  id: string;
  name: string;
  type: 'local' | 'remote';
  available: boolean;
}

// Interface for remote server connection
interface SavedConnection {
  id: string;
  name: string;
  host: string;
  port: number;
  is_active: boolean;
}

// Interface for local model
interface LocalModel {
  name: string;
  downloaded: boolean;
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
};

interface RecentRecordingsProps {
  history: TranscriptionHistory[];
  hotkey?: string;
  onHistoryUpdate?: () => void;
}

export function RecentRecordings({ history, hotkey = "Cmd+Shift+Space", onHistoryUpdate }: RecentRecordingsProps) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [transcriptionSources, setTranscriptionSources] = useState<TranscriptionSource[]>([]);
  const [loadingSources, setLoadingSources] = useState(false);
  const [reTranscribingId, setReTranscribingId] = useState<string | null>(null);
  const [playingAudioId, setPlayingAudioId] = useState<string | null>(null);
  const [verifiedRecordings, setVerifiedRecordings] = useState<Set<string>>(new Set());
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const canRecord = useCanRecord();
  const canAutoInsert = useCanAutoInsert();

  // Verify which recordings exist on filesystem
  useEffect(() => {
    const verifyRecordings = async () => {
      const verified = new Set<string>();
      for (const item of history) {
        if (item.recording_file) {
          try {
            const exists = await invoke<boolean>("check_recording_exists", {
              filename: item.recording_file
            });
            if (exists) {
              verified.add(item.id);
            }
          } catch (error) {
            console.error(`Failed to verify recording ${item.recording_file}:`, error);
          }
        }
      }
      setVerifiedRecordings(verified);
    };
    verifyRecordings();
  }, [history]);

  // Fetch available transcription sources (local models + remote servers)
  const fetchTranscriptionSources = useCallback(async () => {
    setLoadingSources(true);
    const sources: TranscriptionSource[] = [];

    try {
      // Fetch local Whisper models
      const models = await invoke<LocalModel[]>("check_whisper_models");
      const downloadedModels = models.filter(m => m.downloaded);
      for (const model of downloadedModels) {
        sources.push({
          id: `local:${model.name}`,
          name: MODEL_DISPLAY_NAMES[model.name] || model.name,
          type: 'local',
          available: true,
        });
      }
    } catch (error) {
      console.error("Failed to fetch local models:", error);
    }

    try {
      // Fetch remote servers
      const servers = await invoke<SavedConnection[]>("list_remote_servers");
      for (const server of servers) {
        // Test if server is online
        let available = false;
        try {
          await invoke("test_remote_server", { serverId: server.id });
          available = true;
        } catch {
          // Server is offline
        }
        sources.push({
          id: `remote:${server.id}`,
          name: server.name,
          type: 'remote',
          available,
        });
      }
    } catch (error) {
      console.error("Failed to fetch remote servers:", error);
    }

    setTranscriptionSources(sources);
    setLoadingSources(false);
  }, []);

  // Handle audio playback
  const handlePlayAudio = useCallback(async (item: TranscriptionHistory) => {
    if (!item.recording_file) return;

    // If already playing this item, stop it
    if (playingAudioId === item.id && audioRef.current) {
      audioRef.current.pause();
      audioRef.current = null;
      setPlayingAudioId(null);
      return;
    }

    // Stop any currently playing audio
    if (audioRef.current) {
      audioRef.current.pause();
      audioRef.current = null;
    }

    try {
      // Get the full path and convert to asset URL
      const fullPath = await invoke<string>("get_recording_path", {
        filename: item.recording_file
      });
      const assetUrl = convertFileSrc(fullPath);

      // Create and play audio
      const audio = new Audio(assetUrl);
      audioRef.current = audio;
      setPlayingAudioId(item.id);

      audio.onended = () => {
        setPlayingAudioId(null);
        audioRef.current = null;
      };

      audio.onerror = () => {
        toast.error("Failed to play audio");
        setPlayingAudioId(null);
        audioRef.current = null;
      };

      await audio.play();
    } catch (error) {
      console.error("Failed to play audio:", error);
      toast.error("Failed to play audio");
      setPlayingAudioId(null);
    }
  }, [playingAudioId]);

  // Handle re-transcription
  const handleReTranscribe = async (item: TranscriptionHistory, sourceId: string) => {
    if (!item.recording_file) {
      toast.error("No recording file available for re-transcription");
      return;
    }

    setReTranscribingId(item.id);

    try {
      const [sourceType, sourceIdentifier] = sourceId.split(':');

      // Get recordings directory to build full path
      const recordingsDir = await invoke<string>("get_recordings_directory");
      const separator = recordingsDir.includes('\\') ? '\\' : '/';
      const fullPath = `${recordingsDir}${separator}${item.recording_file}`;

      let result: string;
      let modelName: string;

      if (sourceType === 'local') {
        // Re-transcribe using local model
        result = await invoke<string>("transcribe_audio_file", {
          filePath: fullPath,
          modelName: sourceIdentifier,
          modelEngine: null,
        });
        modelName = sourceIdentifier;
      } else if (sourceType === 'remote') {
        // Re-transcribe using remote server
        result = await invoke<string>("transcribe_audio_file", {
          filePath: fullPath,
          modelName: `Remote:${sourceIdentifier}`,
          modelEngine: "remote",
        });
        // Find the server name for the model display
        const server = transcriptionSources.find(s => s.id === sourceId);
        modelName = server ? `Remote: ${server.name}` : `Remote: ${sourceIdentifier}`;
      } else {
        throw new Error(`Unknown source type: ${sourceType}`);
      }

      // Save the re-transcription as a new history entry
      await invoke("save_retranscription", {
        text: result,
        model: modelName,
        recordingFile: item.recording_file,
        sourceRecordingId: item.id,
      });

      toast.success("Re-transcription completed", {
        description: `${result.length} characters transcribed`
      });

      // Refresh history to show the new entry
      if (onHistoryUpdate) {
        onHistoryUpdate();
      }
    } catch (error) {
      console.error("Re-transcription failed:", error);
      toast.error("Re-transcription failed", {
        description: String(error)
      });
    } finally {
      setReTranscribingId(null);
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
                    {items.map((item) => (
                      <div
                        key={item.id}
                        className={cn(
                          "group relative p-4 rounded-lg cursor-pointer",
                          "bg-card border border-border/50",
                          "hover:bg-accent/30 hover:border-border",
                          "transition-all duration-200"
                        )}
                        onClick={() => handleCopy(item.text)}
                        onMouseEnter={() => setHoveredId(item.id)}
                        onMouseLeave={() => setHoveredId(null)}
                      >
                        <div className="flex items-start justify-between gap-4">
                          <div className="flex-1 min-w-0">
                            <p className="text-sm text-foreground leading-relaxed line-clamp-5">
                              {item.text}
                            </p>
                            {item.model && (
                              <div className="mt-2">
                                <span className="text-xs text-muted-foreground">
                                  {MODEL_DISPLAY_NAMES[item.model] || item.model}
                                </span>
                              </div>
                            )}
                          </div>
                          <div className={cn(
                            "flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity",
                            hoveredId === item.id && "opacity-100"
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
                            {/* Audio playback button - only show if recording file exists and verified */}
                            {verifiedRecordings.has(item.id) && (
                              <button
                                onClick={(e) => {
                                  e.stopPropagation();
                                  handlePlayAudio(item);
                                }}
                                className="p-1.5 rounded hover:bg-accent transition-colors"
                                title={playingAudioId === item.id ? "Stop" : "Play audio"}
                              >
                                {playingAudioId === item.id ? (
                                  <VolumeX className="w-4 h-4 text-primary" />
                                ) : (
                                  <Volume2 className="w-4 h-4 text-muted-foreground" />
                                )}
                              </button>
                            )}
                            {/* Re-transcribe button - only show if recording file exists and verified */}
                            {verifiedRecordings.has(item.id) && (
                              <DropdownMenu onOpenChange={(open) => open && fetchTranscriptionSources()}>
                                <DropdownMenuTrigger asChild>
                                  <button
                                    onClick={(e) => e.stopPropagation()}
                                    className={cn(
                                      "p-1.5 rounded hover:bg-accent transition-colors",
                                      reTranscribingId === item.id && "pointer-events-none"
                                    )}
                                    title="Re-transcribe"
                                    disabled={reTranscribingId === item.id}
                                  >
                                    {reTranscribingId === item.id ? (
                                      <Loader2 className="w-4 h-4 text-muted-foreground animate-spin" />
                                    ) : (
                                      <RotateCcw className="w-4 h-4 text-muted-foreground" />
                                    )}
                                  </button>
                                </DropdownMenuTrigger>
                                <DropdownMenuContent align="end" className="w-56">
                                  <DropdownMenuLabel>Re-transcribe using...</DropdownMenuLabel>
                                  <DropdownMenuSeparator />
                                  {loadingSources ? (
                                    <div className="flex items-center justify-center py-4">
                                      <Loader2 className="w-4 h-4 animate-spin text-muted-foreground" />
                                      <span className="ml-2 text-sm text-muted-foreground">Loading sources...</span>
                                    </div>
                                  ) : transcriptionSources.length === 0 ? (
                                    <div className="py-4 text-center text-sm text-muted-foreground">
                                      No transcription sources available
                                    </div>
                                  ) : (
                                    <>
                                      {/* Local models */}
                                      {transcriptionSources.filter(s => s.type === 'local').length > 0 && (
                                        <DropdownMenuGroup>
                                          <DropdownMenuLabel className="text-xs text-muted-foreground flex items-center gap-1">
                                            <Cpu className="w-3 h-3" />
                                            Local Models
                                          </DropdownMenuLabel>
                                          {transcriptionSources
                                            .filter(s => s.type === 'local')
                                            .map(source => (
                                              <DropdownMenuItem
                                                key={source.id}
                                                onClick={() => handleReTranscribe(item, source.id)}
                                                disabled={!source.available}
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
                                            <DropdownMenuLabel className="text-xs text-muted-foreground flex items-center gap-1">
                                              <Server className="w-3 h-3" />
                                              Remote Servers
                                            </DropdownMenuLabel>
                                            {transcriptionSources
                                              .filter(s => s.type === 'remote')
                                              .map(source => (
                                                <DropdownMenuItem
                                                  key={source.id}
                                                  onClick={() => handleReTranscribe(item, source.id)}
                                                  disabled={!source.available}
                                                  className={!source.available ? "opacity-50" : ""}
                                                >
                                                  <span className="flex items-center gap-2">
                                                    {source.name}
                                                    {!source.available && (
                                                      <span className="text-xs text-muted-foreground">(offline)</span>
                                                    )}
                                                  </span>
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
                    ))}
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