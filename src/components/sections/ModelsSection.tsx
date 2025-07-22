import { ModelCard } from "@/components/ModelCard";
import { ScrollArea } from "@/components/ui/scroll-area";
import { ModelInfo } from "@/types";

interface ModelsSectionProps {
  models: [string, ModelInfo][];
  downloadProgress: Record<string, number>;
  verifyingModels: Set<string>;
  currentModel?: string;
  onDownload: (modelName: string) => void;
  onDelete: (modelName: string) => void;
  onCancelDownload: (modelName: string) => void;
  onSelect: (modelName: string) => void;
}

export function ModelsSection({
  models,
  downloadProgress,
  verifyingModels,
  currentModel,
  onDownload,
  onDelete,
onCancelDownload,
  onSelect
}: ModelsSectionProps) {
  return (
    <div className="h-full flex flex-col p-6">
      <div className="flex-shrink-0">
        <h2 className="text-lg font-semibold mb-4">Models</h2>
        <p className="text-sm text-muted-foreground mb-6">
          Download and manage models for transcription
        </p>
      </div>

      <ScrollArea className="flex-1 min-h-0">
        <div className="space-y-3 pr-4">
          {models.map(([name, model]) => (
            <ModelCard
              key={name}
              name={name}
              model={model}
              downloadProgress={downloadProgress[name]}
              isVerifying={verifyingModels.has(name)}
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