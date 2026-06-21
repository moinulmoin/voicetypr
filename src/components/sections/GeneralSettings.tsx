import { HotkeyInput, type BareModifierSpec } from "@/components/HotkeyInput";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLegend,
  FieldSet,
  FieldTitle,
} from "@/components/ui/field";
import { Kbd } from "@/components/ui/kbd";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { useCanAutoInsert } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { updateService } from "@/services/updateService";
import { isMacOS, isWindows } from "@/lib/platform";
import { PillIndicatorMode, PillIndicatorPosition, TranscriptionAcceleration } from "@/types";
import { invoke } from "@tauri-apps/api/core";
import type { ShortcutBinding, ShortcutSettings } from "@/types/shortcuts";
import { AlertCircle, Check, Edit2, FolderOpen, HelpCircle, Mic, RefreshCw, Rocket, X } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { MicrophoneSelection } from "../MicrophoneSelection";
import { NetworkSharingCard } from "./NetworkSharingCard";
import { createLogger } from "@/lib/logger";

const log = createLogger("settings");

const BARE_MOD_ICONS: Record<string, string> = {
  alt: "⌥", meta: "⌘", control: "⌃", shift: "⇧",
};

function formatModifierLabel(mod: { modifier: string; side: string }): string {
  const sideLabel = mod.side === "right" ? "Right " : mod.side === "left" ? "Left " : "";
  const modLabel = isMacOS
    ? (BARE_MOD_ICONS[mod.modifier] ?? mod.modifier)
    : mod.modifier.charAt(0).toUpperCase() + mod.modifier.slice(1);
  return `${sideLabel}${modLabel}`;
}

function formatPrimaryHotkeyLabel(
  binding: ShortcutBinding | null,
  hotkey: string | undefined,
): string {
  if (binding?.modifier) {
    const mod = formatModifierLabel(binding.modifier);
    if (binding.trigger_kind === "isolated_tap") return `Tap ${mod} to toggle`;
    if (binding.trigger_kind === "modifier_hold") return `Hold ${mod} to talk`;
    return mod;
  }
  if (hotkey) return hotkey;
  return "Not set";
}


export function GeneralSettings() {
  const { settings, updateSettings } = useSettings();
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [autostartLoading, setAutostartLoading] = useState(false);
  const [showAccessibilityWarning, setShowAccessibilityWarning] = useState(true);
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);
  const canAutoInsert = useCanAutoInsert();
  const [nativeBinding, setNativeBinding] = useState<ShortcutBinding | null>(null);
  const [isEditingHotkey, setIsEditingHotkey] = useState(false);
  const [pendingHotkey, setPendingHotkey] = useState("");
  const [pendingBareModifier, setPendingBareModifier] = useState<BareModifierSpec | null>(null);
  const [holdToTalk, setHoldToTalk] = useState(false);

  useEffect(() => {
    const checkAutostart = async () => {
      try {
        const enabled = await invoke<boolean>("get_autostart_status");
        setAutostartEnabled(enabled);
      } catch (error) {
        log.error("Failed to check autostart status:", error);
      }
    };

    checkAutostart();
    setShowAccessibilityWarning(isMacOS);
  }, []);

  useEffect(() => {
    const hotkey = settings?.hotkey;
    if (hotkey) {
      setNativeBinding(null);
      return;
    }
    let cancelled = false;
    invoke<ShortcutSettings>("get_shortcut_settings")
      .then((result) => {
        if (cancelled) return;
        const bs = result.bindings;
        const found =
          bs.find((b) => b.id === "onboarding-primary-hold") ??
          bs.find(
            (b) =>
              b.enabled &&
              (b.action === "hold_to_record" || b.action === "toggle_recording") &&
              (b.trigger_kind === "modifier_hold" || b.trigger_kind === "double_tap" || b.trigger_kind === "isolated_tap"),
          ) ??
          null;
        setNativeBinding(found);
      })
      .catch(() => {
        if (!cancelled) setNativeBinding(null);
      });
    return () => {
      cancelled = true;
    };
  }, [settings?.hotkey]);

  if (!settings) return null;

  const startEditing = () => {
    setPendingHotkey(settings.hotkey || "");
    setPendingBareModifier(null);
    setHoldToTalk(
      nativeBinding?.action === "hold_to_record" ||
        (!nativeBinding && settings.recording_mode === "push_to_talk"),
    );
    setIsEditingHotkey(true);
  };

  const handleCancelHotkey = () => {
    setIsEditingHotkey(false);
    setPendingHotkey("");
    setPendingBareModifier(null);
  };

  const handleSaveHotkey = async () => {
    if (pendingBareModifier) {
      try {
        const existing = await invoke<ShortcutSettings>("get_shortcut_settings");
        const existingPrimary =
          existing.bindings.find((b) => b.id === "onboarding-primary-hold") ??
          existing.bindings.find(
            (b) =>
              b.enabled &&
              (b.action === "hold_to_record" || b.action === "toggle_recording") &&
              (b.trigger_kind === "modifier_hold" ||
                b.trigger_kind === "double_tap" ||
                b.trigger_kind === "isolated_tap"),
          );
        const stableId = existingPrimary?.id ?? "onboarding-primary-hold";
        const newBinding: ShortcutBinding = holdToTalk
          ? {
              id: stableId,
              action: "hold_to_record",
              shortcut: "",
              trigger: "hold",
              enabled: true,
              allow_risky_combo: false,
              trigger_kind: "modifier_hold",
              modifier: {
                modifier: pendingBareModifier.modifier as import("@/types/shortcuts").ModifierKind,
                side: pendingBareModifier.side as import("@/types/shortcuts").ModifierSide,
              },
            }
          : {
              id: stableId,
              action: "toggle_recording",
              shortcut: "",
              trigger: "pressed",
              enabled: true,
              allow_risky_combo: false,
              trigger_kind: "isolated_tap",
              modifier: {
                modifier: pendingBareModifier.modifier as import("@/types/shortcuts").ModifierKind,
                side: pendingBareModifier.side as import("@/types/shortcuts").ModifierSide,
              },
            };
        const updatedBindings = existingPrimary
          ? existing.bindings.map((b) => (b.id === stableId ? newBinding : b))
          : [...existing.bindings, newBinding];
        await invoke("update_shortcut_settings", {
          settings: { bindings: updatedBindings },
        });
        if (settings.hotkey) {
          await updateSettings({ hotkey: "" });
        }
        setNativeBinding(newBinding);
        setIsEditingHotkey(false);
        setPendingHotkey("");
        setPendingBareModifier(null);
        toast.success("Hotkey updated successfully!");
      } catch (err) {
        log.error("Failed to save bare modifier hotkey:", err);
        toast.error("Failed to save hotkey. Please try again.");
      }
    } else if (pendingHotkey) {
      try {
        await invoke("set_global_shortcut", { shortcut: pendingHotkey });
        await updateSettings({ hotkey: pendingHotkey });
        setNativeBinding(null);
        setIsEditingHotkey(false);
        setPendingHotkey("");
        toast.success("Hotkey updated successfully!");
      } catch (err) {
        log.error("Failed to update hotkey:", err);
        const errorMessage = err instanceof Error ? err.message : String(err);
        toast.error(errorMessage || "Failed to update hotkey. Please try a different combination.");
      }
    }
  };

  const handleAutostartToggle = async (checked: boolean) => {
    setAutostartLoading(true);
    try {
      const actualState = await invoke<boolean>("set_autostart", {
        enabled: checked,
      });
      setAutostartEnabled(actualState);
      await updateSettings({ launch_at_startup: actualState });

      if (actualState !== checked) {
        toast.warning(
          `Autostart ${checked ? "enable" : "disable"} failed. Current state: ${actualState ? "enabled" : "disabled"}.`,
        );
      }
    } catch (error) {
      log.error("Failed to toggle autostart:", error);
    } finally {
      setAutostartLoading(false);
    }
  };

  const handleCheckUpdate = async () => {
    setIsCheckingUpdate(true);
    try {
      await updateService.checkForUpdatesManually();
    } finally {
      setIsCheckingUpdate(false);
    }
  };

  return (
    <div className="h-full min-h-0 flex flex-col">
      <div className="shrink-0 border-b border-border/40 px-6 py-4">
        <div className="flex items-center gap-2">
          <h1 className="text-2xl font-semibold">Settings</h1>
          <Dialog>
            <DialogTrigger asChild>
              <Button type="button" variant="secondary" size="icon" aria-label="Settings guide" className="rounded-full">
                <HelpCircle className="h-4.5 w-4.5" />
              </Button>
            </DialogTrigger>
            <DialogContent className="sm:max-w-lg">
              <DialogHeader>
                <DialogTitle>Settings guide</DialogTitle>
                <DialogDescription>
                  Settings covers recording controls, insertion behavior, transcript cleanup, and startup defaults.
                </DialogDescription>
              </DialogHeader>
              <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                <p><strong className="text-foreground">Recording</strong> controls the global shortcut, microphone, and floating indicator.</p>
                <p><strong className="text-foreground">Transcript handling</strong> controls paste behavior, clipboard preservation, and history cleanup.</p>
                <p><strong className="text-foreground">Startup</strong> controls launch-at-login and background update checks.</p>
              </div>
            </DialogContent>
          </Dialog>
        </div>
        <p className="mt-1 text-sm text-muted-foreground">
          Configure capture controls, transcript behavior, and startup defaults.
        </p>
      </div>

      <ScrollArea className="flex-1 min-h-0">
        <div className="space-y-5 p-6">
          <div className="rounded-xl border border-border/60 bg-card p-4">
            <div className="mb-4 flex items-center gap-2">
              <div className="rounded-md bg-primary/10 p-1.5">
                <Rocket className="h-4 w-4 text-primary" />
              </div>
              <div>
                <h3 className="font-medium">App behavior</h3>
                <p className="text-xs text-muted-foreground">Startup and update defaults</p>
              </div>
            </div>
            <FieldGroup className="gap-4">
              <Field orientation="responsive" className="items-center gap-4">
                <FieldContent>
                  <FieldTitle>Launch at Startup</FieldTitle>
                  <FieldDescription>Start VoiceTypr automatically after login.</FieldDescription>
                </FieldContent>
                <div className="flex items-center justify-end gap-2">
                  {autostartLoading && <Spinner className="h-4 w-4 text-muted-foreground" />}
                  <Switch
                    id="autostart"
                    checked={autostartEnabled}
                    onCheckedChange={handleAutostartToggle}
                    disabled={autostartLoading}
                  />
                </div>
              </Field>

              <Field orientation="responsive" className="items-center gap-4">
                <FieldContent>
                  <FieldTitle>Check for updates automatically</FieldTitle>
                  <FieldDescription>
                    Check daily and ask before downloading or installing anything.
                  </FieldDescription>
                </FieldContent>
                <div className="flex flex-col items-end gap-2">
                  <Switch
                    id="check-updates-automatically"
                    checked={settings.check_updates_automatically ?? true}
                    onCheckedChange={async (checked) =>
                      await updateSettings({
                        check_updates_automatically: checked,
                      })
                    }
                  />
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={handleCheckUpdate}
                    disabled={isCheckingUpdate}
                  >
                    <RefreshCw className={`h-3.5 w-3.5 ${isCheckingUpdate ? "animate-spin" : ""}`} />
                    {isCheckingUpdate ? "Checking" : "Check updates"}
                  </Button>
                </div>
              </Field>
            </FieldGroup>
          </div>

          <div className="rounded-xl border border-border/60 bg-card">
            <div className="border-b border-border/60 px-4 py-3">
              <div className="flex items-center gap-2">
                <div className="rounded-md bg-primary/10 p-1.5">
                  <Mic className="h-4 w-4 text-primary" />
                </div>
                <div>
                  <h3 className="font-medium">Recording</h3>
                  <p className="text-xs text-muted-foreground">Capture, hotkeys, and transcript handling</p>
                </div>
              </div>
            </div>

            <div className="p-4">
              <FieldGroup className="gap-7">
                <FieldSet className="gap-4 border-t border-border/60 pt-5 first:border-t-0 first:pt-0">
                  <FieldLegend className="mb-1 text-base font-semibold">Capture controls</FieldLegend>

                  <Field orientation="responsive" className="items-start gap-3">
                    <FieldContent>
                      <FieldTitle>Recording Hotkey</FieldTitle>
                      <FieldDescription>
                        {isEditingHotkey
                          ? "Press a key or modifier, then save."
                          : formatPrimaryHotkeyLabel(nativeBinding, settings.hotkey)}
                      </FieldDescription>
                    </FieldContent>
                    <div className="w-full md:w-auto">
                      {isEditingHotkey ? (
                        <div className="space-y-3">
                          <HotkeyInput
                            inline
                            value={pendingHotkey}
                            onChange={(v) => {
                              setPendingHotkey(v);
                              setPendingBareModifier(null);
                            }}
                            allowBareModifier
                            onBareModifier={(spec) => {
                              setPendingBareModifier(spec);
                              setPendingHotkey("");
                            }}
                            placeholder="Press a key..."
                          />
                          {pendingBareModifier && (
                            <label className="flex cursor-pointer items-center gap-2 text-sm select-none">
                              <Switch
                                checked={holdToTalk}
                                onCheckedChange={setHoldToTalk}
                                id="hold-to-talk"
                              />
                              <span>Hold to talk (push-to-talk)</span>
                            </label>
                          )}
                          <div className="flex gap-2">
                            <Button
                              type="button"
                              size="sm"
                              onClick={handleSaveHotkey}
                              disabled={!pendingHotkey && !pendingBareModifier}
                            >
                              <Check className="h-3.5 w-3.5" />
                              Save
                            </Button>
                            <Button
                              type="button"
                              size="sm"
                              variant="outline"
                              onClick={handleCancelHotkey}
                            >
                              <X className="h-3.5 w-3.5" />
                              Cancel
                            </Button>
                          </div>
                        </div>
                      ) : (
                        <div className="flex items-center gap-2">
                          <div className="flex min-h-9 items-center rounded-md border border-input bg-muted/30 px-3 text-sm">
                            {formatPrimaryHotkeyLabel(nativeBinding, settings.hotkey)}
                          </div>
                          <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={startEditing}
                          >
                            <Edit2 className="h-3.5 w-3.5" />
                            Edit
                          </Button>
                        </div>
                      )}
                    </div>
                  </Field>

                  <p className="text-xs text-muted-foreground">
                    Primary recording shortcut. Additional per-action shortcuts live in{" "}
                    <span className="font-medium text-foreground">Settings &#8594; Shortcuts</span>.
                  </p>

                  {!canAutoInsert && showAccessibilityWarning && (
                    <div className="rounded-lg border border-amber-500/25 bg-amber-500/10 p-3">
                      <div className="flex items-start gap-2">
                        <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-amber-600 dark:text-amber-500" />
                        <div className="space-y-2">
                          <div>
                            <p className="text-sm font-medium text-amber-900 dark:text-amber-400">
                              Accessibility permission required
                            </p>
                            <p className="text-xs text-amber-800 dark:text-amber-500">
                              VoiceTypr needs accessibility permission for global hotkeys and auto-insert.
                            </p>
                          </div>
                          <ol className="list-decimal space-y-0.5 pl-4 text-xs text-amber-800 dark:text-amber-500">
                            <li>Open System Settings</li>
                            <li>Go to Privacy &amp; Security → Accessibility</li>
                            <li>Add VoiceTypr and enable it</li>
                          </ol>
                          <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            className="h-auto px-0 text-amber-700 hover:bg-transparent hover:text-amber-800 dark:text-amber-400 dark:hover:text-amber-300"
                            onClick={async () => {
                              try {
                                await invoke("open_accessibility_settings");
                              } catch (error) {
                                log.error("Failed to open accessibility settings:", error);
                                toast.error(
                                  "Could not open settings. Please open System Settings manually.",
                                );
                              }
                            }}
                          >
                            Open Accessibility Settings
                          </Button>
                        </div>
                      </div>
                    </div>
                  )}

                  <Field orientation="responsive" className="items-center gap-3">
                    <FieldContent>
                      <FieldTitle>Microphone</FieldTitle>
                      <FieldDescription>Select your preferred audio input device.</FieldDescription>
                    </FieldContent>
                    <div className="w-full md:w-auto">
                      <MicrophoneSelection
                        value={settings.selected_microphone || undefined}
                        onValueChange={async (deviceName) => {
                          try {
                            await invoke("set_audio_device", {
                              deviceName: deviceName || null,
                            });
                            toast.success(`Microphone changed to: ${deviceName || "Default"}`);
                          } catch (error) {
                            log.error("Failed to set microphone:", error);
                            toast.error("Failed to change microphone");
                          }
                        }}
                      />
                    </div>
                  </Field>
                </FieldSet>

                <FieldSet className="gap-4 border-t border-border/60 pt-5">
                  <FieldLegend className="mb-1 text-base font-semibold">Transcript handling</FieldLegend>

                  <Field orientation="responsive" className="items-center gap-3">
                    <FieldContent>
                      <FieldTitle>Keep Transcript in Clipboard</FieldTitle>
                      <FieldDescription>
                        Leave transcribed text available for manual pastes
                      </FieldDescription>
                    </FieldContent>
                    <Switch
                      id="clipboard-retain"
                      checked={settings.keep_transcription_in_clipboard ?? false}
                      onCheckedChange={async (checked) =>
                        await updateSettings({
                          keep_transcription_in_clipboard: checked,
                        })
                      }
                    />
                  </Field>

                  <Field orientation="responsive" className="items-center gap-3">
                    <FieldContent>
                      <FieldTitle>Auto-paste transcript</FieldTitle>
                      <FieldDescription>
                        Insert completed text automatically. Turn off to copy for manual paste.
                      </FieldDescription>
                    </FieldContent>
                    <Switch
                      id="auto-paste-transcription"
                      checked={settings.auto_paste_transcription ?? true}
                      onCheckedChange={async (checked) =>
                        await updateSettings({
                          auto_paste_transcription: checked,
                        })
                      }
                    />
                  </Field>


                  <Field orientation="responsive" className="items-center gap-3">
                    <FieldContent>
                      <FieldTitle>Pause media during recording</FieldTitle>
                      <FieldDescription>
                        Automatically pause playing music or videos while recording.
                      </FieldDescription>
                    </FieldContent>
                    <Switch
                      id="pause-media"
                      checked={settings.pause_media_during_recording ?? false}
                      onCheckedChange={async (checked) =>
                        await updateSettings({
                          pause_media_during_recording: checked,
                        })
                      }
                    />
                  </Field>

                  <Field orientation="responsive" className="items-center gap-3">
                    <FieldContent>
                      <FieldTitle>Sound on Recording</FieldTitle>
                      <FieldDescription>Play a sound when recording starts</FieldDescription>
                    </FieldContent>
                    <Switch
                      id="sound-on-recording"
                      checked={settings.play_sound_on_recording ?? true}
                      onCheckedChange={async (checked) =>
                        await updateSettings({
                          play_sound_on_recording: checked,
                        })
                      }
                    />
                  </Field>

                  <Field orientation="responsive" className="items-center gap-3">
                    <FieldContent>
                      <FieldTitle>Sound on Recording End</FieldTitle>
                      <FieldDescription>Play a sound when recording stops</FieldDescription>
                    </FieldContent>
                    <Switch
                      id="sound-on-recording-end"
                      checked={settings.play_sound_on_recording_end ?? true}
                      onCheckedChange={async (checked) =>
                        await updateSettings({
                          play_sound_on_recording_end: checked,
                        })
                      }
                    />
                  </Field>
                </FieldSet>

                {isWindows && (
                  <FieldSet className="gap-4 border-t border-border/60 pt-5">
                    <FieldLegend className="mb-1 text-base font-semibold">Transcription performance</FieldLegend>

                    <Field orientation="responsive" className="items-center gap-3">
                      <FieldContent>
                        <FieldTitle>Acceleration</FieldTitle>
                        <FieldDescription>
                          {(settings.transcription_acceleration ?? 'auto') === 'auto'
                            ? 'Use GPU when available, fall back to CPU (recommended)'
                            : (settings.transcription_acceleration ?? 'auto') === 'gpu'
                              ? 'Always use the GPU'
                              : 'Always use the CPU'}
                        </FieldDescription>
                      </FieldContent>
                      <Select
                        value={settings.transcription_acceleration ?? 'auto'}
                        onValueChange={async (value: TranscriptionAcceleration) => {
                          await updateSettings({
                            transcription_acceleration: value,
                          });
                        }}
                      >
                        <SelectTrigger className="w-full md:w-[190px]">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="auto">Auto</SelectItem>
                          <SelectItem value="gpu">GPU</SelectItem>
                          <SelectItem value="cpu">CPU</SelectItem>
                        </SelectContent>
                      </Select>
                    </Field>
                  </FieldSet>
                )}

                <FieldSet className="gap-4 border-t border-border/60 pt-5">
                  <FieldLegend className="mb-1 text-base font-semibold">Recording indicator</FieldLegend>

                  <Field orientation="responsive" className="items-center gap-3">
                    <FieldContent>
                      <FieldTitle>Indicator visibility</FieldTitle>
                      <FieldDescription>Show or hide the small recording status overlay.</FieldDescription>
                    </FieldContent>
                    <Select
                      value={settings.pill_indicator_mode ?? "when_recording"}
                      onValueChange={async (value: PillIndicatorMode) => {
                        await updateSettings({
                          pill_indicator_mode: value,
                        });
                      }}
                    >
                      <SelectTrigger className="w-full md:w-[190px]">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="never">Never</SelectItem>
                        <SelectItem value="always">Always</SelectItem>
                        <SelectItem value="when_recording">When Recording</SelectItem>
                      </SelectContent>
                    </Select>
                  </Field>

                  {settings.pill_indicator_mode !== "never" && (
                    <>
                      <Field orientation="responsive" className="items-center gap-3">
                        <FieldContent>
                          <FieldTitle>Indicator Position</FieldTitle>
                          <FieldDescription>
                            Choose where the status overlay appears on screen.
                          </FieldDescription>
                        </FieldContent>
                        <Select
                          value={settings.pill_indicator_position ?? "bottom-center"}
                          onValueChange={async (value: PillIndicatorPosition) => {
                            await updateSettings({
                              pill_indicator_position: value,
                            });
                          }}
                        >
                          <SelectTrigger className="w-full md:w-[190px]">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="top-left">Top Left</SelectItem>
                            <SelectItem value="top-center">Top Center</SelectItem>
                            <SelectItem value="top-right">Top Right</SelectItem>
                            <SelectItem value="bottom-left">Bottom Left</SelectItem>
                            <SelectItem value="bottom-center">Bottom Center</SelectItem>
                            <SelectItem value="bottom-right">Bottom Right</SelectItem>
                          </SelectContent>
                        </Select>
                      </Field>

                      <Field orientation="responsive" className="items-center gap-3">
                        <FieldContent>
                          <FieldTitle>Edge offset</FieldTitle>
                          <FieldDescription>
                            Distance from screen edge.
                          </FieldDescription>
                        </FieldContent>
                        <div className="w-full min-w-0 md:flex-1">
                          <div className="flex items-center gap-3">
                            <Slider
                              aria-label="Indicator edge offset"
                              min={10}
                              max={50}
                              step={5}
                              value={[settings.pill_indicator_offset ?? 10]}
                              onValueChange={async ([offset]) =>
                                await updateSettings({
                                  pill_indicator_offset: offset,
                                })
                              }
                              className="w-full"
                            />
                            <div className="min-w-12 rounded-md border bg-muted/60 px-2 py-1 text-center text-[11px] font-medium text-foreground tabular-nums">
                              {settings.pill_indicator_offset ?? 10}px
                            </div>
                          </div>
                        </div>
                      </Field>
                    </>
                  )}
                </FieldSet>

                <FieldSet className="gap-4 border-t border-border/60 pt-5">
                  <FieldLegend className="mb-1 text-base font-semibold">Storage & cleanup</FieldLegend>

                  <Field orientation="responsive" className="items-center gap-3">
                    <FieldContent>
                      <FieldTitle>Transcript history cleanup</FieldTitle>
                      <FieldDescription>
                        Automatically remove old transcript history after a set number of days.
                      </FieldDescription>
                    </FieldContent>
                    <Select
                      value={
                        settings.transcription_cleanup_days == null
                          ? "forever"
                          : String(settings.transcription_cleanup_days)
                      }
                      onValueChange={async (value) =>
                        await updateSettings({
                          transcription_cleanup_days:
                            value === "forever" ? null : parseInt(value, 10),
                        })
                      }
                    >
                      <SelectTrigger className="w-full md:w-[190px]">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="forever">Keep forever</SelectItem>
                        <SelectItem value="7">7 days</SelectItem>
                        <SelectItem value="14">14 days</SelectItem>
                        <SelectItem value="30">30 days</SelectItem>
                        <SelectItem value="90">90 days</SelectItem>
                      </SelectContent>
                    </Select>
                  </Field>

                  <Field
                    orientation="responsive"
                    className="items-start gap-3 md:[&_[data-slot=field-content]]:pt-1"
                  >
                    <FieldContent>
                      <FieldTitle>Save recording audio</FieldTitle>
                      <FieldDescription>
                        Keeps the original audio for re-transcription — including retrying a failed transcription from History — then automatically deletes it after your chosen period. With this off, failed recordings can't be retried.
                      </FieldDescription>
                    </FieldContent>
                    <div className="w-full md:w-auto">
                      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-end">
                        <Select
                          value={
                            !settings.save_recordings
                              ? "off"
                              : settings.recording_retention_days === null
                                ? "forever"
                                : String(settings.recording_retention_days ?? 30)
                          }
                          onValueChange={async (value) => {
                            if (value === "off") {
                              await updateSettings({
                                save_recordings: false,
                              });
                              return;
                            }

                            const days = value === "forever" ? null : parseInt(value, 10);
                            await updateSettings({
                              save_recordings: true,
                              recording_retention_days: days,
                            });
                            toast.success("Recording audio will now be saved");
                          }}
                        >
                          <SelectTrigger className="w-full sm:w-[190px]">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="off">Don&apos;t save</SelectItem>
                            <SelectItem value="7">Keep for 7 days</SelectItem>
                            <SelectItem value="30">Keep for 30 days</SelectItem>
                            <SelectItem value="90">Keep for 90 days</SelectItem>
                            <SelectItem value="forever">Keep forever</SelectItem>
                          </SelectContent>
                        </Select>

                        {settings.save_recordings && (
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="justify-center"
                            onClick={async () => {
                              try {
                                await invoke("open_recordings_folder");
                              } catch (error) {
                                log.error("Failed to open recordings folder:", error);
                                toast.error("Failed to open recordings folder");
                              }
                            }}
                          >
                            <FolderOpen className="h-4 w-4" />
                            Open folder
                          </Button>
                        )}
                      </div>
                    </div>
                  </Field>
                </FieldSet>

                <div className="flex items-center gap-2 rounded-lg border border-primary/15 bg-primary/5 p-3">
                  <Kbd>ESC</Kbd>
                  <p className="text-xs text-muted-foreground">
                    Press twice while recording to cancel the current take.
                  </p>
                </div>
              </FieldGroup>
            </div>
          </div>

          <NetworkSharingCard />

        </div>
      </ScrollArea>
    </div>
  );
}
