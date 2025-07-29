import { CheckCircle, Download, HardDrive, Loader2, X, Zap } from 'lucide-react';
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
  const formatModelName = (name: string) => {
    const nameMap: Record<string, string> = {
      'base.en': 'Base (English)',
      'small.en': 'Small (English)',
      'large-v3': 'Large v3',
      'large-v3-q5_0': 'Large v3 Q5',
      'large-v3-turbo': 'Large v3 Turbo',
      'large-v3-turbo-q5_0': 'Large v3 Turbo Q5',
      'large-v3-turbo-q8_0': 'Large v3 Turbo Q8'
    };
    return nameMap[name] || name;
  };

  const formatSize = () => {
    const sizeInMB = model.size / (1024 * 1024);
    return sizeInMB >= 1024
      ? `${(sizeInMB / 1024).toFixed(1)} GB`
      : `${Math.round(sizeInMB)} MB`;
  };

  return (
    <Card
      className={`px-4 py-2 transition-all ${
        model.downloaded ? 'cursor-pointer' : ''
      } ${
        isSelected ? 'border-primary bg-primary/5' : ''
      }`}
      onClick={() => model.downloaded && showSelectButton && onSelect(name)}
    >
      <div className="flex items-center justify-between gap-3">
        {/* Model Name */}
        <h3 className="font-medium flex-shrink-0">{formatModelName(name)}</h3>

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