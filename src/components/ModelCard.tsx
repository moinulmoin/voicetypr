import { CheckCircle, Download, HardDrive, Loader2, Star, X, Zap } from 'lucide-react';
import { ModelInfo } from '../types';
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
      className={`px-4 py-2 transition-all ${
        isUsable ? 'cursor-pointer' : ''
      } ${
        isSelected ? 'border-primary bg-primary/5' : ''
      }`}
      onClick={() => isUsable && showSelectButton && onSelect(name)}
    >
      <div className="flex items-center justify-between gap-3">
        {/* Model Name */}
        <div className="flex items-center gap-1.5 flex-shrink-0">
          <h3 className="font-medium">{model.display_name || name}</h3>
          {model.recommended && (
            <Star className="w-4 h-4 fill-yellow-500 text-yellow-500" aria-label="Recommended model" />
          )}
        </div>

        {/* Centered Stats */}
        <div className="flex items-center justify-center gap-6 flex-1">
          <div className="flex items-center gap-1.5">
            <Zap className="w-4 h-4 text-muted-foreground" />
            <span className="text-sm font-medium">{model.speed_score}</span>
          </div>
          <div className="flex items-center gap-1.5">
            <CheckCircle className="w-4 h-4 text-muted-foreground" />
            <span className="text-sm font-medium">{model.accuracy_score}</span>
          </div>
          <div className="flex items-center gap-1.5">
            <HardDrive className="w-4 h-4 text-muted-foreground" />
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
                  size="icon"
                  className="h-8 w-8 text-muted-foreground hover:text-destructive"
                >
                  <X className="w-4 h-4" />
                </Button>
              )}
            </>
          ) : isVerifying ? (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Loader2 className="w-4 h-4 animate-spin" />
              Verifying...
            </div>
          ) : downloadProgress !== undefined ? (
            <>
              <Progress value={downloadProgress} className="w-24 h-2" />
              <span className="text-sm font-medium w-10 text-right">{Math.round(downloadProgress)}%</span>
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