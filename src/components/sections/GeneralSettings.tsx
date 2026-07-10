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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { useCanAutoInsert } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { updateService } from "@/services/updateService";
import { isMacOS, isWindows } from "@/lib/platform";
import { findActivePrimaryBinding, formatPrimaryHotkeyLabel } from "@/lib/shortcut-display";
import { PillIndicatorMode, PillIndicatorPosition, TranscriptionAcceleration, type UpdateChannel } from "@/types";
import { invoke } from "@tauri-apps/api/core";
import type { ShortcutBinding, ShortcutSettings } from "@/types/shortcuts";
import type { AccelerationStatus } from "@/types/acceleration";
import { isStoreDistribution, type DistributionInfo } from "@/types/distribution";
import { AlertCircle, Check, Edit2, FolderOpen, HelpCircle, Mic, RefreshCw, Rocket, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { MicrophoneSelection } from "../MicrophoneSelection";
import { createLogger } from "@/lib/logger";

const log = createLogger("settings");

function isAccelerationStatus(value: unknown): value is AccelerationStatus {
  if (!value || typeof value !== "object") {
    return false;
  }

  const status = value as Record<string, unknown>;
  return (
    typeof status.message === "string" &&
    typeof status.effective_backend === "string" &&
    typeof status.diagnostic_code === "string" &&
    typeof status.recommended_action === "string"
  );
}

function isMetalOnUnsupportedPlatformStatus(
  status: AccelerationStatus,
): boolean {
  return (
    status.diagnostic_code === "unsupported_platform" &&
    status.effective_backend === "metal"
  );
}

function getRecommendedActionDescription(action: string): string | undefined {
  switch (action) {
    case "download_model":
      return "Download or select a local Whisper model, then retry Test GPU.";
    case "update_graphics_driver":
      return "Update or install your graphics driver, then retry Test GPU.";
    case "use_cpu":
      return "Use CPU mode for now, or retry GPU after updating your graphics driver.";
    case "report_bug":
      return "Report this with logs so we can inspect the packaged GPU helper.";
    default:
      return undefined;
  }
}

function getAccelerationGuidance(status: AccelerationStatus | null): string {
  if (!status) {
    return "Voicetypr will test GPU acceleration when needed and keep CPU transcription available.";
  }

  if (isMetalOnUnsupportedPlatformStatus(status)) {
    return "Metal acceleration is active on this Mac.";
  }

  switch (status.diagnostic_code) {
    case "ready":
      return "GPU acceleration is ready.";
    case "unsupported_platform":
      return "GPU acceleration is not available on this platform. Voicetypr will keep using CPU transcription safely.";
    case "vulkan_loader_missing":
    case "vulkan_device_missing":
    case "driver_or_runtime_failed":
      return "GPU acceleration needs a Vulkan-capable NVIDIA, AMD, or Intel graphics driver. Update or install your graphics driver, then retry Test GPU. Voicetypr will keep using CPU transcription safely until GPU acceleration is available.";
    case "sidecar_missing":
    case "sidecar_protocol_error":
      return "Voicetypr has a package or runtime issue. Please report this with logs. Voicetypr will keep using CPU transcription safely.";
    case "sidecar_timeout":
      return "The Vulkan helper did not respond in time. Voicetypr will keep using CPU transcription safely; retry Test GPU after updating your graphics driver.";
    case "model_missing":
      return "Download or select a local Whisper model before testing GPU acceleration. Voicetypr will keep using CPU transcription safely.";
    default:
      return (
        getRecommendedActionDescription(status.recommended_action) ||
        "Voicetypr will keep using CPU transcription safely."
      );
  }
}

function getAccelerationToastDescription(status: AccelerationStatus): string | undefined {
  return (
    getRecommendedActionDescription(status.recommended_action) ||
    status.last_error ||
    undefined
  );
}


export function GeneralSettings() {
  const { settings, updateSettings } = useSettings();
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [autostartLoading, setAutostartLoading] = useState(false);
  const [showAccessibilityWarning, setShowAccessibilityWarning] = useState(true);
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);
  const [updateDistribution, setUpdateDistribution] =
    useState<"loading" | "direct" | "store" | "error">("loading");
  const [isChangingUpdateChannel, setIsChangingUpdateChannel] = useState(false);
  const canAutoInsert = useCanAutoInsert();
  const [nativeBinding, setNativeBinding] = useState<ShortcutBinding | null>(null);
  const [isEditingHotkey, setIsEditingHotkey] = useState(false);
  const [pendingHotkey, setPendingHotkey] = useState("");
  const [pendingBareModifier, setPendingBareModifier] = useState<BareModifierSpec | null>(null);
  const [holdToTalk, setHoldToTalk] = useState(false);
  const [accelerationStatus, setAccelerationStatus] =
    useState<AccelerationStatus | null>(null);
  const [testingAcceleration, setTestingAcceleration] = useState(false);

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
    invoke<DistributionInfo>("get_distribution_info")
      .then((info) =>
        setUpdateDistribution(isStoreDistribution(info) ? "store" : "direct"),
      )
      .catch((error) => {
        log.error("Failed to check update distribution:", error);
        setUpdateDistribution("error");
      });
  }, []);

  const loadAccelerationStatus = useCallback(async () => {
    try {
      const status = await invoke<AccelerationStatus>(
        "get_transcription_acceleration_status",
      );
      if (isAccelerationStatus(status)) {
        setAccelerationStatus(status);
      }
    } catch (error) {
      log.error("Failed to check acceleration status:", error);
    }
  }, []);

  useEffect(() => {
    void loadAccelerationStatus();
  }, [loadAccelerationStatus]);

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
        setNativeBinding(findActivePrimaryBinding(result.bindings));
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
        // Clear the combo hotkey BEFORE saving the bare-modifier binding:
        // update_shortcut_settings rebuilds the engine from settings.hotkey, and
        // if the combo is still set the migration treats it as authoritative and
        // disables the bare-modifier primary we just created (onboarding clears
        // first for this exact reason).
        if (settings.hotkey) {
          await updateSettings({ hotkey: "" });
        }
        await invoke("update_shortcut_settings", {
          settings: { bindings: updatedBindings },
        });
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
        // Replacing a bare-modifier primary with a combo: disable the existing
        // native primary binding so only the combo fires. Otherwise both the
        // native trigger and the new combo global shortcut stay active at once.
        const existing = await invoke<ShortcutSettings>("get_shortcut_settings");
        const primary = findActivePrimaryBinding(existing.bindings);
        if (primary) {
          const updatedBindings = existing.bindings.map((b) =>
            b.id === primary.id ? { ...b, enabled: false } : b,
          );
          await invoke("update_shortcut_settings", {
            settings: { bindings: updatedBindings },
          });
        }
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

  const handleUpdateChannelChange = async (value: string) => {
    if (value !== "stable" && value !== "beta") {
      toast.error("Invalid update channel");
      return;
    }

    setIsChangingUpdateChannel(true);
    try {
      await updateSettings({ update_channel: value });
      const channel: UpdateChannel = value;
      toast.success(
        channel === "beta" ? "Beta updates enabled" : "Stable updates enabled",
        {
          description:
            channel === "beta"
              ? "Checking for the latest beta. Beta builds may be less stable."
              : "Future checks use stable releases. Switching does not downgrade an installed beta.",
        },
      );
      await handleCheckUpdate();
    } catch (error) {
      log.error("Failed to change update channel:", error);
      toast.error("Failed to change update channel");
    } finally {
      setIsChangingUpdateChannel(false);
    }
  };

  const handleAccelerationChange = async (value: TranscriptionAcceleration) => {
    await updateSettings({ transcription_acceleration: value });
    await loadAccelerationStatus();
    toast.success(
      value === "auto"
        ? "Acceleration set to Auto"
        : value === "gpu"
          ? "GPU acceleration preferred"
          : "CPU-only transcription enabled",
    );
  };

  const handleTestAcceleration = async () => {
    setTestingAcceleration(true);
    try {
      const status = await invoke<AccelerationStatus>(
        "test_transcription_acceleration",
      );
      if (isAccelerationStatus(status)) {
        setAccelerationStatus(status);
        if (status.gpu_available === true) {
          toast.success(status.message || "GPU acceleration is available");
        } else if (status.gpu_available === false) {
          toast.warning(status.message, {
            description: getAccelerationToastDescription(status),
          });
        } else {
          toast.info(status.message);
        }
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast.error("GPU acceleration test failed", { description: message });
      await loadAccelerationStatus();
    } finally {
      setTestingAcceleration(false);
    }
  };

  return (
    <div className="h-full min-h-0 overflow-auto">
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-3.5 px-6 py-7 md:px-8">
        <div className="mb-1 flex flex-wrap items-start gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
              <Dialog>
                <DialogTrigger asChild>
                  <Button type="button" variant="ghost" size="icon-sm" aria-label="Settings guide" className="size-7 rounded-full text-muted-foreground">
                    <HelpCircle className="h-4 w-4" />
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
            <p className="mt-0.5 text-sm text-muted-foreground">
              Configure capture controls, transcript behavior, and startup defaults.
            </p>
          </div>
        </div>
          <div className="rounded-2xl border border-border bg-card p-4">
            <div className="mb-4 flex items-center gap-2">
              <div className="rounded-md bg-sage-bg p-1.5">
                <Rocket className="h-4 w-4 text-sage" />
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
                  <FieldDescription>Start Voicetypr automatically after login.</FieldDescription>
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

              {updateDistribution === "store" ? (
                <Field>
                  <FieldContent>
                    <FieldTitle>Updates managed by Microsoft Store</FieldTitle>
                    <FieldDescription>
                      Update channels and installation are controlled by Microsoft Store.
                    </FieldDescription>
                  </FieldContent>
                </Field>
              ) : updateDistribution === "direct" ? (
                <>
                  <Field orientation="responsive" className="items-center gap-4">
                    <FieldContent>
                      <FieldTitle>Update channel</FieldTitle>
                      <FieldDescription>
                        Beta gets early builds for testing. Switching to Stable changes future checks and does not downgrade an installed beta.
                      </FieldDescription>
                    </FieldContent>
                    <Select
                      value={settings.update_channel ?? "stable"}
                      onValueChange={(value) => void handleUpdateChannelChange(value)}
                      disabled={isChangingUpdateChannel || isCheckingUpdate}
                    >
                      <SelectTrigger className="w-full md:w-[190px]" aria-label="Update channel">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="stable">Stable</SelectItem>
                        <SelectItem value="beta">Beta</SelectItem>
                      </SelectContent>
                    </Select>
                  </Field>

                  <Field orientation="responsive" className="items-center gap-4">
                    <FieldContent>
                      <FieldTitle>Check for updates automatically</FieldTitle>
                      <FieldDescription>
                        Check the selected channel daily and ask before downloading or installing anything.
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
                </>
              ) : (
                <Field>
                  <FieldContent>
                    <FieldTitle>
                      {updateDistribution === "loading"
                        ? "Loading update options"
                        : "Update options unavailable"}
                    </FieldTitle>
                    <FieldDescription>
                      {updateDistribution === "loading"
                        ? "Checking how this installation receives updates."
                        : "Could not verify this installation type. Restart Voicetypr to try again."}
                    </FieldDescription>
                  </FieldContent>
                </Field>
              )}
            </FieldGroup>
          </div>

          <div className="rounded-2xl border border-border bg-card">
            <div className="border-b border-border/60 px-4 py-3">
              <div className="flex items-center gap-2">
                <div className="rounded-md bg-sage-bg p-1.5">
                  <Mic className="h-4 w-4 text-sage" />
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
                              Voicetypr needs accessibility permission for global hotkeys and auto-insert.
                            </p>
                          </div>
                          <ol className="list-decimal space-y-0.5 pl-4 text-xs text-amber-800 dark:text-amber-500">
                            <li>Open System Settings</li>
                            <li>Go to Privacy &amp; Security → Accessibility</li>
                            <li>Add Voicetypr and enable it</li>
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
                      onValueChange={(value: TranscriptionAcceleration) => {
                        void handleAccelerationChange(value);
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

                  <div className="flex items-start justify-between gap-4 rounded-lg bg-muted/40 p-3">
                    <div className="space-y-1">
                      <p className="text-sm font-medium">
                        {accelerationStatus?.effective_backend === "vulkan"
                          ? "GPU acceleration ready"
                          : accelerationStatus?.effective_backend === "metal"
                            ? "Using Metal acceleration"
                            : accelerationStatus?.effective_backend === "cpu"
                              ? "Using CPU mode"
                              : "Acceleration status"}
                      </p>
                      <p className="text-xs text-muted-foreground">
                        {accelerationStatus?.message ??
                          "Voicetypr will test GPU acceleration when needed."}
                      </p>
                      {accelerationStatus?.diagnostic_code !== "ready" && (
                        <p className="text-xs text-muted-foreground">
                          {getAccelerationGuidance(accelerationStatus)}
                        </p>
                      )}
                      {accelerationStatus?.last_error && (
                        <p className="text-xs text-amber-600 dark:text-amber-500">
                          {accelerationStatus.last_error}
                        </p>
                      )}
                    </div>
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={handleTestAcceleration}
                      disabled={testingAcceleration}
                    >
                      {testingAcceleration ? "Checking..." : isWindows ? "Test GPU" : "Check Status"}
                    </Button>
                  </div>
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
        </div>
    </div>
  );
}
