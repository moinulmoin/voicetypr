import { Mic, Trash2 } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { TranscriptionHistory } from "@/types";
import { useState } from "react";
import { toast } from "sonner";

interface RecentRecordingsProps {
  history: TranscriptionHistory[];
  hotkey?: string;
}

export function RecentRecordings({ history, hotkey = "Cmd+Shift+Space" }: RecentRecordingsProps) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  
  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    toast.success("Copied to clipboard");
  };
  
  const handleDelete = (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    // TODO: Implement delete functionality
    toast.error("Delete functionality not yet implemented");
  };
  return (
    <div className="flex-1 flex flex-col p-6">
      <h2 className="text-lg font-semibold mb-4">Recent Transcriptions</h2>
      {history.length > 0 ? (
        <ScrollArea className="flex-1">
          <div className="space-y-3 pr-4">
            {history.map((item) => (
              <div
                key={item.id}
                className="group relative p-3 rounded-lg cursor-pointer bg-card hover:bg-accent/50 border border-border hover:border-accent transition-all duration-200"
                onClick={() => handleCopy(item.text)}
                onMouseEnter={() => setHoveredId(item.id)}
                onMouseLeave={() => setHoveredId(null)}
                title="Click to copy"
              >
                <p className="text-sm leading-relaxed pr-8 text-card-foreground">{item.text}</p>
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
        </ScrollArea>
      ) : (
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center">
            <Mic className="w-12 h-12 text-muted-foreground/30 mx-auto mb-4" />
            <p className="text-sm text-muted-foreground">No recordings yet</p>
            <p className="text-xs text-muted-foreground/70 mt-2">
              Press {hotkey} to start recording
            </p>
          </div>
        </div>
      )}
    </div>
  );
}