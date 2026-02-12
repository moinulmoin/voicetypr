import { ask } from "@tauri-apps/plugin-dialog";
import { Check, ChevronDown, ExternalLink, Key, Loader2, RefreshCw, Settings2, Star, Trash2 } from "lucide-react";
import { Button } from "./ui/button";
import { Card } from "./ui/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "./ui/dropdown-menu";
import type { AIProviderConfig, AIProviderModel } from "@/types/providers";

interface ProviderCardProps {
  provider: AIProviderConfig;
  hasApiKey: boolean;
  isActive: boolean;
  selectedModel: string | null;
  onSetupApiKey: () => void;
  onRemoveApiKey: () => void;
  onSelectModel: (modelId: string) => void;
  /** Dynamic models fetched from API */
  models: AIProviderModel[];
  /** Loading state for model fetch */
  modelsLoading: boolean;
  /** Error state for model fetch */
  modelsError: string | null;
  /** Callback to refresh models */
  onRefreshModels: () => void;
  /** For custom providers - the configured model name to display */
  customModelName?: string;
}

export function ProviderCard({
  provider,
  hasApiKey,
  isActive,
  selectedModel,
  onSetupApiKey,
  onRemoveApiKey,
  onSelectModel,
  models,
  modelsLoading,
  modelsError,
  onRefreshModels,
  customModelName,
}: ProviderCardProps) {
  const selectedModelData = models.find((m) => m.id === selectedModel);
  const isCustom = provider.isCustom;

  // Handle dropdown open - fetch models if not already loaded
  const handleDropdownOpenChange = (open: boolean) => {
    if (open && hasApiKey && !isCustom && models.length === 0 && !modelsLoading) {
      onRefreshModels();
    }
  };

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

          {/* Model Selection Dropdown - for standard providers */}
          {hasApiKey && !isCustom && (
            <DropdownMenu onOpenChange={handleDropdownOpenChange}>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 px-2 text-sm text-muted-foreground hover:text-foreground"
                >
                  {modelsLoading ? (
                    <>
                      <Loader2 className="w-3.5 h-3.5 animate-spin mr-1" />
                      Loading...
                    </>
                  ) : selectedModelData ? (
                    <>
                      {selectedModelData.recommended && (
                        <Star className="w-3 h-3 text-amber-500 fill-amber-500 mr-1" />
                      )}
                      <span className="truncate max-w-[180px]">
                        {selectedModelData.name}
                      </span>
                      <ChevronDown className="w-3.5 h-3.5 ml-1 flex-shrink-0" />
                    </>
                  ) : selectedModel ? (
                    <>
                      {/* Show model ID as fallback when model data not loaded */}
                      <span className="truncate max-w-[180px]">
                        {selectedModel}
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
              <DropdownMenuContent align="start" className="w-72">
                {/* Refresh button header */}
                <div className="flex items-center justify-between px-2 py-1.5">
                  <span className="text-xs text-muted-foreground font-medium">
                    Available Models
                  </span>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-6 w-6 p-0"
                    onClick={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      onRefreshModels();
                    }}
                    disabled={modelsLoading}
                  >
                    <RefreshCw className={`w-3.5 h-3.5 ${modelsLoading ? 'animate-spin' : ''}`} />
                  </Button>
                </div>
                <DropdownMenuSeparator />

                {/* Loading state */}
                {modelsLoading && models.length === 0 && (
                  <div className="flex items-center justify-center py-4 text-muted-foreground">
                    <Loader2 className="w-4 h-4 animate-spin mr-2" />
                    <span className="text-sm">Loading models...</span>
                  </div>
                )}

                {/* Error state */}
                {modelsError && (
                  <div className="px-2 py-3 text-center">
                    <p className="text-sm text-destructive">{modelsError}</p>
                    <Button
                      variant="ghost"
                      size="sm"
                      className="mt-2"
                      onClick={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        onRefreshModels();
                      }}
                    >
                      <RefreshCw className="w-3.5 h-3.5 mr-1" />
                      Retry
                    </Button>
                  </div>
                )}

                {/* Empty state */}
                {!modelsLoading && !modelsError && models.length === 0 && (
                  <div className="px-2 py-3 text-center text-sm text-muted-foreground">
                    No models available
                  </div>
                )}

                {/* Model list */}
                {models.map((model) => (
                  <DropdownMenuItem
                    key={model.id}
                    onClick={() => onSelectModel(model.id)}
                    className="flex items-start gap-2 py-2"
                  >
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        {model.recommended && (
                          <Star className="w-3 h-3 text-amber-500 fill-amber-500 flex-shrink-0" />
                        )}
                        <span className="font-medium truncate">{model.name}</span>
                        {selectedModel === model.id && (
                          <Check className="w-3.5 h-3.5 text-primary flex-shrink-0" />
                        )}
                      </div>
                      <p className="text-xs text-muted-foreground mt-0.5 truncate">
                        {model.id}
                      </p>
                    </div>
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          )}

          {/* Custom provider - show configured model or prompt */}
          {isCustom && hasApiKey && customModelName && (
            <p className="text-sm text-muted-foreground">
              Model: <span className="text-foreground">{customModelName}</span>
            </p>
          )}

          {/* Not configured prompt */}
          {!hasApiKey && (
            <p className="text-sm text-muted-foreground">
              {isCustom ? "Configure endpoint to enable" : "Add API key to enable"}
            </p>
          )}
        </div>

        {/* Action Buttons */}
        <div className="flex items-center gap-2">
          {hasApiKey ? (
            <>
              {/* For custom providers, show Configure button to edit */}
              {isCustom && (
                <Button onClick={onSetupApiKey} variant="ghost" size="sm">
                  <Settings2 className="w-3.5 h-3.5" />
                </Button>
              )}
              <Button
                onClick={async () => {
                  const message = isCustom 
                    ? `Remove configuration for ${provider.name}?`
                    : `Remove API key for ${provider.name}?`;
                  const confirmed = await ask(message, { 
                    title: isCustom ? "Remove Configuration" : "Remove API Key", 
                    kind: "warning" 
                  });
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
            </>
          ) : (
            <>
              {/* External link only for standard providers */}
              {!isCustom && provider.apiKeyUrl && (
                <Button
                  onClick={() => window.open(provider.apiKeyUrl, "_blank")}
                  variant="ghost"
                  size="sm"
                  className="text-muted-foreground"
                  title={`Get ${provider.name} API Key`}
                >
                  <ExternalLink className="w-3.5 h-3.5" />
                </Button>
              )}
              <Button onClick={onSetupApiKey} variant="outline" size="sm">
                {isCustom ? (
                  <>
                    <Settings2 className="w-3.5 h-3.5 mr-1.5" />
                    Configure
                  </>
                ) : (
                  <>
                    <Key className="w-3.5 h-3.5 mr-1.5" />
                    Add Key
                  </>
                )}
              </Button>
            </>
          )}
        </div>
      </div>
    </Card>
  );
}
