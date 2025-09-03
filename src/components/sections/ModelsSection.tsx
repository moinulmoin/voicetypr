import { ModelCard } from "@/components/ModelCard";
import { ScrollArea } from "@/components/ui/scroll-area";
import { ModelInfo } from "@/types";
import { CheckCircle, HardDrive, Star, Zap, Bot, Download } from "lucide-react";
import { useMemo } from "react";

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
  // Categorize models
  const { installedModels, availableModels } = useMemo(() => {
    const installed: [string, ModelInfo][] = [];
    const available: [string, ModelInfo][] = [];
    
    models.forEach(([name, model]) => {
      if (model.downloaded) {
        installed.push([name, model]);
      } else {
        available.push([name, model]);
      }
    });
    
    return { installedModels: installed, availableModels: available };
  }, [models]);
  
  const hasDownloading = Object.keys(downloadProgress).length > 0;
  const hasVerifying = verifyingModels.size > 0;
  
  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="px-6 py-4 border-b border-border/40">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold">Models</h1>
            <p className="text-sm text-muted-foreground mt-1">
              {installedModels.length} installed • {availableModels.length} available
            </p>
          </div>
          <div className="flex items-center gap-3">
            {(hasDownloading || hasVerifying) && (
              <div className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-blue-500/10 text-sm font-medium">
                <Download className="h-3.5 w-3.5 text-blue-500" />
                {hasDownloading ? 'Downloading...' : 'Verifying...'}
              </div>
            )}
            {currentModel ? (
              <span className="text-sm text-muted-foreground">
                Active: <span className="text-amber-600 dark:text-amber-500">{installedModels.find(([name]) => name === currentModel)?.[1].display_name || currentModel}</span>
              </span>
            ) : installedModels.length > 0 && (
              <span className="text-sm text-amber-600 dark:text-amber-500">
                No model selected
              </span>
            )}
          </div>
        </div>
      </div>

      {/* Legend */}
      <div className="px-6 py-3 border-b border-border/20">
        <div className="flex items-center gap-6 text-xs text-muted-foreground">
          <span className="flex items-center gap-1.5">
            <Zap className="w-3.5 h-3.5 text-green-500" />
            Speed
          </span>
          <span className="flex items-center gap-1.5">
            <CheckCircle className="w-3.5 h-3.5 text-blue-500" />
            Accuracy
          </span>
          <span className="flex items-center gap-1.5">
            <HardDrive className="w-3.5 h-3.5 text-purple-500" />
            Size
          </span>
          <span className="flex items-center gap-1.5">
            <Star className="w-3.5 h-3.5 fill-yellow-500 text-yellow-500" />
            Recommended
          </span>
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-hidden">
        <ScrollArea className="h-full">
          <div className="p-6 space-y-6">
            {/* Installed Models */}
            {installedModels.length > 0 && (
              <div className="space-y-4">
                <div className="flex items-center gap-2">
                  <h2 className="text-base font-semibold text-foreground">Installed Models</h2>
                  <div className="h-px bg-border/50 flex-1" />
                  <span className="text-xs text-muted-foreground px-2 py-1 bg-muted/50 rounded">
                    {installedModels.length}
                  </span>
                </div>
                <div className="grid gap-3">
                  {installedModels.map(([name, model]) => (
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
              </div>
            )}

            {/* Available Models */}
            {availableModels.length > 0 && (
              <div className="space-y-4">
                <div className="flex items-center gap-2">
                  <h2 className="text-base font-semibold text-foreground">Available to Download</h2>
                  <div className="h-px bg-border/50 flex-1" />
                  <span className="text-xs text-muted-foreground px-2 py-1 bg-muted/50 rounded">
                    {availableModels.length}
                  </span>
                </div>
                <div className="grid gap-3">
                  {availableModels.map(([name, model]) => (
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
              </div>
            )}

            {/* Empty State */}
            {models.length === 0 && (
              <div className="flex-1 flex items-center justify-center py-12">
                <div className="text-center">
                  <Bot className="w-12 h-12 text-muted-foreground/30 mx-auto mb-4" />
                  <p className="text-sm text-muted-foreground">No models available</p>
                  <p className="text-xs text-muted-foreground/70 mt-2">
                    Models will appear here when they become available
                  </p>
                </div>
              </div>
            )}
          </div>
        </ScrollArea>
      </div>
    </div>
  );
}