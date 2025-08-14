import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { useState } from "react";
import { FileText, Mail, GitCommit, Sparkles, X } from "lucide-react";

interface EnhancementSettingsProps {
  settings: {
    preset: "Default" | "Prompts" | "Email" | "Commit";
    customVocabulary: string[];
  };
  onSettingsChange: (settings: any) => void;
}

export function EnhancementSettings({ settings, onSettingsChange }: EnhancementSettingsProps) {
  const [vocabularyInput, setVocabularyInput] = useState("");

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

  const handleAddVocabulary = () => {
    const terms = vocabularyInput.split(",").map(t => t.trim()).filter(t => t);
    if (terms.length > 0) {
      onSettingsChange({
        ...settings,
        customVocabulary: [...settings.customVocabulary, ...terms]
      });
      setVocabularyInput("");
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' && vocabularyInput.trim()) {
      handleAddVocabulary();
    }
  };

  const handleRemoveTerm = (term: string) => {
    onSettingsChange({
      ...settings,
      customVocabulary: settings.customVocabulary.filter(t => t !== term)
    });
  };

  return (
    <div className="space-y-6">
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
                className="gap-2"
                onClick={() => handlePresetChange(preset.id)}
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

      {/* Custom Vocabulary */}
      <div className="space-y-3">
        <Label className="text-sm font-medium">Custom Vocabulary</Label>
        <Input
          placeholder="Add terms (comma separated)"
          value={vocabularyInput}
          onChange={(e) => setVocabularyInput(e.target.value)}
          onKeyDown={handleKeyDown}
          className="text-sm"
        />
        
        {settings.customVocabulary.length > 0 && (
          <div className="flex flex-wrap gap-1.5">
            {settings.customVocabulary.map((term) => (
              <Badge
                key={term}
                variant="secondary"
                className="gap-1 pr-1"
              >
                {term}
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-4 w-4 p-0 hover:bg-transparent"
                  onClick={() => handleRemoveTerm(term)}
                >
                  <X className="h-3 w-3" />
                </Button>
              </Badge>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}