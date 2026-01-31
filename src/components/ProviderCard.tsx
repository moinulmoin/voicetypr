import { ask } from "@tauri-apps/plugin-dialog";
import { Check, ChevronDown, ExternalLink, Key, Trash2 } from "lucide-react";
import { Button } from "./ui/button";
import { Card } from "./ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "./ui/dropdown-menu";
import type { AIProviderConfig } from "@/types/providers";

interface ProviderCardProps {
  provider: AIProviderConfig;
  hasApiKey: boolean;
  isActive: boolean;
  selectedModel: string | null;
  onSetupApiKey: () => void;
  onRemoveApiKey: () => void;
  onSelectModel: (modelId: string) => void;
}

export function ProviderCard({
  provider,
  hasApiKey,
  isActive,
  selectedModel,
  onSetupApiKey,
  onRemoveApiKey,
  onSelectModel,
}: ProviderCardProps) {
  const selectedModelData = provider.models.find((m) => m.id === selectedModel);

  return (
    <Card
      className={`p-4 transition-all ${
        isActive ? "border-primary/50 bg-primary/5" : ""
      }`}
    >
      <div className="flex items-center justify-between gap-4">
        {/* Provider Info */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-1">
            <h3 className={`font-semibold ${provider.color}`}>{provider.name}</h3>
            {isActive && (
              <span className="text-xs bg-primary/10 text-primary px-2 py-0.5 rounded-full">
                Active
              </span>
            )}
          </div>

          {/* Model Selection Dropdown */}
          {hasApiKey && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 px-2 text-sm text-muted-foreground hover:text-foreground"
                >
                  {selectedModelData ? (
                    <>
                      <span className="truncate max-w-[180px]">
                        {selectedModelData.name}
                      </span>
                      <ChevronDown className="w-3.5 h-3.5 ml-1 flex-shrink-0" />
                    </>
                  ) : (
                    <>
                      Select model
                      <ChevronDown className="w-3.5 h-3.5 ml-1" />
                    </>
                  )}
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="w-64">
                {provider.models.map((model) => (
                  <DropdownMenuItem
                    key={model.id}
                    onClick={() => onSelectModel(model.id)}
                    className="flex items-start gap-2 py-2"
                  >
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="font-medium">{model.name}</span>
                        {selectedModel === model.id && (
                          <Check className="w-3.5 h-3.5 text-primary" />
                        )}
                      </div>
                      {model.description && (
                        <p className="text-xs text-muted-foreground mt-0.5">
                          {model.description}
                        </p>
                      )}
                    </div>
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          )}

          {!hasApiKey && (
            <p className="text-sm text-muted-foreground">
              Add API key to enable
            </p>
          )}
        </div>

        {/* Action Buttons */}
        <div className="flex items-center gap-2">
          {hasApiKey ? (
            <Button
              onClick={async () => {
                const confirmed = await ask(
                  `Remove API key for ${provider.name}?`,
                  { title: "Remove API Key", kind: "warning" }
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
            </Button>
          ) : (
            <>
              <Button
                onClick={() => window.open(provider.apiKeyUrl, "_blank")}
                variant="ghost"
                size="sm"
                className="text-muted-foreground"
                title={`Get ${provider.name} API Key`}
              >
                <ExternalLink className="w-3.5 h-3.5" />
              </Button>
              <Button onClick={onSetupApiKey} variant="outline" size="sm">
                <Key className="w-3.5 h-3.5 mr-1.5" />
                Add Key
              </Button>
            </>
          )}
        </div>
      </div>
    </Card>
  );
}
