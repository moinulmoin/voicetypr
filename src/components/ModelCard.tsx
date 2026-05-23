import { CheckCircle, Download, HardDrive, Star, Trash2, X, Zap } from 'lucide-react';
import { ModelInfo, isLocalModel } from '../types';
import { Badge } from './ui/badge';
import { Button } from './ui/button';
import { Card } from './ui/card';
import { Progress } from './ui/progress';
import { Spinner } from './ui/spinner';
import { cn } from '@/lib/utils';
import { getModelDisplayName } from '@/lib/model-display';

interface ModelCardProps {
  name: string;
  model: ModelInfo;
  downloadProgress?: number;
  downloadPhase?: string;
  isVerifying?: boolean;
  isSelected?: boolean;
  onDownload: (name: string) => void;
  onSelect: (name: string) => void;
  onDelete?: (name: string) => void;
  onCancelDownload?: (name: string) => void;
  onRepair?: (name: string) => void;
  showSelectButton?: boolean;
}

export const ModelCard = function ModelCard({
  name,
  model,
  downloadProgress,
  downloadPhase,
  isVerifying = false,
  isSelected = false,
  onDownload,
  onSelect,
  onDelete,
  onCancelDownload,
  onRepair,
  showSelectButton = true
}: ModelCardProps) {

  if (!isLocalModel(model)) {
    console.warn(`[ModelCard] Skipping non-local model card for ${model.name}`);
    return null;
  }

  const formatSize = () => {
    const sizeInMB = model.size / (1024 * 1024);
    return sizeInMB >= 1024
      ? `${(sizeInMB / 1024).toFixed(1)} GB`
      : `${Math.round(sizeInMB)} MB`;
  };

  // Model is usable if downloaded
  const isUsable = model.downloaded;
  const downloadLabel = downloadPhase
    ? downloadPhase.charAt(0).toUpperCase() + downloadPhase.slice(1)
    : "Downloading";

  return (
    <Card
      className={cn(
        "group px-4 py-3 border transition-all hover:shadow-sm",
        isUsable && showSelectButton ? "cursor-pointer" : "",
        isSelected
          ? "border-primary/45 bg-primary/5 shadow-sm ring-2 ring-primary/15"
          : "border-border/60 bg-card/90 hover:border-border"
      )}
      onClick={() => isUsable && showSelectButton && onSelect(name)}
    >
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h3
              className={cn(
                "truncate text-sm font-semibold tracking-tight",
                isSelected && "text-primary"
              )}
            >
              {getModelDisplayName(name, { [name]: model })}
            </h3>
            {model.recommended && (
              <Badge variant="secondary" className="gap-1 bg-amber-500/10 text-amber-700">
                <Star className="size-3 fill-current" aria-label="Recommended model" />
                Recommended
              </Badge>
            )}
            {isSelected && (
              <Badge className="gap-1">
                <CheckCircle className="size-3" />
                Active
              </Badge>
            )}
          </div>

          <div className="mt-2 flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
            <span className="inline-flex items-center gap-1.5">
              <Zap className="size-3.5 text-emerald-600" />
              Speed <span className="font-medium text-foreground">{model.speed_score}</span>
            </span>
            <span className="inline-flex items-center gap-1.5">
              <CheckCircle className="size-3.5 text-blue-600" />
              Accuracy <span className="font-medium text-foreground">{model.accuracy_score}</span>
            </span>
            <span className="inline-flex items-center gap-1.5">
              <HardDrive className="size-3.5 text-muted-foreground" />
              <span className="font-mono text-foreground">{formatSize()}</span>
            </span>
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          {model.downloaded ? (
            <>
              {onRepair && (
                <Button
                  onClick={(e) => {
                    e.stopPropagation();
                    onRepair(name);
                  }}
                  variant="ghost"
                  size="sm"
                  className="text-muted-foreground"
                >
                  Repair
                </Button>
              )}
              {onDelete && (
                <Button
                  onClick={(e) => {
                    e.stopPropagation();
                    onDelete(name);
                  }}
                  variant="ghost"
                  size="sm"
                  className="text-muted-foreground hover:text-destructive"
                >
                  <Trash2 className="size-3.5" />
                  Remove
                </Button>
              )}
            </>
          ) : isVerifying ? (
            <Badge variant="outline" className="gap-1.5 bg-amber-500/10 text-amber-700">
              <Spinner className="size-3.5" />
              Verifying
            </Badge>
          ) : downloadProgress !== undefined ? (
            <>
              {model.engine === 'parakeet' && downloadProgress === 0 ? (
                <Badge variant="outline" className="gap-1.5 bg-primary/10 text-primary">
                  <Spinner className="size-3.5" />
                  {downloadLabel}
                </Badge>
              ) : (
                <div className="flex items-center gap-2">
                  <div className="min-w-0">
                    <Progress value={downloadProgress} className="h-1.5 w-24" />
                    {downloadPhase && (
                      <p className="mt-1 max-w-24 truncate text-[10px] text-muted-foreground">
                        {downloadLabel}
                      </p>
                    )}
                  </div>
                  <span className="w-10 text-right text-xs font-medium text-primary">
                    {Math.round(downloadProgress)}%
                  </span>
                </div>
              )}
              {onCancelDownload && (
                <Button
                  onClick={(e) => {
                    e.stopPropagation();
                    onCancelDownload(name);
                  }}
                  variant="ghost"
                  size="icon"
                  className="size-8"
                  aria-label={`Cancel ${getModelDisplayName(name, { [name]: model })} download`}
                >
                  <X className="size-4" />
                </Button>
              )}
            </>
          ) : (
            <Button
              onClick={(e) => {
                e.stopPropagation();
                onDownload(name);
              }}
              variant="default"
              size="sm"
            >
              <Download className="size-4" />
              Download
            </Button>
          )}
        </div>
      </div>
    </Card>
  );
};
