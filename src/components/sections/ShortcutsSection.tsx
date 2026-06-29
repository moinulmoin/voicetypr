import { BareModifierSpec, HotkeyInput } from "@/components/HotkeyInput";
import { Button } from "@/components/ui/button";
import { SettingsCard, SettingsHeader, SettingsPage } from "@/components/settings/settings-ui";
import { Spinner } from "@/components/ui/spinner";
import { normalizeShortcutKeys, ValidationPresets } from "@/lib/keyboard-normalizer";
import type {
  ModifierKind,
  ModifierSide,
  ShortcutAction,
  ShortcutActionDefinition,
  ShortcutBinding,
  ShortcutSettings,
} from "@/types/shortcuts";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, Check, Keyboard, Pencil, Plus, Trash2, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { createLogger } from "@/lib/logger";

const log = createLogger("shortcuts");

const emptySettings: ShortcutSettings = { bindings: [] };

const singleKeyValidation = ValidationPresets.custom({
  minKeys: 1,
  requireModifier: false,
  requireModifierForMultiKey: true,
});

const MAX_SINGLE_KEY_BINDINGS = 5;

function isSingleKeyShortcut(shortcut: string): boolean {
  if (!shortcut) return false;
  const normalized = normalizeShortcutKeys(shortcut);
  const parts = normalized.split("+").filter(Boolean);
  if (parts.length !== 1) return false;
  const modifiers = ["CommandOrControl", "Super", "Shift", "Alt", "Control", "Command", "Cmd", "Ctrl", "Option", "Meta"];
  return !modifiers.includes(parts[0]);
}

function createBinding(action: ShortcutActionDefinition): ShortcutBinding {
  return {
    id: typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID()
      : `${action.action}-${Date.now()}-${Math.random().toString(36).slice(2)}`,
    action: action.action,
    shortcut: "",
    trigger: action.recommended_trigger,
    enabled: true,
    allow_risky_combo: false,
  };
}

function normalizeSettings(value: ShortcutSettings | null | undefined): ShortcutSettings {
  return {
    bindings: Array.isArray(value?.bindings) ? value.bindings : [],
  };
}

function formatError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function shortcutComparisonKey(shortcut: string) {
  return normalizeShortcutKeys(shortcut).toLowerCase();
}

const MOD_LABELS: Record<string, string> = {
  alt: "Option",
  meta: "Command",
  control: "Control",
  shift: "Shift",
};

// The primary recording trigger (toggle/hold) is configured in General Settings,
// not here. Exclude these actions from the custom-shortcuts list so the recording
// hotkey isn't shown/edited in two places and a duplicate recording binding can't
// be created from this page.
const PRIMARY_RECORDING_ACTIONS: Record<string, true> = {
  toggle_recording: true,
  hold_to_record: true,
};

function formatBindingDisplay(binding: ShortcutBinding): string {
  const kind = binding.trigger_kind ?? "combo";
  const mod = binding.modifier;
  if (mod && (kind === "modifier_hold" || kind === "isolated_tap")) {
    const sideLabel = mod.side === "right" ? "Right " : mod.side === "left" ? "Left " : "";
    const modLabel = MOD_LABELS[mod.modifier] ?? mod.modifier;
    const verbLabel = kind === "modifier_hold" ? "Hold" : "Tap";
    return `${verbLabel} ${sideLabel}${modLabel}`;
  }
  return binding.shortcut || "No shortcut configured";
}

type EditingCapture = {
  bindingId: string;
  /** Captured combo shortcut string (mutually exclusive with bareModifier). */
  combo: string;
  /** Captured bare modifier (mutually exclusive with combo). */
  bareModifier: BareModifierSpec | null;
  /** Mirrors allow_risky_combo for the draft being captured. */
  allowRiskyCombo: boolean;
};

export function ShortcutsSection() {
  const [actions, setActions] = useState<ShortcutActionDefinition[]>([]);
  const [settings, setSettings] = useState<ShortcutSettings>(emptySettings);
  const [draftBindings, setDraftBindings] = useState<ShortcutBinding[]>([]);
  const [loading, setLoading] = useState(true);
  const [actionLoadError, setActionLoadError] = useState<string | null>(null);
  const [settingsLoadError, setSettingsLoadError] = useState<string | null>(null);
  const [savingBindingId, setSavingBindingId] = useState<string | null>(null);
  const savingBindingIdRef = useRef<string | null>(null);
  const [editingCapture, setEditingCapture] = useState<EditingCapture | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadShortcuts() {
      setLoading(true);
      try {
        const [actionResult, settingsResult] = await Promise.allSettled([
          invoke<ShortcutActionDefinition[]>("list_shortcut_actions"),
          invoke<ShortcutSettings>("get_shortcut_settings"),
        ]);

        if (cancelled) return;

        let nextActionLoadError: string | null = null;
        let nextSettingsLoadError: string | null = null;

        if (actionResult.status === "fulfilled") {
          setActions(actionResult.value);
        } else {
          log.error("Failed to load shortcut actions:", actionResult.reason);
          setActions([]);
          nextActionLoadError = formatError(actionResult.reason);
        }

        if (settingsResult.status === "fulfilled") {
          setSettings(normalizeSettings(settingsResult.value));
          nextSettingsLoadError = null;
        } else {
          log.error("Failed to load shortcut settings:", settingsResult.reason);
          setSettings(emptySettings);
          nextSettingsLoadError = formatError(settingsResult.reason);
        }

        setActionLoadError(nextActionLoadError);
        setSettingsLoadError(nextSettingsLoadError);
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    loadShortcuts();

    return () => {
      cancelled = true;
    };
  }, []);

  const groupedActions = useMemo(() => {
    const groups = new Map<string, ShortcutActionDefinition[]>();

    for (const action of actions) {
      if (PRIMARY_RECORDING_ACTIONS[action.action]) {
        continue; // primary recording hotkey is managed in General Settings
      }
      const existing = groups.get(action.section);
      if (existing) {
        existing.push(action);
      } else {
        groups.set(action.section, [action]);
      }
    }

    return Array.from(groups.entries());
  }, [actions]);

  const bindingsByAction = useMemo(() => {
    const groups = new Map<ShortcutAction, ShortcutBinding[]>();

    for (const binding of [...settings.bindings, ...draftBindings]) {
      const existing = groups.get(binding.action);
      if (existing) {
        existing.push(binding);
      } else {
        groups.set(binding.action, [binding]);
      }
    }

    return groups;
  }, [draftBindings, settings.bindings]);

  const actionLabels = useMemo(() => {
    return new Map(actions.map((action) => [action.action, action.label]));
  }, [actions]);

  const singleKeyCount = useMemo(() => {
    return [...settings.bindings, ...draftBindings].filter(
      (b) => b.enabled && b.shortcut && isSingleKeyShortcut(b.shortcut)
    ).length;
  }, [settings.bindings, draftBindings]);

  const isMutating = savingBindingId !== null;
  const editingDisabled = isMutating || settingsLoadError !== null;
  const isCapturing = editingCapture !== null;

  const beginMutation = useCallback((bindingId: string) => {
    if (savingBindingIdRef.current !== null || settingsLoadError !== null) {
      return false;
    }

    savingBindingIdRef.current = bindingId;
    setSavingBindingId(bindingId);
    return true;
  }, [settingsLoadError]);

  const endMutation = useCallback(() => {
    savingBindingIdRef.current = null;
    setSavingBindingId(null);
  }, []);

  const persistSettings = useCallback(async (nextSettings: ShortcutSettings, successMessage: string) => {
    const savedSettings = await invoke<ShortcutSettings>("update_shortcut_settings", { settings: nextSettings });
    setSettings(normalizeSettings(savedSettings));
    toast.success(successMessage);
  }, []);

  const updateBinding = useCallback(async (nextBinding: ShortcutBinding) => {
    const nextShortcut = nextBinding.shortcut.trim();
    if (nextShortcut) {
      const nextShortcutKey = shortcutComparisonKey(nextShortcut);
      const duplicateBinding = [...settings.bindings, ...draftBindings].find((binding) =>
        binding.id !== nextBinding.id
        && binding.enabled
        && binding.shortcut
        && shortcutComparisonKey(binding.shortcut) === nextShortcutKey,
      );

      if (duplicateBinding) {
        toast.error("Shortcut already assigned", {
          description: `${nextShortcut} is already assigned to ${actionLabels.get(duplicateBinding.action) ?? duplicateBinding.action}.`,
        });
        return;
      }
    }

    if (!beginMutation(nextBinding.id)) {
      return;
    }

    const isDraft = draftBindings.some((binding) => binding.id === nextBinding.id);
    const nextSettings = {
      bindings: isDraft
        ? [...settings.bindings, nextBinding]
        : settings.bindings.map((binding) =>
          binding.id === nextBinding.id ? nextBinding : binding,
        ),
    };

    try {
      await persistSettings(nextSettings, "Shortcut saved.");
      if (isDraft) {
        setDraftBindings((bindings) => bindings.filter((binding) => binding.id !== nextBinding.id));
      }
    } catch (error) {
      log.error("Failed to save shortcut:", error);
      toast.error("Could not save shortcut", {
        description: formatError(error),
      });
    } finally {
      endMutation();
    }
  }, [actionLabels, beginMutation, draftBindings, endMutation, persistSettings, settings.bindings]);

  const deleteBinding = useCallback(async (bindingId: string) => {
    if (editingDisabled || savingBindingIdRef.current !== null) {
      return;
    }

    if (draftBindings.some((binding) => binding.id === bindingId)) {
      setDraftBindings((bindings) => bindings.filter((binding) => binding.id !== bindingId));
      return;
    }

    if (!beginMutation(bindingId)) {
      return;
    }

    const nextSettings = {
      bindings: settings.bindings.filter((binding) => binding.id !== bindingId),
    };

    try {
      await persistSettings(nextSettings, "Shortcut removed.");
    } catch (error) {
      log.error("Failed to remove shortcut:", error);
      toast.error("Could not remove shortcut", { description: formatError(error) });
    } finally {
      endMutation();
    }
  }, [beginMutation, draftBindings, editingDisabled, endMutation, persistSettings, settings.bindings]);

  const startEditing = useCallback((binding: ShortcutBinding) => {
    if (editingDisabled || savingBindingIdRef.current !== null) return;
    setEditingCapture({
      bindingId: binding.id,
      combo: binding.shortcut || "",
      bareModifier: null,
      allowRiskyCombo: binding.allow_risky_combo,
    });
  }, [editingDisabled]);

  const cancelEdit = useCallback(() => {
    if (!editingCapture) return;
    const isDraft = draftBindings.some((b) => b.id === editingCapture.bindingId);
    if (isDraft) {
      setDraftBindings((bindings) => bindings.filter((b) => b.id !== editingCapture.bindingId));
    }
    setEditingCapture(null);
  }, [draftBindings, editingCapture]);

  const saveEdit = useCallback(async () => {
    if (!editingCapture) return;
    const { bindingId, combo, bareModifier, allowRiskyCombo } = editingCapture;

    const originalBinding =
      settings.bindings.find((b) => b.id === bindingId) ??
      draftBindings.find((b) => b.id === bindingId);

    if (!originalBinding) return;

    const recommendedTrigger =
      actions.find((a) => a.action === originalBinding.action)?.recommended_trigger ?? "pressed";

    let nextBinding: ShortcutBinding;

    if (bareModifier) {
      // Mode comes from the action row, not a separate toggle: Hold to Record =
      // push-to-talk (modifier_hold); Toggle Recording = tap (isolated_tap).
      const isPushToTalk = originalBinding.action === "hold_to_record";
      nextBinding = {
        ...originalBinding,
        trigger_kind: isPushToTalk ? "modifier_hold" : "isolated_tap",
        trigger: isPushToTalk ? "hold" : "pressed",
        modifier: {
          modifier: bareModifier.modifier as ModifierKind,
          side: bareModifier.side as ModifierSide,
        },
        shortcut: "",
        enabled: true,
        allow_risky_combo: allowRiskyCombo,
      };
    } else {
      // Combo (or normal key) → combo kind
      nextBinding = {
        ...originalBinding,
        trigger_kind: "combo",
        trigger: recommendedTrigger,
        modifier: null,
        shortcut: combo,
        enabled: !!combo,
        allow_risky_combo: isSingleKeyShortcut(combo),
      };
    }

    setEditingCapture(null);
    await updateBinding(nextBinding);
  }, [actions, draftBindings, editingCapture, settings.bindings, updateBinding]);

  const addDraftBinding = useCallback((action: ShortcutActionDefinition) => {
    if (editingDisabled || savingBindingIdRef.current !== null || isCapturing) {
      return;
    }

    const newBinding = createBinding(action);
    setDraftBindings((bindings) => [...bindings, newBinding]);
    setEditingCapture({
      bindingId: newBinding.id,
      combo: "",
      bareModifier: null,
      allowRiskyCombo: false,
    });
  }, [editingDisabled, isCapturing]);

  return (
    <SettingsPage>
      <SettingsHeader
        title="Shortcuts"
        description="Keyboard shortcuts for recording, history, formatting, and the dashboard."
      />

      <div className="rounded-2xl border border-border bg-muted/40 p-4 text-sm text-muted-foreground">
        <div className="flex gap-2">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-sage" />
          <p>
            Voicetypr tests global shortcuts and refuses combos already owned by macOS, Windows, or another app.
          </p>
        </div>
        <p className="mt-1 text-xs">
          {singleKeyCount} of {MAX_SINGLE_KEY_BINDINGS} single-key shortcuts used.
        </p>
      </div>

      {actionLoadError && (
        <div role="alert" className="rounded-2xl border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
          Shortcut actions could not be loaded: {actionLoadError}. You can still review saved shortcuts once the app reconnects.
        </div>
      )}

      {settingsLoadError && (
        <div role="alert" className="rounded-2xl border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
          Shortcut settings could not be loaded: {settingsLoadError}. Reload shortcut settings before editing; controls are read-only to avoid overwriting existing shortcuts.
        </div>
      )}

      {loading ? (
        <div className="flex items-center gap-2 rounded-2xl border border-border bg-card p-4 text-sm text-muted-foreground">
          <Spinner className="h-4 w-4" />
          Loading shortcuts…
        </div>
      ) : groupedActions.length === 0 ? (
        <div className="rounded-2xl border border-border bg-card p-6 text-sm text-muted-foreground">
          No shortcut actions are available.
        </div>
      ) : (
        groupedActions.map(([section, sectionActions]) => {
          const sectionBindingCount = sectionActions.reduce(
            (count, action) => count + (bindingsByAction.get(action.action)?.length ?? 0),
            0,
          );

          return (
            <SettingsCard
              key={section}
              icon={Keyboard}
              title={section}
              description={
                sectionBindingCount === 1 ? "1 binding configured" : `${sectionBindingCount} bindings configured`
              }
            >
              <div className="mt-4 divide-y divide-border">
                {sectionActions.map((action) => {
                      const bindings = bindingsByAction.get(action.action) ?? [];

                      return (
                        <div key={action.action} role="group" aria-label={action.label} className="space-y-3 py-4 first:pt-0 last:pb-0">
                          <div className="min-w-0">
                            <h3 className="text-[13.5px] font-semibold text-foreground">{action.label}</h3>
                            <p className="mt-0.5 text-[12.5px] leading-relaxed text-muted-foreground">{action.description}</p>
                          </div>

                          {bindings.length === 0 ? (
                            <Button
                              type="button"
                              variant="outline"
                              size="sm"
                              disabled={editingDisabled || isCapturing}
                              onClick={() => addDraftBinding(action)}
                            >
                              <Plus className="h-3.5 w-3.5" />
                              Set shortcut
                            </Button>
                          ) : (
                            <div className="space-y-3">
                              {bindings.map((binding) => {
                                const isEditing = editingCapture?.bindingId === binding.id;
                                const isSaving = savingBindingId === binding.id;
                                const showRecordingCheckbox = action.action === "toggle_recording" || action.action === "hold_to_record";

                                if (isEditing && editingCapture) {
                                  // ── Edit / capture mode ──────────────────────────
                                  return (
                                    <div key={binding.id} className="rounded-xl border border-sage/40 bg-sage-bg/40 p-3 space-y-3">
                                      <div className="flex items-start gap-2">
                                        <HotkeyInput
                                          inline
                                          value={editingCapture.combo}
                                          onChange={(combo) =>
                                            setEditingCapture((prev) =>
                                              prev && prev.combo !== combo
                                                ? { ...prev, combo, bareModifier: null }
                                                : prev
                                            )
                                          }
                                          placeholder="Press a key or key combo"
                                          validationRules={
                                            action.allows_single_key
                                              ? singleKeyValidation
                                              : ValidationPresets.standard()
                                          }
                                          allowBareModifier={showRecordingCheckbox}
                                          onBareModifier={(spec) =>
                                            setEditingCapture((prev) =>
                                              prev ? { ...prev, bareModifier: spec, combo: "" } : prev
                                            )
                                          }
                                        />
                                        <Button
                                          type="button"
                                          size="icon-sm"
                                          aria-label="Save"
                                          className="bg-green-600 text-white hover:bg-green-600/90"
                                          disabled={
                                            isSaving ||
                                            (!editingCapture.combo && !editingCapture.bareModifier)
                                          }
                                          onClick={saveEdit}
                                        >
                                          {isSaving ? <Spinner className="h-4 w-4" /> : <Check className="h-4 w-4" />}
                                        </Button>
                                        <Button
                                          type="button"
                                          variant="ghost"
                                          size="icon-sm"
                                          aria-label="Cancel"
                                          disabled={isSaving}
                                          onClick={cancelEdit}
                                        >
                                          <X className="h-4 w-4" />
                                        </Button>
                                      </div>

                                      {!editingCapture.bareModifier && (
                                        <p className="text-xs text-muted-foreground">
                                          Use a key combo, a function key, or a numpad/navigation key. A bare letter or number won't work — it would block typing.
                                        </p>
                                      )}
                                    </div>
                                  );
                                }

                                // ── Read mode ────────────────────────────────────
                                return (
                                  <div key={binding.id} className="flex flex-col gap-3 rounded-xl border border-border p-3 sm:flex-row sm:items-center sm:justify-between">
                                    <span aria-label={`${action.label} shortcut`} className="font-mono text-sm">
                                      {formatBindingDisplay(binding)}
                                    </span>
                                    <div className="flex items-center gap-3">
                                      {isSaving && <Spinner className="h-4 w-4 text-muted-foreground" />}
                                      <Button
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        disabled={editingDisabled || isCapturing}
                                        onClick={() => startEditing(binding)}
                                      >
                                        <Pencil className="mr-1 h-3.5 w-3.5" />
                                        Edit
                                      </Button>
                                      <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon-sm"
                                        aria-label="Remove"
                                        className="text-muted-foreground hover:text-destructive"
                                        disabled={editingDisabled || isCapturing}
                                        onClick={() => void deleteBinding(binding.id)}
                                      >
                                        <Trash2 className="h-4 w-4" />
                                      </Button>
                                    </div>
                                  </div>
                                );
                              })}
                            </div>
                          )}
                        </div>
                      );
                    })}
              </div>
            </SettingsCard>
          );
        })
      )}
    </SettingsPage>
  );
}
