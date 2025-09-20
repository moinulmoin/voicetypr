import { ask } from '@tauri-apps/plugin-dialog';
import { Key, Trash2 } from 'lucide-react';
import { Button } from './ui/button';
import { Card } from './ui/card';

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
  const providers: Record<string, { name: string; color: string }> = {
    groq: { name: 'Groq', color: 'text-orange-600' },
    gemini: { name: 'Gemini', color: 'text-blue-600' }
  };

  const provider = providers[model.provider] || { name: model.provider, color: 'text-gray-600' };

  return (
    <Card
      className={`py-2 px-4 transition-all ${
        hasApiKey ? 'cursor-pointer hover:border-border' : 'opacity-60'
      } ${
        isSelected ? 'bg-primary/5' : ''
      }`}
      onClick={() => hasApiKey && onSelect()}
    >
      <div className="flex items-center justify-between gap-3">
        <div className="flex-1 space-y-1">
          <div className="flex items-center gap-2">
            <h3 className="font-medium">{model.name}</h3>
            <span className={`text-xs font-medium ${provider.color}`}>
              {provider.name}
            </span>
          </div>
        </div>

        {hasApiKey ? (
          <Button
            onClick={async (e) => {
              e.stopPropagation();
              const confirmed = await ask(
                `Remove API key for ${provider.name}?`,
                { title: 'Remove API Key', kind: 'warning' }
              );
              if (confirmed) {
                onRemoveApiKey();
              }
            }}
            variant="ghost"
            size="sm"
            className="text-muted-foreground hover:text-destructive"
          >
            <Trash2 className="w-3.5 h-3.5" />
            Remove Key
          </Button>
        ) : (
          <Button
            onClick={(e) => {
              e.stopPropagation();
              onSetupApiKey();
            }}
            variant="outline"
            size="sm"
          >
            <Key className="w-4 h-4 mr-1" />
            Add Key
          </Button>
        )}
      </div>
    </Card>
  );
}