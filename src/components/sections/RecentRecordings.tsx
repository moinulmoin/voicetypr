import { ScrollArea } from "@/components/ui/scroll-area";
import { formatHotkey } from "@/lib/hotkey-utils";
import { TranscriptionHistory } from "@/types";
import { useCanRecord, useCanAutoInsert } from "@/contexts/ReadinessContext";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { AlertCircle, Mic, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

interface RecentRecordingsProps {
  history: TranscriptionHistory[];
  hotkey?: string;
  onHistoryUpdate?: () => void;
}

export function RecentRecordings({ history, hotkey = "Cmd+Shift+Space", onHistoryUpdate }: RecentRecordingsProps) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const canRecord = useCanRecord();
  const canAutoInsert = useCanAutoInsert();

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

  return (
    <div className="h-full flex flex-col p-6">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg font-semibold">Recent Transcriptions</h2>
        {history.length > 0 && (
          <button
            onClick={handleClearAll}
            className="flex items-center gap-2 px-3 py-1.5 text-sm text-destructive hover:bg-destructive/10 rounded-md transition-colors"
            title="Clear all transcriptions"
          >

            Clear All
          </button>
        )}
      </div>
      <div className="flex-1 min-h-0">
      {history.length > 0 ? (
        <ScrollArea className="h-full">
          <div className="flex flex-col gap-2.5">
            {history.map((item) => (
              <div
                key={item.id}
                className="group flex items-center relative p-2 rounded-lg cursor-pointer bg-card hover:bg-accent/50 border border-border hover:border-accent transition-all duration-200"
                onClick={() => handleCopy(item.text)}
                onMouseEnter={() => setHoveredId(item.id)}
                onMouseLeave={() => setHoveredId(null)}
                title="Click to copy"
              >
                <p className="text-sm text-card-foreground transcription-text pr-4">{item.text}</p>
                {hoveredId === item.id && (
                  <button
                    onClick={(e) => handleDelete(e, item.id)}
                    className="absolute top-0 right-0 m-1 p-1 rounded hover:bg-destructive/10 transition-colors"
                    title="Delete"
                  >
                    <Trash2 className="w-4 h-4 text-destructive" />
                  </button>
                )}
              </div>
            ))}
          </div>
        </ScrollArea>
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