import { LanguageSelection } from "@/components/LanguageSelection";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
  FieldTitle,
} from "@/components/ui/field";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
  InputGroupText,
  InputGroupTextarea,
} from "@/components/ui/input-group";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { invoke } from "@tauri-apps/api/core";
import { presetDisplayLabel, presetRequiresAiFormatting, type EnhancementPreset } from "@/types/ai";
import type {
  AppFormattingRule,
  CustomWord,
  Snippet,
  TextReplacementRule,
  VoiceCommandRule,
  WritingSettings,
} from "@/types/writing";
import {
  AudioLines,
  Code,
  FileText,
  Globe,
  Lock,
  MessageSquare,
  PenLine,
  Loader2,
  Plus,
  StickyNote,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

interface EnhancementSettingsProps {
  preset: EnhancementPreset;
  finalTextLanguage: string;
  writingSettings: WritingSettings;
  aiFormattingEnabled: boolean;
  onPresetChange: (preset: EnhancementPreset) => void;
  onFinalTextLanguageChange: (value: string) => void;
  onWritingSettingsChange: (settings: WritingSettings) => void;
  disabled?: boolean;
  writingSettingsDisabled?: boolean;
}

function updateItem<T>(items: T[], index: number, next: T): T[] {
  return items.map((item, itemIndex) => (itemIndex === index ? next : item));
}

function removeItem<T>(items: T[], index: number): T[] {
  return items.filter((_, itemIndex) => itemIndex !== index);
}
type ParakeetVocabularyStatus = {
  supported: boolean;
  ready: boolean;
};
const FORMATTING_MODES = [
  { id: "PersonalDictation", icon: AudioLines },
  { id: "CleanDictation", icon: FileText },
  { id: "Writing", icon: PenLine },
  { id: "Notes", icon: StickyNote },
  { id: "Message", icon: MessageSquare },
  { id: "Code", icon: Code },
] as const satisfies ReadonlyArray<{
  id: EnhancementPreset;
  icon: typeof AudioLines;
}>;

const VOICE_COMMAND_OUTPUT_OPTIONS = [
  { value: "comma", label: "Comma ," },
  { value: "period", label: "Period ." },
  { value: "question_mark", label: "Question mark ?" },
  { value: "exclamation_mark", label: "Exclamation mark !" },
  { value: "colon", label: "Colon :" },
  { value: "semicolon", label: "Semicolon ;" },
  { value: "dash", label: "Dash —" },
  { value: "new_line", label: "New line" },
  { value: "paragraph", label: "New paragraph" },
] as const;

const formattingModeLabel = (preset: EnhancementPreset) => presetDisplayLabel(preset);

function AppFormattingRulesEditor({
  rules,
  onChange,
  disabled,
  aiFormattingEnabled,
}: {
  rules: AppFormattingRule[];
  onChange: (rules: AppFormattingRule[]) => void;
  disabled: boolean;
  aiFormattingEnabled: boolean;
}) {
  const hasAiRequiredSelection =
    !aiFormattingEnabled && rules.some((rule) => presetRequiresAiFormatting(rule.preset));

  return (
    <FieldSet className="mt-4 rounded-lg border border-border/60 bg-background/60 p-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <FieldLegend className="mb-1 text-sm">App Rules</FieldLegend>
          <FieldDescription>
            Switch mode when the active app matches. Uses the app name only — not URLs, titles, or
            clipboard.
          </FieldDescription>
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={disabled}
          onClick={() =>
            onChange([
              ...rules,
              { app_name: "", preset: "PersonalDictation", enabled: true },
            ])
          }
        >
          <Plus className="mr-2 h-4 w-4" />
          Add rule
        </Button>
      </div>

      {!aiFormattingEnabled && hasAiRequiredSelection && (
        <div className="mt-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-300">
          One or more app rules use AI modes. Turn on AI formatting with a selected provider model to activate them.
        </div>
      )}

      {rules.length === 0 ? (
        <Empty className="mt-3 border-border/60 bg-muted/20 p-4">
          <EmptyHeader className="max-w-none gap-1">
            <EmptyTitle className="text-sm">No app rules yet</EmptyTitle>
            <EmptyDescription className="text-xs">
              Example: when <span className="font-mono">Slack</span> is active, use{" "}
              <span className="font-mono">Message</span> mode.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <FieldGroup className="mt-3 gap-2">
          {rules.map((rule, index) => {
            const selectedMode = FORMATTING_MODES.find((mode) => mode.id === rule.preset);

            return (
              <div
                key={`app-rule-${index}`}
                className="rounded-lg border border-border/60 bg-card/60 p-3"
              >
                <div className="flex flex-wrap items-center gap-2">
                  <InputGroup className="min-w-[10rem] flex-1">
                    <InputGroupAddon>
                      <InputGroupText>App</InputGroupText>
                    </InputGroupAddon>
                    <InputGroupInput
                      placeholder="App name, e.g. Slack"
                      value={rule.app_name}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(rules, index, {
                            ...rule,
                            app_name: event.target.value,
                          }),
                        )
                      }
                    />
                  </InputGroup>

                  <Select
                    value={rule.preset}
                    disabled={disabled}
                    onValueChange={(value) =>
                      onChange(
                        updateItem(rules, index, {
                          ...rule,
                          preset: value as EnhancementPreset,
                        }),
                      )
                    }
                  >
                    <SelectTrigger size="sm" className="w-[11rem]" aria-label="Formatting mode">
                      <SelectValue placeholder="Mode">
                        {selectedMode ? formattingModeLabel(selectedMode.id) : rule.preset}
                      </SelectValue>
                    </SelectTrigger>
                    <SelectContent>
                      {FORMATTING_MODES.map((modeOption) => {
                        const requiresAi = presetRequiresAiFormatting(modeOption.id);
                        const isSelected = rule.preset === modeOption.id;
                        const isOptionDisabled =
                          disabled || (requiresAi && !aiFormattingEnabled && !isSelected);

                        return (
                          <SelectItem
                            key={modeOption.id}
                            value={modeOption.id}
                            disabled={isOptionDisabled}
                          >
                            {formattingModeLabel(modeOption.id)}
                            {requiresAi && !aiFormattingEnabled && !isSelected
                              ? " (requires AI)"
                              : ""}
                          </SelectItem>
                        );
                      })}
                    </SelectContent>
                  </Select>

                  <div className="flex items-center gap-2">
                    <span className="text-xs text-muted-foreground">Enabled</span>
                    <Switch
                      checked={rule.enabled}
                      disabled={disabled}
                      onCheckedChange={(checked) =>
                        onChange(updateItem(rules, index, { ...rule, enabled: checked }))
                      }
                    />
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      disabled={disabled}
                      onClick={() => onChange(removeItem(rules, index))}
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                </div>

              </div>
            );
          })}
        </FieldGroup>
      )}
    </FieldSet>
  );
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
    <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <FieldLegend className="mb-1">Corrections</FieldLegend>
          <FieldDescription>Find-and-replace rules. Always applied, with or without AI.</FieldDescription>
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
          Add rule
        </Button>
      </div>

      {replacements.length === 0 ? (
        <Empty className="mt-3 border-border/60 bg-muted/20 p-6">
          <EmptyHeader className="max-w-none gap-1">
            <EmptyTitle className="text-sm">No replacement rules yet</EmptyTitle>
            <EmptyDescription className="text-xs">
              Example: <span className="font-mono">voice typer → Voicetypr</span>
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <FieldGroup className="mt-3 gap-3">
          {replacements.map((rule, index) => (
            <FieldSet
              key={`replacement-${index}`}
              className="rounded-lg border border-border/60 bg-background/60 p-3"
            >
              <div className="mb-3 flex items-center justify-between gap-3">
                <FieldTitle>Rule {index + 1}</FieldTitle>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">Enabled</span>
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

              <FieldGroup className="gap-3">
                <Field>
                  <InputGroup>
                    <InputGroupAddon>
                      <InputGroupText>Match</InputGroupText>
                    </InputGroupAddon>
                    <InputGroupInput
                      placeholder="Text to match"
                      value={rule.from}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(replacements, index, {
                            ...rule,
                            from: event.target.value,
                          }),
                        )
                      }
                    />
                  </InputGroup>
                </Field>
                <Field>
                  <InputGroup>
                    <InputGroupAddon>
                      <InputGroupText>Replace</InputGroupText>
                    </InputGroupAddon>
                    <InputGroupInput
                      placeholder="Replacement text"
                      value={rule.to}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(replacements, index, {
                            ...rule,
                            to: event.target.value,
                          }),
                        )
                      }
                    />
                  </InputGroup>
                </Field>
                <Field>
                  <InputGroup>
                    <InputGroupAddon>
                      <Globe className="h-4 w-4" />
                    </InputGroupAddon>
                    <InputGroupInput
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
                  </InputGroup>
                </Field>
              </FieldGroup>
            </FieldSet>
          ))}
        </FieldGroup>
      )}
    </FieldSet>
  );
}

function VoiceCommandEditor({
  voiceCommands,
  onChange,
  disabled,
}: {
  voiceCommands: VoiceCommandRule[];
  onChange: (voiceCommands: VoiceCommandRule[]) => void;
  disabled: boolean;
}) {
  return (
    <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <FieldLegend className="mb-1">Voice Commands</FieldLegend>
          <FieldDescription>
            Spoken phrases that insert punctuation or breaks. Not AI. Example:{" "}
            <span className="font-mono">new paragraph</span> starts a new paragraph.
          </FieldDescription>
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={disabled}
          onClick={() =>
            onChange([
              ...voiceCommands,
              { phrase: "", output: "period", language: null, enabled: true },
            ])
          }
        >
          <Plus className="mr-2 h-4 w-4" />
          Add command
        </Button>
      </div>

      {voiceCommands.length === 0 ? (
        <Empty className="mt-3 border-border/60 bg-muted/20 p-6">
          <EmptyHeader className="max-w-none gap-1">
            <EmptyTitle className="text-sm">No voice commands yet</EmptyTitle>
            <EmptyDescription className="text-xs">
              Example: <span className="font-mono">new paragraph</span> inserts a paragraph break.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <FieldGroup className="mt-3 gap-3">
          {voiceCommands.map((rule, index) => {
            const selectedOutput = VOICE_COMMAND_OUTPUT_OPTIONS.find(
              (option) => option.value === rule.output,
            );

            return (
              <FieldSet
                key={`voice-command-${index}`}
                className="rounded-lg border border-border/60 bg-background/60 p-3"
              >
                <div className="mb-3 flex items-center justify-between gap-3">
                  <FieldTitle>Command {index + 1}</FieldTitle>
                  <div className="flex items-center gap-2">
                    <span className="text-xs text-muted-foreground">Enabled</span>
                    <Switch
                      aria-label={`Enable voice command ${index + 1}`}
                      checked={rule.enabled}
                      disabled={disabled}
                      onCheckedChange={(checked) =>
                        onChange(updateItem(voiceCommands, index, { ...rule, enabled: checked }))
                      }
                    />
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      aria-label={`Delete voice command ${index + 1}`}
                      disabled={disabled}
                      onClick={() => onChange(removeItem(voiceCommands, index))}
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                </div>

                <FieldGroup className="gap-3">
                  <Field>
                    <InputGroup>
                      <InputGroupAddon>
                        <InputGroupText>When I say…</InputGroupText>
                      </InputGroupAddon>
                      <InputGroupInput
                        aria-label={`Voice command phrase ${index + 1}`}
                        placeholder="Spoken phrase"
                        value={rule.phrase}
                        disabled={disabled}
                        onChange={(event) =>
                          onChange(
                            updateItem(voiceCommands, index, {
                              ...rule,
                              phrase: event.target.value,
                            }),
                          )
                        }
                      />
                    </InputGroup>
                  </Field>

                  <Field>
                    <Select
                      value={rule.output}
                      disabled={disabled}
                      onValueChange={(value) =>
                        onChange(
                          updateItem(voiceCommands, index, {
                            ...rule,
                            output: value,
                          }),
                        )
                      }
                    >
                      <SelectTrigger
                        size="sm"
                        className="w-full"
                        aria-label={`Voice command output ${index + 1}`}
                      >
                        <SelectValue placeholder="Output">
                          {selectedOutput ? selectedOutput.label : rule.output}
                        </SelectValue>
                      </SelectTrigger>
                      <SelectContent>
                        {VOICE_COMMAND_OUTPUT_OPTIONS.map((option) => (
                          <SelectItem key={option.value} value={option.value}>
                            {option.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </Field>

                  <Field>
                    <InputGroup>
                      <InputGroupAddon>
                        <Globe className="h-4 w-4" />
                      </InputGroupAddon>
                      <InputGroupInput
                        aria-label={`Voice command language ${index + 1}`}
                        placeholder="Language code (optional, e.g. en)"
                        value={rule.language ?? ""}
                        disabled={disabled}
                        onChange={(event) =>
                          onChange(
                            updateItem(voiceCommands, index, {
                              ...rule,
                              language: event.target.value || null,
                            }),
                          )
                        }
                      />
                    </InputGroup>
                  </Field>
                </FieldGroup>
              </FieldSet>
            );
          })}
        </FieldGroup>
      )}
    </FieldSet>
  );
}

function ParakeetVocabularyBoostRow({ disabled }: { disabled: boolean }) {
  const [status, setStatus] = useState<ParakeetVocabularyStatus | null>(null);
  const [isDownloading, setIsDownloading] = useState(false);

  const refreshStatus = useCallback(async () => {
    try {
      const nextStatus = await invoke<ParakeetVocabularyStatus>(
        "get_parakeet_vocabulary_status",
      );
      setStatus(nextStatus.supported ? nextStatus : null);
    } catch {
      setStatus(null);
    }
  }, []);

  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  const handleDownload = async () => {
    setIsDownloading(true);
    try {
      await invoke("download_parakeet_vocabulary_model");
      await refreshStatus();
    } catch {
      toast.error("Failed to download Parakeet vocabulary model");
    } finally {
      setIsDownloading(false);
    }
  };

  if (!status) {
    return null;
  }

  if (status.ready) {
    return (
      <div className="mt-3 rounded-lg border border-border/60 bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
        Vocabulary boost installed. Parakeet can use your Words & Names on-device after
        transcription to help with names and domain terms.
      </div>
    );
  }

  return (
    <div className="mt-3 flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border/60 bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
      <div>
        <div className="font-medium text-foreground">
          Parakeet vocabulary boost (optional, ~70–100MB)
        </div>
        <div>
          Runs on-device after transcription to help Parakeet recognize your Words & Names,
          including names and domain terms.
        </div>
      </div>
      <Button
        type="button"
        size="sm"
        variant="outline"
        disabled={disabled || isDownloading}
        onClick={handleDownload}
        aria-label="Download Parakeet vocabulary boost model"
      >
        {isDownloading ? (
          <>
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            Downloading…
          </>
        ) : (
          "Download"
        )}
      </Button>
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
    <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <FieldLegend className="mb-1">Words & Names</FieldLegend>
          <FieldDescription>
            Used to correct spelling; also improves recognition on Whisper, Parakeet, and Soniox.
          </FieldDescription>
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
          Add word
        </Button>
      </div>

      <ParakeetVocabularyBoostRow disabled={disabled} />

      {customWords.length === 0 ? (
        <Empty className="mt-3 border-border/60 bg-muted/20 p-6">
          <EmptyHeader className="max-w-none gap-1">
            <EmptyTitle className="text-sm">No words or names yet</EmptyTitle>
            <EmptyDescription className="text-xs">
              Example: canonical <span className="font-mono">Voicetypr</span>, spoken form{" "}
              <span className="font-mono">voice typer</span>
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <FieldGroup className="mt-3 gap-3">
          {customWords.map((word, index) => (
            <FieldSet
              key={`custom-word-${index}`}
              className="rounded-lg border border-border/60 bg-background/60 p-3"
            >
              <div className="mb-3 flex items-center justify-between gap-3">
                <FieldTitle>Word {index + 1}</FieldTitle>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">Enabled</span>
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

              <FieldGroup className="gap-3">
                <Field>
                  <InputGroup>
                    <InputGroupAddon>
                      <InputGroupText>Canonical</InputGroupText>
                    </InputGroupAddon>
                    <InputGroupInput
                      placeholder="Canonical phrase"
                      value={word.phrase}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(customWords, index, {
                            ...word,
                            phrase: event.target.value,
                          }),
                        )
                      }
                    />
                  </InputGroup>
                </Field>
                <Field>
                  <InputGroup>
                    <InputGroupAddon>
                      <InputGroupText>Spoken</InputGroupText>
                    </InputGroupAddon>
                    <InputGroupInput
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
                  </InputGroup>
                </Field>
                <Field>
                  <InputGroup>
                    <InputGroupAddon>
                      <Globe className="h-4 w-4" />
                    </InputGroupAddon>
                    <InputGroupInput
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
                  </InputGroup>
                </Field>
              </FieldGroup>
            </FieldSet>
          ))}
        </FieldGroup>
      )}
    </FieldSet>
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
    <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <FieldLegend className="mb-1">Text Shortcuts</FieldLegend>
          <FieldDescription>
            Replace a whole spoken phrase with a template. "Preserve literally" skips cleanup.
          </FieldDescription>
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
          Add shortcut
        </Button>
      </div>

      {snippets.length === 0 ? (
        <Empty className="mt-3 border-border/60 bg-muted/20 p-6">
          <EmptyHeader className="max-w-none gap-1">
            <EmptyTitle className="text-sm">No text shortcuts yet</EmptyTitle>
            <EmptyDescription className="text-xs">
              Example trigger: <span className="font-mono">insert bug report template</span>
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <FieldGroup className="mt-3 gap-3">
          {snippets.map((snippet, index) => (
            <FieldSet
              key={`snippet-${index}`}
              className="rounded-lg border border-border/60 bg-background/60 p-3"
            >
              <div className="mb-3 flex items-center justify-between gap-3">
                <FieldTitle>Shortcut {index + 1}</FieldTitle>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">Enabled</span>
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

              <FieldGroup className="gap-3">
                <Field>
                  <InputGroup>
                    <InputGroupAddon>
                      <InputGroupText>Trigger</InputGroupText>
                    </InputGroupAddon>
                    <InputGroupInput
                      placeholder="Whole spoken trigger"
                      value={snippet.trigger}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(snippets, index, {
                            ...snippet,
                            trigger: event.target.value,
                          }),
                        )
                      }
                    />
                  </InputGroup>
                </Field>

                <Field>
                  <InputGroup>
                    <InputGroupAddon align="block-start">
                      <InputGroupText>Body</InputGroupText>
                    </InputGroupAddon>
                    <InputGroupTextarea
                      placeholder="Snippet body"
                      value={snippet.body}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(snippets, index, {
                            ...snippet,
                            body: event.target.value,
                          }),
                        )
                      }
                    />
                  </InputGroup>
                </Field>

                <Field orientation="responsive">
                  <FieldContent>
                    <InputGroup>
                      <InputGroupAddon>
                        <Globe className="h-4 w-4" />
                      </InputGroupAddon>
                      <InputGroupInput
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
                    </InputGroup>
                  </FieldContent>
                  <FieldLabel className="md:justify-end">
                    <Field orientation="horizontal">
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
                      <FieldContent>
                        <FieldTitle className="text-xs">Preserve literally</FieldTitle>
                      </FieldContent>
                    </Field>
                  </FieldLabel>
                </Field>
              </FieldGroup>
            </FieldSet>
          ))}
        </FieldGroup>
      )}
    </FieldSet>
  );
}

export function EnhancementSettings({
  preset,
  finalTextLanguage,
  writingSettings,
  aiFormattingEnabled,
  onPresetChange,
  onFinalTextLanguageChange,
  onWritingSettingsChange,
  disabled = false,
  writingSettingsDisabled = disabled,
}: EnhancementSettingsProps) {
  const allowsSpecificFinalLanguage = preset !== "PersonalDictation";
  const usingSpecificLanguage =
    allowsSpecificFinalLanguage && finalTextLanguage !== "same_as_transcript";
  const selectedRequiresAi = presetRequiresAiFormatting(preset);
  return (
    <div className={`space-y-4 ${disabled ? "opacity-60" : ""}`}>
      <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
        <FieldLegend className="mb-1">Formatting mode</FieldLegend>
        <FieldDescription className="mb-3">Pick how the final text is shaped.</FieldDescription>
        {!aiFormattingEnabled && selectedRequiresAi && (
          <div className="mb-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-300">
            {formattingModeLabel(preset)} needs AI. Turn on AI with a provider model, or pick
            Personal Dictation.
          </div>
        )}
        <ButtonGroup className="w-full flex-wrap md:w-fit">
          {FORMATTING_MODES.map((modeOption) => {
            const Icon = modeOption.icon;
            const isSelected = preset === modeOption.id;
            const requiresAi = presetRequiresAiFormatting(modeOption.id);
            const modeLabel = formattingModeLabel(modeOption.id);
            const isModeDisabled =
              disabled || (requiresAi && !aiFormattingEnabled && !isSelected);
            const aiRequiredHint = requiresAi
              ? `${modeLabel} requires AI formatting. Turn on AI formatting with a selected provider model.`
              : undefined;
            return (
              <Button
                key={modeOption.id}
                type="button"
                variant={isSelected ? "default" : "outline"}
                size="sm"
                disabled={isModeDisabled}
                title={aiRequiredHint}
                aria-label={
                  requiresAi && !aiFormattingEnabled
                    ? `${modeLabel} (requires AI formatting)`
                    : modeLabel
                }
                onClick={() => !isModeDisabled && onPresetChange(modeOption.id)}
              >
                <Icon className="h-4 w-4" />
                {modeLabel}
                {requiresAi && !aiFormattingEnabled && (
                  <Lock className="h-3 w-3 opacity-70" aria-hidden="true" />
                )}
              </Button>
            );
          })}
        </ButtonGroup>
        <FieldDescription className="mt-3">
          {preset === "PersonalDictation" && "Just transcription with local cleanup. No AI."}
          {preset === "CleanDictation" && "AI fixes grammar and punctuation. Keeps your meaning."}
          {preset === "Writing" && "AI polishes it into clear prose."}
          {preset === "Notes" && "AI turns it into short, structured notes."}
          {preset === "Message" && "AI formats it as a short message."}
          {preset === "Code" && "AI formats commits and code notes."}
        </FieldDescription>
      </FieldSet>

      <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
        <FieldLegend className="mb-1">Final Text Language</FieldLegend>
        <FieldDescription className="mb-3">
          Keep the transcript language, or pick a different written language. Changing it needs AI.
        </FieldDescription>
        {!aiFormattingEnabled && finalTextLanguage !== "same_as_transcript" && (
          <div className="mb-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-300">
            Turn on AI and pick an AI mode to use a final text language different from the transcript.
          </div>
        )}

        <ButtonGroup className="w-full flex-wrap md:w-fit">
          <Button
            type="button"
            variant={!usingSpecificLanguage ? "default" : "outline"}
            size="sm"
            disabled={disabled}
            onClick={() => onFinalTextLanguageChange("same_as_transcript")}
          >
            Same as transcript
          </Button>
          <Button
            type="button"
            variant={usingSpecificLanguage ? "default" : "outline"}
            size="sm"
            disabled={disabled || !allowsSpecificFinalLanguage}
            onClick={() =>
              onFinalTextLanguageChange(usingSpecificLanguage ? finalTextLanguage : "en")
            }
          >
            Specific language
          </Button>
        </ButtonGroup>

        {usingSpecificLanguage && (
          <div className="mt-3">
            <LanguageSelection
              value={finalTextLanguage}
              onValueChange={onFinalTextLanguageChange}
              className="w-full md:w-64"
            />
          </div>
        )}
      </FieldSet>

      <AppFormattingRulesEditor
        rules={writingSettings.app_formatting_rules}
        disabled={writingSettingsDisabled}
        aiFormattingEnabled={aiFormattingEnabled}
        onChange={(app_formatting_rules) =>
          onWritingSettingsChange({ ...writingSettings, app_formatting_rules })
        }
      />

      <header className="pt-2">
        <h2 className="text-base font-semibold">Your text rules (always on)</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          Exact, predictable edits. Run on every transcription, with or without AI.
        </p>
      </header>
      <ReplacementEditor
        replacements={writingSettings.replacements}
        disabled={writingSettingsDisabled}
        onChange={(replacements) =>
          onWritingSettingsChange({ ...writingSettings, replacements })
        }
      />

      <CustomWordEditor
        customWords={writingSettings.custom_words}
        disabled={writingSettingsDisabled}
        onChange={(custom_words) =>
          onWritingSettingsChange({ ...writingSettings, custom_words })
        }
      />

      <VoiceCommandEditor
        voiceCommands={writingSettings.voice_commands}
        disabled={writingSettingsDisabled}
        onChange={(voice_commands) =>
          onWritingSettingsChange({ ...writingSettings, voice_commands })
        }
      />

      <SnippetEditor
        snippets={writingSettings.snippets}
        disabled={writingSettingsDisabled}
        onChange={(snippets) =>
          onWritingSettingsChange({ ...writingSettings, snippets })
        }
      />
    </div>
  );
}
