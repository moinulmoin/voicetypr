import { Mic } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { TranscriptionHistory } from "@/types";

interface RecentRecordingsProps {
  history: TranscriptionHistory[];
  hotkey?: string;
}

export function RecentRecordings({ history, hotkey = "Cmd+Shift+Space" }: RecentRecordingsProps) {
  return (
    <div className="flex-1 flex flex-col p-6">
      <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4">Recent Recordings</h2>
      {history.length > 0 ? (
        <ScrollArea className="flex-1">
          <div className="space-y-3 pr-4">
            {history.map((item) => (
              <div
                key={item.id}
                className="p-4 rounded-lg bg-gray-50 dark:bg-gray-800/50 border border-gray-200 dark:border-gray-800 cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
                onClick={() => navigator.clipboard.writeText(item.text)}
                title="Click to copy"
              >
                <p className="text-sm text-gray-900 dark:text-gray-100 leading-relaxed">{item.text}</p>
                <p className="text-xs text-gray-500 dark:text-gray-400 mt-2">
                  {new Date(item.timestamp).toLocaleTimeString()} • {item.model}
                </p>
              </div>
            ))}
          </div>
        </ScrollArea>
      ) : (
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center">
            <Mic className="w-12 h-12 text-gray-300 dark:text-gray-700 mx-auto mb-4" />
            <p className="text-sm text-gray-600 dark:text-gray-400">No recordings yet</p>
            <p className="text-xs text-gray-500 dark:text-gray-500 mt-2">
              Press {hotkey} to start recording
            </p>
          </div>
        </div>
      )}
    </div>
  );
}