import { CheckCircle, Download, HardDrive, Loader2, Star, X, Zap, Trash2 } from 'lucide-react';
import { ModelInfo, isLocalModel } from '../types';
import { Button } from './ui/button';
import { Card } from './ui/card';
import { Progress } from './ui/progress';

interface ModelCardProps {
  name: string;
  model: ModelInfo;
  downloadProgress?: number;
  isVerifying?: boolean;
  isSelected?: boolean;
  onDownload: (name: string) => void;
  onSelect: (name: string) => void;
  onDelete?: (name: string) => void;
  onCancelDownload?: (name: string) => void;
  showSelectButton?: boolean;
}

export const ModelCard = function ModelCard({
  name,
  model,
  downloadProgress,
  isVerifying = false,
  isSelected = false,
  onDownload,
  onSelect,
  onDelete,
  onCancelDownload,
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

  return (
    <Card
      className={`px-4 py-3 border transition-all hover:shadow-sm ${
        isUsable ? 'cursor-pointer' : ''
      } ${
        isSelected
          ? 'bg-primary/15 border-primary ring-2 ring-primary/30'
          : 'border-border/50 hover:border-border'
      }`}
      onClick={() => isUsable && showSelectButton && onSelect(name)}
    >
      <div className="flex items-center justify-between gap-3">
        {/* Model Name */}
        <div className="flex items-center gap-2 flex-shrink-0 min-w-0">
          <h3 className={`font-medium text-sm ${isSelected ? 'text-primary' : ''}`}>
            {model.display_name || name}
          </h3>
          {model.recommended && (
            <Star className="w-3.5 h-3.5 fill-yellow-500 text-yellow-500" aria-label="Recommended model" />
          )}
          {isSelected && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-primary/20 text-primary text-xs font-medium flex-shrink-0">
              <CheckCircle className="h-3 w-3" />
              Active
            </span>
          )}
        </div>

        {/* Centered Stats */}
        <div className="flex items-center justify-center gap-6 flex-1">
          <div className="flex items-center gap-1.5">
            <Zap className="w-3.5 h-3.5 text-green-500/70" />
            <span className="text-sm font-medium">{model.speed_score}</span>
          </div>
          <div className="flex items-center gap-1.5">
            <CheckCircle className="w-3.5 h-3.5 text-blue-500/70" />
            <span className="text-sm font-medium">{model.accuracy_score}</span>
          </div>
          <div className="flex items-center gap-1.5">
            <HardDrive className="w-3.5 h-3.5 text-purple-500/70" />
            <span className="text-sm font-medium">{formatSize()}</span>
          </div>
        </div>

        {/* Action Buttons */}
        <div className="flex items-center gap-2 flex-shrink-0">
          {model.downloaded ? (
            // Model is downloaded - show delete option
            <>
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
                  <Trash2 className="w-3.5 h-3.5 mr-1" />
                  Remove
                </Button>
              )}
            </>
          ) : isVerifying ? (
            <div className="flex items-center gap-2 px-2 py-1 rounded bg-yellow-500/10">
              <Loader2 className="w-3.5 h-3.5 animate-spin text-yellow-600" />
              <span className="text-xs font-medium text-yellow-600">Verifying</span>
            </div>
          ) : downloadProgress !== undefined ? (
            <>
              {/* For Parakeet models, show indeterminate progress (FluidAudio doesn't report progress) */}
              {model.engine === 'parakeet' && downloadProgress === 0 ? (
                <div className="flex items-center gap-2 px-2 py-1 rounded bg-blue-500/10">
                  <Loader2 className="w-3.5 h-3.5 animate-spin text-blue-600" />
                  <span className="text-xs font-medium text-blue-600">Downloading...</span>
                </div>
              ) : (
                <>
                  <Progress value={downloadProgress} className="w-20 h-1.5" />
                  <span className="text-xs font-medium text-blue-600 w-10 text-right">{Math.round(downloadProgress)}%</span>
                </>
              )}
              {onCancelDownload && (
                <Button
                  onClick={(e) => {
                    e.stopPropagation();
                    onCancelDownload(name);
                  }}
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8"
                >
                  <X className="w-4 h-4" />
                </Button>
              )}
            </>
          ) : (
            <Button
              onClick={(e) => {
                e.stopPropagation();
                onDownload(name);
              }}
              variant="outline"
              size="sm"
            >
              <Download className="w-4 h-4 mr-1" />
              Download
            </Button>
          )}
        </div>
      </div>
    </Card>
  );
};
