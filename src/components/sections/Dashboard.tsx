import { ScrollArea } from "@/components/ui/scroll-area";
import { formatHotkey } from "@/lib/hotkey-utils";
import { TranscriptionHistory } from "@/types";
import { useCanRecord, useCanAutoInsert } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { 
  AlertCircle, 
  Mic, 
  Trash2, 
  Clock,
  Sparkles,
  TrendingUp,
  Copy,
  FileText,
  Calendar,
  BarChart3
} from "lucide-react";
import { useState, useMemo } from "react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";

interface DashboardProps {
  history: TranscriptionHistory[];
  hotkey?: string;
  onHistoryUpdate?: () => void;
}

export function Dashboard({ history, hotkey = "Cmd+Shift+Space", onHistoryUpdate }: DashboardProps) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const canRecord = useCanRecord();
  const canAutoInsert = useCanAutoInsert();
  const { settings } = useSettings();

  // Calculate stats
  const stats = useMemo(() => {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const thisWeek = new Date();
    thisWeek.setDate(thisWeek.getDate() - 7);

    const todayCount = history.filter(item => 
      new Date(item.timestamp) >= today
    ).length;

    const weekCount = history.filter(item => 
      new Date(item.timestamp) >= thisWeek
    ).length;

    const totalWords = history.reduce((acc, item) => 
      acc + item.text.split(' ').length, 0
    );

    const avgLength = history.length > 0 
      ? Math.round(totalWords / history.length)
      : 0;

    return {
      todayCount,
      weekCount,
      totalWords,
      avgLength
    };
  }, [history]);

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    toast.success("Copied to clipboard");
  };

  const handleDelete = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();

    try {
      const confirmed = await ask("Are you sure you want to delete this transcription?", {
        title: "Delete Transcription",
        kind: "warning"
      });

      if (!confirmed) return;

      await invoke("delete_transcription_entry", { timestamp: id });
      toast.success("Transcription deleted");

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
      const confirmed = await ask(`Are you sure you want to delete all ${history.length} transcriptions? This action cannot be undone.`, {
        title: "Clear All Transcriptions",
        kind: "warning"
      });

      if (!confirmed) return;

      await invoke("clear_all_transcriptions");
      toast.success("All transcriptions cleared");

      if (onHistoryUpdate) {
        onHistoryUpdate();
      }
    } catch (error) {
      console.error("Failed to clear all transcriptions:", error);
      toast.error("Failed to clear all transcriptions");
    }
  };

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="px-6 py-4 border-b border-border/40">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold">Welcome back, {settings?.user_name || 'there'}</h1>
            <p className="text-sm text-muted-foreground mt-1">
              {canRecord ? 'Ready to transcribe' : 'Setup required'}
            </p>
          </div>
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-accent/50 text-sm">
              <Calendar className="h-3.5 w-3.5" />
              {new Date().toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' })}
            </div>
            {stats.todayCount > 0 && (
              <div className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-primary/10 text-sm font-medium">
                <TrendingUp className="h-3.5 w-3.5" />
                {stats.todayCount} today
              </div>
            )}
          </div>
        </div>
      </div>

      <div className="flex-1 overflow-hidden">
        <div className="h-full p-6">
          {/* Quick Stats */}
          <div className="grid grid-cols-4 gap-4 mb-6">
            <div className="p-4 rounded-lg bg-card border border-border/50">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-xs text-muted-foreground font-medium">Today</p>
                  <p className="text-2xl font-bold mt-1">{stats.todayCount}</p>
                </div>
                <Clock className="h-8 w-8 text-muted-foreground/20" />
              </div>
            </div>
            
            <div className="p-4 rounded-lg bg-card border border-border/50">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-xs text-muted-foreground font-medium">This Week</p>
                  <p className="text-2xl font-bold mt-1">{stats.weekCount}</p>
                </div>
                <BarChart3 className="h-8 w-8 text-muted-foreground/20" />
              </div>
            </div>
            
            <div className="p-4 rounded-lg bg-card border border-border/50">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-xs text-muted-foreground font-medium">Total Words</p>
                  <p className="text-2xl font-bold mt-1">{stats.totalWords.toLocaleString()}</p>
                </div>
                <FileText className="h-8 w-8 text-muted-foreground/20" />
              </div>
            </div>
            
            <div className="p-4 rounded-lg bg-card border border-border/50">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-xs text-muted-foreground font-medium">Avg Length</p>
                  <p className="text-2xl font-bold mt-1">{stats.avgLength} WPM</p>
                </div>
                <Sparkles className="h-8 w-8 text-muted-foreground/20" />
              </div>
            </div>
          </div>

          {/* Quick Actions */}
          <div className="mb-6 p-4 rounded-lg bg-accent/50 border border-border">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <div className="p-2 rounded-lg bg-primary/10">
                  <Mic className="h-5 w-5 text-primary" />
                </div>
                <div>
                  <p className="text-sm font-medium">Voice dictation in any app</p>
                  <p className="text-xs text-muted-foreground">
                    Hold down the trigger key {formatHotkey(hotkey)} and speak into any textbox
                  </p>
                </div>
              </div>
              <button
                onClick={() => invoke('start_recording').catch(console.error)}
                disabled={!canRecord}
                className={cn(
                  "px-4 py-2 rounded-lg text-sm font-medium transition-all",
                  canRecord 
                    ? "bg-primary text-primary-foreground hover:bg-primary/90"
                    : "bg-muted text-muted-foreground cursor-not-allowed"
                )}
              >
                Start Recording
              </button>
            </div>
          </div>

          {/* Recent Activity */}
          <div className="flex items-center justify-between mb-4">
            <div>
              <h2 className="text-lg font-semibold">Recent activity</h2>
              <p className="text-xs text-muted-foreground">{history.length} transcriptions</p>
            </div>
            {history.length > 0 && (
              <button
                onClick={handleClearAll}
                className="flex items-center gap-2 px-3 py-1.5 text-sm text-destructive hover:bg-destructive/10 rounded-md transition-colors"
                title="Clear all transcriptions"
              >
                <Trash2 className="h-3.5 w-3.5" />
                Clear All
              </button>
            )}
          </div>

          {/* Transcription List */}
          <div className="flex-1 min-h-0">
            {history.length > 0 ? (
              <ScrollArea className="h-[calc(100vh-520px)]">
                <div className="flex flex-col gap-2">
                  {history.slice(0, 10).map((item) => (
                    <div
                      key={item.id}
                      className="group relative p-4 rounded-xl bg-card hover:bg-accent/30 border border-border/50 hover:border-accent transition-all duration-200 cursor-pointer"
                      onClick={() => handleCopy(item.text)}
                      onMouseEnter={() => setHoveredId(item.id)}
                      onMouseLeave={() => setHoveredId(null)}
                    >
                      <div className="flex items-start justify-between">
                        <div className="flex-1 pr-4">
                          <p className="text-sm text-card-foreground line-clamp-2">{item.text}</p>
                          <div className="flex items-center gap-3 mt-2">
                            <span className="text-xs text-muted-foreground">
                              {new Date(item.timestamp).toLocaleTimeString('en-US', { 
                                hour: 'numeric', 
                                minute: '2-digit',
                                hour12: true 
                              })}
                            </span>
                            {item.model && (
                              <span className="text-xs px-2 py-0.5 rounded-full bg-accent text-accent-foreground">
                                {item.model}
                              </span>
                            )}
                            <span className="text-xs text-muted-foreground">
                              {item.text.split(' ').length} words
                            </span>
                          </div>
                        </div>
                        {hoveredId === item.id && (
                          <div className="flex items-center gap-1">
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                handleCopy(item.text);
                              }}
                              className="p-1.5 rounded-lg hover:bg-accent transition-colors"
                              title="Copy"
                            >
                              <Copy className="w-4 h-4" />
                            </button>
                            <button
                              onClick={(e) => handleDelete(e, item.id)}
                              className="p-1.5 rounded-lg hover:bg-destructive/10 transition-colors"
                              title="Delete"
                            >
                              <Trash2 className="w-4 h-4 text-destructive" />
                            </button>
                          </div>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              </ScrollArea>
            ) : (
              <div className="flex-1 flex items-center justify-center h-[calc(100vh-520px)]">
                <div className="text-center">
                  {canRecord ? (
                    <>
                      <div className="relative">
                        <div className="absolute inset-0 animate-ping">
                          <Mic className="w-16 h-16 text-primary/20 mx-auto" />
                        </div>
                        <Mic className="w-16 h-16 text-primary/40 mx-auto mb-4 relative" />
                      </div>
                      <p className="text-base font-medium text-foreground">No recordings yet</p>
                      {canAutoInsert ? (
                        <p className="text-sm text-muted-foreground mt-2">
                          Press {formatHotkey(hotkey)} to start recording
                        </p>
                      ) : (
                        <p className="text-sm text-amber-600 mt-2">
                          Recording available but accessibility permission needed for hotkeys
                        </p>
                      )}
                    </>
                  ) : (
                    <>
                      <AlertCircle className="w-16 h-16 text-amber-500/50 mx-auto mb-4" />
                      <p className="text-base font-medium text-foreground">Setup Required</p>
                      <p className="text-sm text-amber-600 mt-2">
                        Check permissions and download a model in Settings
                      </p>
                    </>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}