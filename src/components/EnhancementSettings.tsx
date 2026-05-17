import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { LanguageSelection } from "@/components/LanguageSelection";
import type {
  CustomWord,
  Snippet,
  TextReplacementRule,
  WritingSettings,
} from "@/types/writing";
import { FileText, Mail, GitCommit, Sparkles, Plus, Trash2 } from "lucide-react";

interface EnhancementSettingsProps {
  preset: "Default" | "Prompts" | "Email" | "Commit";
  finalTextLanguage: string;
  writingSettings: WritingSettings;
  onPresetChange: (preset: "Default" | "Prompts" | "Email" | "Commit") => void;
  onFinalTextLanguageChange: (value: string) => void;
  onWritingSettingsChange: (settings: WritingSettings) => void;
  disabled?: boolean;
}

function updateItem<T>(items: T[], index: number, next: T): T[] {
  return items.map((item, itemIndex) => (itemIndex === index ? next : item));
}

function removeItem<T>(items: T[], index: number): T[] {
  return items.filter((_, itemIndex) => itemIndex !== index);
}

function ReplacementEditor({
  replacements,
  onChange,
  disabled,
}: {
  replacements: TextReplacementRule[];
  onChange: (replacements: TextReplacementRule[]) => void;
  disabled: boolean;
}) {
  return (
    <div className="space-y-3 rounded-lg border border-border/50 p-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h4 className="text-sm font-medium">Text replacements</h4>
          <p className="text-xs text-muted-foreground">
            Deterministic corrections applied before any optional AI cleanup.
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={disabled}
          onClick={() =>
            onChange([
              ...replacements,
              { from: "", to: "", language: null, enabled: true },
            ])
          }
        >
          <Plus className="mr-2 h-4 w-4" />
          Add
        </Button>
      </div>

      {replacements.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          Example: <span className="font-mono">voice typer → VoiceTypr</span>
        </p>
      ) : (
        <div className="space-y-3">
          {replacements.map((rule, index) => (
            <div key={`replacement-${index}`} className="rounded-md border border-border/50 p-3 space-y-3">
              <div className="flex items-center justify-between gap-3">
                <Label className="text-xs font-medium">Replacement {index + 1}</Label>
                <div className="flex items-center gap-3">
                  <Label className="text-xs text-muted-foreground">Enabled</Label>
                  <Switch
                    checked={rule.enabled}
                    disabled={disabled}
                    onCheckedChange={(checked) =>
                      onChange(updateItem(replacements, index, { ...rule, enabled: checked }))
                    }
                  />
                  <Button
                    type="button"
                    size="icon"
                    variant="ghost"
                    disabled={disabled}
                    onClick={() => onChange(removeItem(replacements, index))}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              </div>
              <div className="grid gap-3 md:grid-cols-2">
                <Input
                  placeholder="Match text"
                  value={rule.from}
                  disabled={disabled}
                  onChange={(event) =>
                    onChange(updateItem(replacements, index, { ...rule, from: event.target.value }))
                  }
                />
                <Input
                  placeholder="Replace with"
                  value={rule.to}
                  disabled={disabled}
                  onChange={(event) =>
                    onChange(updateItem(replacements, index, { ...rule, to: event.target.value }))
                  }
                />
              </div>
              <Input
                placeholder="Language code (optional, e.g. en)"
                value={rule.language ?? ""}
                disabled={disabled}
                onChange={(event) =>
                  onChange(
                    updateItem(replacements, index, {
                      ...rule,
                      language: event.target.value || null,
                    }),
                  )
                }
              />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function CustomWordEditor({
  customWords,
  onChange,
  disabled,
}: {
  customWords: CustomWord[];
  onChange: (customWords: CustomWord[]) => void;
  disabled: boolean;
}) {
  return (
    <div className="space-y-3 rounded-lg border border-border/50 p-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h4 className="text-sm font-medium">Personal dictionary words</h4>
          <p className="text-xs text-muted-foreground">
            Canonical spellings plus optional spoken forms to preserve names and domain terms.
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={disabled}
          onClick={() =>
            onChange([
              ...customWords,
              { phrase: "", spoken_form: null, language: null, enabled: true },
            ])
          }
        >
          <Plus className="mr-2 h-4 w-4" />
          Add
        </Button>
      </div>

      {customWords.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          Example: canonical <span className="font-mono">VoiceTypr</span>, spoken form <span className="font-mono">voice typer</span>
        </p>
      ) : (
        <div className="space-y-3">
          {customWords.map((word, index) => (
            <div key={`custom-word-${index}`} className="rounded-md border border-border/50 p-3 space-y-3">
              <div className="flex items-center justify-between gap-3">
                <Label className="text-xs font-medium">Word {index + 1}</Label>
                <div className="flex items-center gap-3">
                  <Label className="text-xs text-muted-foreground">Enabled</Label>
                  <Switch
                    checked={word.enabled}
                    disabled={disabled}
                    onCheckedChange={(checked) =>
                      onChange(updateItem(customWords, index, { ...word, enabled: checked }))
                    }
                  />
                  <Button
                    type="button"
                    size="icon"
                    variant="ghost"
                    disabled={disabled}
                    onClick={() => onChange(removeItem(customWords, index))}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              </div>
              <div className="grid gap-3 md:grid-cols-2">
                <Input
                  placeholder="Canonical phrase"
                  value={word.phrase}
                  disabled={disabled}
                  onChange={(event) =>
                    onChange(updateItem(customWords, index, { ...word, phrase: event.target.value }))
                  }
                />
                <Input
                  placeholder="Spoken form (optional)"
                  value={word.spoken_form ?? ""}
                  disabled={disabled}
                  onChange={(event) =>
                    onChange(
                      updateItem(customWords, index, {
                        ...word,
                        spoken_form: event.target.value || null,
                      }),
                    )
                  }
                />
              </div>
              <Input
                placeholder="Language code (optional, e.g. en)"
                value={word.language ?? ""}
                disabled={disabled}
                onChange={(event) =>
                  onChange(
                    updateItem(customWords, index, {
                      ...word,
                      language: event.target.value || null,
                    }),
                  )
                }
              />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function SnippetEditor({
  snippets,
  onChange,
  disabled,
}: {
  snippets: Snippet[];
  onChange: (snippets: Snippet[]) => void;
  disabled: boolean;
}) {
  return (
    <div className="space-y-3 rounded-lg border border-border/50 p-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h4 className="text-sm font-medium">Snippets</h4>
          <p className="text-xs text-muted-foreground">
            Whole-utterance template expansions. Literal snippets skip cleanup unless you turn it off.
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={disabled}
          onClick={() =>
            onChange([
              ...snippets,
              {
                trigger: "",
                body: "",
                language: null,
                enabled: true,
                preserve_literal: true,
              },
            ])
          }
        >
          <Plus className="mr-2 h-4 w-4" />
          Add
        </Button>
      </div>

      {snippets.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          Example trigger: <span className="font-mono">insert bug report template</span>
        </p>
      ) : (
        <div className="space-y-3">
          {snippets.map((snippet, index) => (
            <div key={`snippet-${index}`} className="rounded-md border border-border/50 p-3 space-y-3">
              <div className="flex items-center justify-between gap-3">
                <Label className="text-xs font-medium">Snippet {index + 1}</Label>
                <div className="flex items-center gap-3">
                  <Label className="text-xs text-muted-foreground">Enabled</Label>
                  <Switch
                    checked={snippet.enabled}
                    disabled={disabled}
                    onCheckedChange={(checked) =>
                      onChange(updateItem(snippets, index, { ...snippet, enabled: checked }))
                    }
                  />
                  <Button
                    type="button"
                    size="icon"
                    variant="ghost"
                    disabled={disabled}
                    onClick={() => onChange(removeItem(snippets, index))}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              </div>
              <Input
                placeholder="Whole spoken trigger"
                value={snippet.trigger}
                disabled={disabled}
                onChange={(event) =>
                  onChange(updateItem(snippets, index, { ...snippet, trigger: event.target.value }))
                }
              />
              <Textarea
                placeholder="Snippet body"
                value={snippet.body}
                disabled={disabled}
                onChange={(event) =>
                  onChange(updateItem(snippets, index, { ...snippet, body: event.target.value }))
                }
              />
              <div className="grid gap-3 md:grid-cols-[1fr_auto] md:items-center">
                <Input
                  placeholder="Language code (optional, e.g. en)"
                  value={snippet.language ?? ""}
                  disabled={disabled}
                  onChange={(event) =>
                    onChange(
                      updateItem(snippets, index, {
                        ...snippet,
                        language: event.target.value || null,
                      }),
                    )
                  }
                />
                <div className="flex items-center gap-3">
                  <Label className="text-xs text-muted-foreground">Preserve literally</Label>
                  <Switch
                    checked={snippet.preserve_literal}
                    disabled={disabled}
                    onCheckedChange={(checked) =>
                      onChange(
                        updateItem(snippets, index, {
                          ...snippet,
                          preserve_literal: checked,
                        }),
                      )
                    }
                  />
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export function EnhancementSettings({
  preset,
  finalTextLanguage,
  writingSettings,
  onPresetChange,
  onFinalTextLanguageChange,
  onWritingSettingsChange,
  disabled = false,
}: EnhancementSettingsProps) {
  const presets = [
    { id: "Default", label: "Default", icon: FileText, description: "Clean text" },
    { id: "Prompts", label: "Prompts", icon: Sparkles, description: "AI prompts" },
    { id: "Email", label: "Email", icon: Mail, description: "Email format" },
    { id: "Commit", label: "Commit", icon: GitCommit, description: "Git messages" },
  ] as const;

  const usingSpecificLanguage = finalTextLanguage !== "same_as_transcript";

  return (
    <div className={`space-y-6 ${disabled ? "opacity-50" : ""}`}>
      <div className="space-y-3">
        <div className="flex flex-wrap gap-2">
          {presets.map((presetOption) => {
            const Icon = presetOption.icon;
            const isSelected = preset === presetOption.id;
            return (
              <Button
                key={presetOption.id}
                variant={isSelected ? "default" : "outline"}
                size="sm"
                className={`gap-2 ${disabled ? "cursor-not-allowed" : ""}`}
                onClick={() => !disabled && onPresetChange(presetOption.id)}
                disabled={disabled}
              >
                <Icon className="h-4 w-4" />
                {presetOption.label}
              </Button>
            );
          })}
        </div>
        <p className="text-sm text-muted-foreground">
          {preset === "Default" && "Clean dictation with grammar, punctuation, and intent cleanup."}
          {preset === "Prompts" && "Transform speech into clear, actionable AI prompts."}
          {preset === "Email" && "Format as a polished email with subject, greeting, and structure."}
          {preset === "Commit" && "Create a conventional commit message."}
        </p>
      </div>

      <div className="space-y-3 rounded-lg border border-border/50 p-4">
        <div className="space-y-1">
          <h4 className="text-sm font-medium">Final text language</h4>
          <p className="text-xs text-muted-foreground">
            Keep the transcript language, or request a final written language. English can use native speech translation when supported; other target languages rely on AI cleanup/translation.
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            size="sm"
            variant={!usingSpecificLanguage ? "default" : "outline"}
            disabled={disabled}
            onClick={() => onFinalTextLanguageChange("same_as_transcript")}
          >
            Same as transcript
          </Button>
          <Button
            type="button"
            size="sm"
            variant={usingSpecificLanguage ? "default" : "outline"}
            disabled={disabled}
            onClick={() => onFinalTextLanguageChange(usingSpecificLanguage ? finalTextLanguage : "en")}
          >
            Specific language
          </Button>
        </div>
        {usingSpecificLanguage && (
          <LanguageSelection
            value={finalTextLanguage}
            onValueChange={onFinalTextLanguageChange}
            className="w-full md:w-56"
          />
        )}
      </div>

      <div className="space-y-3 rounded-lg border border-border/50 p-4">
        <div className="flex items-start justify-between gap-3">
          <div className="space-y-1">
            <h4 className="text-sm font-medium">Context-aware cleanup</h4>
            <p className="text-xs text-muted-foreground">
              Optionally pass the active app name/category as a privacy-safe hint for writing style. No document contents or clipboard data are shared automatically.
            </p>
          </div>
          <Switch
            checked={writingSettings.context_policy === "app_hint_only"}
            disabled={disabled}
            onCheckedChange={(checked) =>
              onWritingSettingsChange({
                ...writingSettings,
                context_policy: checked ? "app_hint_only" : "off",
              })
            }
          />
        </div>
      </div>

      <ReplacementEditor
        replacements={writingSettings.replacements}
        disabled={disabled}
        onChange={(replacements) =>
          onWritingSettingsChange({ ...writingSettings, replacements })
        }
      />

      <CustomWordEditor
        customWords={writingSettings.custom_words}
        disabled={disabled}
        onChange={(custom_words) =>
          onWritingSettingsChange({ ...writingSettings, custom_words })
        }
      />

      <SnippetEditor
        snippets={writingSettings.snippets}
        disabled={disabled}
        onChange={(snippets) =>
          onWritingSettingsChange({ ...writingSettings, snippets })
        }
      />
    </div>
  );
}
