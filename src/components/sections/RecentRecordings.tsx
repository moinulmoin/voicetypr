import { formatHotkey } from "@/lib/hotkey-utils";
import { TranscriptionHistory } from "@/types";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { Mic, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

interface RecentRecordingsProps {
  history: TranscriptionHistory[];
  hotkey?: string;
  onHistoryUpdate?: () => void;
}

export function RecentRecordings({ history, hotkey = "Cmd+Shift+Space", onHistoryUpdate }: RecentRecordingsProps) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);

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

  return (
    <div className="flex-1 w-full flex flex-col p-6">
      <h2 className="text-lg font-semibold mb-4">Recent Transcriptions</h2>
      <div className="">
      {history.length > 0 ? (
        // <ScrollArea className="flex-1">
          <div className="flex flex-col gap-2.5">
            {history.map((item) => (
              <div
                key={item.id}
                className="group relative p-3 rounded-lg cursor-pointer bg-card hover:bg-accent/50 border border-border hover:border-accent transition-all duration-200"
                onClick={() => handleCopy(item.text)}
                onMouseEnter={() => setHoveredId(item.id)}
                onMouseLeave={() => setHoveredId(null)}
                title="Click to copy"
              >
                <p className="text-sm pr-8 text-card-foreground break-words ">{item.text}</p>
                {hoveredId === item.id && (
                  <button
                    onClick={(e) => handleDelete(e, item.id)}
                    className="absolute top-3 right-3 p-1 rounded hover:bg-destructive/10 transition-colors"
                    title="Delete"
                  >
                    <Trash2 className="w-4 h-4 text-destructive" />
                  </button>
                )}
              </div>
            ))}
          </div>
        // </ScrollArea>
      ) : (
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center">
            <Mic className="w-12 h-12 text-muted-foreground/30 mx-auto mb-4" />
            <p className="text-sm text-muted-foreground">No recordings yet</p>
            <p className="text-xs text-muted-foreground/70 mt-2">
              Press {formatHotkey(hotkey)} to start recording
            </p>
          </div>
        </div>
      )}
      </div>
    </div>
  );
}