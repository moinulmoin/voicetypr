import { Brain, Key, Trash2 } from 'lucide-react';
import { Button } from './ui/button';
import { Card, CardContent } from './ui/card';

interface AIModel {
  id: string;
  name: string;
  provider: string;
  description?: string;
}

interface EnhancementModelCardProps {
  model: AIModel;
  hasApiKey: boolean;
  isSelected: boolean;
  onSetupApiKey: () => void;
  onSelect: () => void;
  onRemoveApiKey: () => void;
}

export function EnhancementModelCard({
  model,
  hasApiKey,
  isSelected,
  onSetupApiKey,
  onSelect,
  onRemoveApiKey,
}: EnhancementModelCardProps) {
  const getProviderBadge = () => {
    const badges: Record<string, { name: string; className: string }> = {
      groq: { name: 'Groq', className: 'bg-orange-100 text-orange-700' }
    };
    return badges[model.provider] || { name: model.provider, className: 'bg-gray-100 text-gray-700' };
  };

  const providerBadge = getProviderBadge();

  return (
    <Card
      className={`transition-all hover:shadow-md py-2 cursor-pointer ${
        isSelected ? 'border-primary shadow-sm bg-primary/5' : 'hover:border-muted-foreground/50'
      }`}
      onClick={() => hasApiKey && onSelect()}
    >
      <CardContent className="px-4">
        <div className="flex items-center justify-between gap-4">
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <h3 className={`font-medium text-base ${
                isSelected ? 'text-primary' : ''
              }`}>
                {model.name}
              </h3>
              <span className={`text-xs px-2 py-0.5 rounded-full font-medium ${providerBadge.className}`}>
                {providerBadge.name}
              </span>
            </div>
            {model.description && (
              <div className="flex items-center gap-3 mt-1">
                <div className="flex items-center gap-1">
                  <Brain className={`w-3.5 h-3.5 ${isSelected ? 'text-primary' : 'text-muted-foreground'}`} />
                  <span className={`text-sm ${isSelected ? 'text-primary' : 'text-muted-foreground'}`}>
                    {model.description}
                  </span>
                </div>
              </div>
            )}
          </div>

          <div className="flex-shrink-0 flex items-center gap-2">
            {hasApiKey ? (
              <>
                <Button
                  onClick={(e) => {
                    e.stopPropagation();
                    onRemoveApiKey();
                  }}
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8 hover:bg-destructive/10"
                  title="Remove API key"
                >
                  <Trash2 className="w-4 h-4 text-destructive" />
                </Button>
              </>
            ) : (
              <Button
                onClick={(e) => {
                  e.stopPropagation();
                  onSetupApiKey();
                }}
                variant="ghost"
                size="icon"
                className="h-8 w-8"
                title="Add API key"
              >
                <Key className="w-5 h-5" />
              </Button>
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}