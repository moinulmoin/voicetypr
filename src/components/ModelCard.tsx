import { AlertCircle, CheckCircle, Download, HardDrive, Loader2, Star, X, Zap, Trash2 } from 'lucide-react';
import { ModelInfo, isLocalModel } from '../types';
import { Button } from './ui/button';
import { Card } from './ui/card';
import { Progress } from './ui/progress';

interface ModelCardProps {
  name: string;
  model: ModelInfo;
  downloadProgress?: number;
  isVerifying?: boolean;
  downloadError?: string;
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
  downloadError,
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

  // Model is usable only when it is ready for transcription.
  const isUsable = model.downloaded && !model.requires_setup;

  return (
    <Card
      className={`px-4 py-3 transition-all hover:shadow-sm ${
        isUsable ? 'cursor-pointer hover:border-border' : ''
      } ${
        isSelected ? 'bg-primary/5 border-border/50' : 'border-border/50'
      }`}
      onClick={() => isUsable && showSelectButton && onSelect(name)}
    >
      <div className="flex items-center justify-between gap-3">
        {/* Model Name */}
        <div className="flex items-center gap-2 flex-shrink-0 min-w-0">
          <h3 className="font-medium text-sm">{model.display_name || name}</h3>
          {model.recommended && (
            <Star className="w-3.5 h-3.5 fill-yellow-500 text-yellow-500" aria-label="Recommended model" />
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
          {isUsable ? (
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
      {downloadError && !isUsable && downloadProgress === undefined && !isVerifying && (
        <div className="mt-2 flex items-start gap-2 rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive" role="alert">
          <AlertCircle className="mt-0.5 h-3.5 w-3.5 flex-shrink-0" />
          <span className="whitespace-pre-line">{downloadError}</span>
        </div>
      )}
    </Card>
  );
};
