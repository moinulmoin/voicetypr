import { ModelCard } from "@/components/ModelCard";
import { ScrollArea } from "@/components/ui/scroll-area";
import { ModelInfo } from "@/types";

interface ModelsSectionProps {
  models: [string, ModelInfo][];
  downloadProgress: Record<string, number>;
  currentModel?: string;
  onDownload: (modelName: string) => void;
  onDelete: (modelName: string) => void;
  onCancelDownload: (modelName: string) => void;
  onSelect: (modelName: string) => void;
}

export function ModelsSection({ 
  models, 
  downloadProgress, 
  currentModel,
  onDownload, 
  onDelete, 
  onCancelDownload,
  onSelect 
}: ModelsSectionProps) {
  return (
    <div className="p-6">
      <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4">Models</h2>
      <p className="text-sm text-gray-600 dark:text-gray-400 mb-6">
        Download and manage Whisper models for transcription
      </p>
      
      <ScrollArea className="h-[calc(100vh-200px)]">
        <div className="space-y-3 pr-4">
          {models.map(([name, model]) => (
            <ModelCard
              key={name}
              name={name}
              model={model}
              downloadProgress={downloadProgress[name]}
              onDownload={onDownload}
              onDelete={onDelete}
              onCancelDownload={onCancelDownload}
              onSelect={async (modelName) => {
                if (model.downloaded) {
                  onSelect(modelName);
                }
              }}
              showSelectButton={model.downloaded}
              isSelected={currentModel === name}
            />
          ))}
        </div>
      </ScrollArea>
    </div>
  );
}