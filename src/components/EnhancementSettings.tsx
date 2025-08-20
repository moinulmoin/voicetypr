import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { FileText, Mail, GitCommit, Sparkles } from "lucide-react";

interface EnhancementSettingsProps {
  settings: {
    preset: "Default" | "Prompts" | "Email" | "Commit";
    customVocabulary: string[];
  };
  onSettingsChange: (settings: any) => void;
  disabled?: boolean;
}

export function EnhancementSettings({ settings, onSettingsChange, disabled = false }: EnhancementSettingsProps) {
  const presets = [
    {
      id: "Default",
      label: "Default",
      icon: FileText,
      description: "Clean text"
    },
    {
      id: "Prompts",
      label: "Prompts",
      icon: Sparkles,
      description: "AI prompts"
    },
    {
      id: "Email",
      label: "Email",
      icon: Mail,
      description: "Email format"
    },
    {
      id: "Commit",
      label: "Commit",
      icon: GitCommit,
      description: "Git messages"
    }
  ];

  const handlePresetChange = (preset: string) => {
    if (["Default", "Prompts", "Email", "Commit"].includes(preset)) {
      onSettingsChange({
        ...settings,
        preset: preset as "Default" | "Prompts" | "Email" | "Commit"
      });
    }
  };


  return (
    <div className={`space-y-6 ${disabled ? 'opacity-50' : ''}`}>
      {/* Enhancement Mode */}
      <div className="space-y-3">
        <Label className="text-sm font-medium">Enhancement Mode</Label>
        <div className="flex flex-wrap gap-2">
          {presets.map((preset) => {
            const Icon = preset.icon;
            const isSelected = settings.preset === preset.id;
            
            return (
              <Button
                key={preset.id}
                variant={isSelected ? "default" : "outline"}
                size="sm"
                className={`gap-2 ${disabled ? 'cursor-not-allowed' : ''}`}
                onClick={() => !disabled && handlePresetChange(preset.id)}
                disabled={disabled}
              >
                <Icon className="h-4 w-4" />
                {preset.label}
              </Button>
            );
          })}
        </div>
        
        {/* Mode description */}
        <p className="text-sm text-muted-foreground">
          {settings.preset === "Default" && "Comprehensive cleaning with grammar, spelling, formatting, and lists"}
          {settings.preset === "Prompts" && "Transforms speech into well-structured AI prompts"}
          {settings.preset === "Email" && "Formats speech into professional emails with subject and greeting"}
          {settings.preset === "Commit" && "Creates conventional commit messages with type and scope"}
        </p>
      </div>
    </div>
  );
}