import { CheckCircle2, Download, Trash2, X } from 'lucide-react';
import { ModelInfo } from '../types';
import { Badge } from './ui/badge';
import { Button } from './ui/button';
import { Card, CardContent } from './ui/card';
import { Progress } from './ui/progress';

interface ModelCardProps {
  name: string;
  model: ModelInfo;
  downloadProgress?: number;
  isSelected?: boolean;
  onDownload: (name: string) => void;
  onSelect: (name: string) => void;
  onDelete?: (name: string) => void;
  onCancelDownload?: (name: string) => void;
  showSelectButton?: boolean;
}

export function ModelCard({
  name,
  model,
  downloadProgress,
  isSelected = false,
  onDownload,
  onSelect,
  onDelete,
  onCancelDownload,
  showSelectButton = true
}: ModelCardProps) {
  const formatModelName = (name: string) => {
    const nameMap: Record<string, string> = {
      'tiny': 'Tiny',
      'base': 'Base',
      'small': 'Small',
      'medium': 'Medium',
      'large-v3': 'Large v3',
      'large-v3-q5_0': 'Large v3 Q5',
      'large-v3-turbo': 'Large v3 Turbo',
      'large-v3-turbo-q5_0': 'Large v3 Turbo Q5'
    };
    return nameMap[name] || name;
  };

  const getModelDescription = () => {
    const sizeInMB = model.size / (1024 * 1024);
    const sizeStr = sizeInMB >= 1024 
      ? `${(sizeInMB / 1024).toFixed(1)} GB`
      : `${Math.round(sizeInMB)} MB`;
    
    return `Speed: ${model.speed_score}/10 • Accuracy: ${model.accuracy_score}/10 • ${sizeStr}`;
  };

  return (
    <Card className={`transition-all hover:shadow-md py-2 ${
      isSelected ? 'border-primary shadow-sm bg-primary/5' : 'hover:border-muted-foreground/50'
    }`}>
      <CardContent className="px-4">
        <div className="flex items-center justify-between gap-4">
          <div className="flex-1 min-w-0">
            <h3 className="font-medium text-base">
              {formatModelName(name)}
            </h3>
            <p className="text-sm text-muted-foreground mt-1">
              {getModelDescription()}
            </p>
          </div>

          <div className="flex-shrink-0 flex items-center gap-1">
            {model.downloaded ? (
              <>
                {showSelectButton ? (
                  isSelected ? (
                    <CheckCircle2 className="w-5 h-5 text-primary" />
                  ) : (
                    <Button
                      onClick={() => onSelect(name)}
                      variant="ghost"
                      size="icon"
                      className="h-8 w-8"
                    >
                      <CheckCircle2 className="w-5 h-5 text-muted-foreground hover:text-primary" />
                    </Button>
                  )
                ) : (
                  <CheckCircle2 className="w-5 h-5 text-green-600" />
                )}
                {onDelete && (
                  <Button
                    onClick={() => {
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
            ) : downloadProgress !== undefined ? (
              <>
                <Progress value={downloadProgress} className="w-20 h-2" />
                <span className="text-sm font-medium w-12 text-right">{downloadProgress.toFixed(0)}%</span>
                {onCancelDownload && (
                  <Button
                    onClick={() => onCancelDownload(name)}
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
                onClick={() => onDownload(name)}
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
}