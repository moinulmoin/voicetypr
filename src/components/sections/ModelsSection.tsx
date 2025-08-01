import { ModelCard } from "@/components/ModelCard";
import { ScrollArea } from "@/components/ui/scroll-area";
import { ModelInfo } from "@/types";
import { CheckCircle, HardDrive, Star, Zap } from "lucide-react";

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
      <div className="flex-shrink-0 mb-4 space-y-3">
        <h2 className="text-lg font-semibold">Models</h2>
        <p className="text-sm text-muted-foreground">
          Choose a model to transcribe your voice into text
        </p>

        {/* Icon Legend */}
        <div className="flex gap-4 text-xs text-muted-foreground">
          <span className="flex items-center gap-1">
            <Zap className="w-4 h-4" />
            Speed
          </span>
          <span className="flex items-center gap-1">
            <CheckCircle className="w-4 h-4" />
            Accuracy
          </span>
          <span className="flex items-center gap-1">
            <HardDrive className="w-4 h-4" />
            Size
          </span>
          <span className="flex items-center gap-1">
            <Star className="w-4 h-4 fill-yellow-500 text-yellow-500" />
            Recommended
          </span>
        </div>
      </div>

      <ScrollArea className="flex-1 min-h-0">
        <div className="space-y-3">
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