import { CheckCircle2, Download } from 'lucide-react';
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

  const getModelDescription = (name: string) => {
    const descriptions: Record<string, string> = {
      'tiny': 'Fastest • 75 MB',
      'base': 'Balanced • 142 MB',
      'small': 'Accurate • 466 MB',
      'medium': 'Very accurate • 1.5 GB',
      'large-v3': 'Best quality • 2.9 GB',
      'large-v3-q5_0': 'Large compressed • 1.1 GB',
      'large-v3-turbo': 'Fast & accurate • 1.5 GB',
      'large-v3-turbo-q5_0': 'Fast compressed • 547 MB'
    };
    return descriptions[name] || '';
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
            <p className="text-sm text-muted-foreground mt-0.5">
              {getModelDescription(name)}
            </p>
          </div>

          <div className="flex-shrink-0">
            {model.downloaded ? (
              showSelectButton ? (
                isSelected ? (
                  <div className="flex items-center gap-2 text-sm text-muted-foreground">
                    <CheckCircle2 className="w-4 h-4 text-primary" />
                    <span>Active</span>
                  </div>
                ) : (
                  <Button
                    onClick={() => onSelect(name)}
                    variant="ghost"
                    size="sm"
                    className="text-primary hover:text-primary"
                  >
                    Activate
                  </Button>
                )
              ) : (
                <Badge variant="secondary" className="font-normal">
                  <CheckCircle2 className="w-3 h-3 mr-1" />
                  Ready
                </Badge>
              )
            ) : downloadProgress !== undefined ? (
              <div className="flex items-center gap-3">
                <Progress value={downloadProgress} className="w-20 h-2" />
                <span className="text-sm font-medium w-12 text-right">{downloadProgress.toFixed(0)}%</span>
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
        </div>
      </CardContent>
    </Card>
  );
}