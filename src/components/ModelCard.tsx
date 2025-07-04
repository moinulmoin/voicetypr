import { Check, CheckCircle2, Download, Loader2 } from 'lucide-react';
import { ModelInfo } from '../types';
import { Button } from './ui/button';
import { Card, CardContent } from './ui/card';
import { Badge } from './ui/badge';
import { Progress } from './ui/progress';

interface ModelCardProps {
  name: string;
  model: ModelInfo;
  downloadProgress?: number;
  isSelected?: boolean;
  onDownload: (name: string) => void;
  onSelect: (name: string) => void;
  showSelectButton?: boolean;
}

export function ModelCard({
  name,
  model,
  downloadProgress,
  isSelected = false,
  onDownload,
  onSelect,
  showSelectButton = true
}: ModelCardProps) {
  const getModelDescription = (name: string) => {
    if (name.includes('.en')) return 'English only';
    if (name === 'tiny') return 'Fastest, least accurate';
    if (name === 'base') return 'Good balance';
    if (name === 'small') return 'Best accuracy';
    return '';
  };

  return (
    <Card className={`hover:border-primary transition-colors ${
      isSelected ? 'border-primary bg-primary/5' : ''
    }`}>
      <CardContent className="p-4">
        <div className="flex items-center justify-between">
          <div className="flex-1">
            <div className="flex items-center gap-2">
              <h3 className="font-semibold capitalize">
                {name} Model
              </h3>
              {isSelected && model.downloaded && (
                <Badge variant="default" className="gap-1">
                  <CheckCircle2 className="w-3 h-3" />
                  Active
                </Badge>
              )}
            </div>
            <p className="text-sm text-muted-foreground mt-1">
              {(model.size / 1024 / 1024).toFixed(0)} MB
              {getModelDescription(name) && ` • ${getModelDescription(name)}`}
            </p>
          </div>

        {model.downloaded ? (
          showSelectButton ? (
            <Button
              onClick={() => onSelect(name)}
              variant="default"
              size="sm"
            >
              <Check className="w-4 h-4" />
              Select
            </Button>
          ) : (
            <Badge variant="secondary">Downloaded</Badge>
          )
        ) : downloadProgress !== undefined ? (
          <div className="flex flex-col items-end gap-1 min-w-[100px]">
            <div className="flex items-center gap-2">
              <Loader2 className="w-4 h-4 animate-spin" />
              <span className="text-sm font-medium">{downloadProgress.toFixed(0)}%</span>
            </div>
            <Progress value={downloadProgress} className="w-full h-2" />
          </div>
        ) : (
          <Button
            onClick={() => onDownload(name)}
            variant="outline"
            size="sm"
          >
            <Download className="w-4 h-4" />
            Download
          </Button>
        )}
        </div>
      </CardContent>
    </Card>
  );
}