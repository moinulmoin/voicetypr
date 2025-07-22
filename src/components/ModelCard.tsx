import { Brain, Download, HardDrive, Loader2, Trash2, X, Zap } from 'lucide-react';
import { ModelInfo } from '../types';
import { Button } from './ui/button';
import { Card, CardContent } from './ui/card';
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

  const getModelDescription = () => {
    const sizeInMB = model.size / (1024 * 1024);
    const sizeStr = sizeInMB >= 1024
      ? `${(sizeInMB / 1024).toFixed(1)} GB`
      : `${Math.round(sizeInMB)} MB`;

    return sizeStr;
  };

  return (
    <Card
      className={`transition-all hover:shadow-md py-2 cursor-pointer ${
        isSelected ? 'border-primary shadow-sm bg-primary/5' : 'hover:border-muted-foreground/50'
      }`}
      onClick={() => model.downloaded && showSelectButton && onSelect(name)}
    >
      <CardContent className="px-4">
        <div className="flex items-center justify-between gap-4">
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <h3 className={`font-medium text-base ${
                isSelected ? 'text-primary' : ''
              }`}>
                {formatModelName(name)}
              </h3>
            </div>
            <div className="flex items-center gap-3 mt-1">
              <div className="flex items-center gap-1">
                <Zap className={`w-3.5 h-3.5 ${isSelected ? 'text-primary' : 'text-muted-foreground'}`} />
                <span className={`text-sm ${isSelected ? 'text-primary' : 'text-muted-foreground'}`}>{model.speed_score}</span>
              </div>
              <div className="flex items-center gap-1">
                <Brain className={`w-3.5 h-3.5 ${isSelected ? 'text-primary' : 'text-muted-foreground'}`} />
                <span className={`text-sm ${isSelected ? 'text-primary' : 'text-muted-foreground'}`}>{model.accuracy_score}</span>
              </div>
              <div className="flex items-center gap-1">
                <HardDrive className={`w-3.5 h-3.5 ${isSelected ? 'text-primary' : 'text-muted-foreground'}`} />
                <span className={`text-sm ${isSelected ? 'text-primary' : 'text-muted-foreground'}`}>{getModelDescription()}</span>
              </div>
            </div>
          </div>

          <div className="flex-shrink-0 flex items-center gap-1">
            {model.downloaded ? (
              <>
                {onDelete && (
                  <Button
                    onClick={(e) => {
                      e.stopPropagation();
                      console.log("Delete button clicked for:", name);
                      onDelete(name);
                    }}
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8 hover:text-destructive"
                  >
                    <Trash2 className="w-4 h-4" />
                  </Button>
                )}
              </>
            ) : isVerifying ? (
              <>
                <Loader2 className="w-4 h-4 animate-spin text-muted-foreground" />
              </>
            ) : downloadProgress !== undefined ? (
              <>
                <Progress value={downloadProgress} className="w-20 h-2" />
                <span className="text-sm font-medium w-12 text-right">{Math.round(downloadProgress)}%</span>
                {onCancelDownload && (
                  <Button
                    onClick={(e) => {
                      e.stopPropagation();
                      onCancelDownload(name);
                    }}
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8 hover:text-destructive"
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
                variant="ghost"
                size="icon"
                className="h-8 w-8"
              >
                <Download className="w-5 h-5" />
              </Button>
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  );
};