import { HotkeyInput } from "@/components/HotkeyInput";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { normalizeShortcutKeys, ValidationPresets } from "@/lib/keyboard-normalizer";
import type {
  ShortcutAction,
  ShortcutActionDefinition,
  ShortcutBinding,
  ShortcutSettings,
} from "@/types/shortcuts";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, Keyboard, Plus, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

const emptySettings: ShortcutSettings = { bindings: [] };

const singleKeyValidation = ValidationPresets.custom({
  minKeys: 1,
  requireModifier: false,
  requireModifierForMultiKey: true,
});

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

function formatTrigger(trigger: ShortcutBinding["trigger"]) {
  return trigger === "hold" ? "Hold" : "Press";
}

function shortcutComparisonKey(shortcut: string) {
  return normalizeShortcutKeys(shortcut).toLowerCase();
}

export function ShortcutsSection() {
  const [actions, setActions] = useState<ShortcutActionDefinition[]>([]);
  const [settings, setSettings] = useState<ShortcutSettings>(emptySettings);
  const [draftBindings, setDraftBindings] = useState<ShortcutBinding[]>([]);
  const [loading, setLoading] = useState(true);
  const [actionLoadError, setActionLoadError] = useState<string | null>(null);
  const [settingsLoadError, setSettingsLoadError] = useState<string | null>(null);
  const [savingBindingId, setSavingBindingId] = useState<string | null>(null);
  const savingBindingIdRef = useRef<string | null>(null);

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
          console.error("Failed to load shortcut actions:", actionResult.reason);
          setActions([]);
          nextActionLoadError = formatError(actionResult.reason);
        }

        if (settingsResult.status === "fulfilled") {
          setSettings(normalizeSettings(settingsResult.value));
          nextSettingsLoadError = null;
        } else {
          console.error("Failed to load shortcut settings:", settingsResult.reason);
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

  const isMutating = savingBindingId !== null;
  const editingDisabled = isMutating || settingsLoadError !== null;

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
      console.error("Failed to save shortcut:", error);
      toast.error("Could not save shortcut", {
        description: "Global shortcuts may conflict with macOS, Windows, or other apps. VoiceTypr tests registration and refuses unavailable shortcuts.",
      });
    } finally {
      endMutation();
    }
  }, [actionLabels, beginMutation, draftBindings, endMutation, persistSettings, settings.bindings]);

  const changeBinding = useCallback((nextBinding: ShortcutBinding) => {
    if (editingDisabled || savingBindingIdRef.current !== null) {
      return;
    }

    if (draftBindings.some((binding) => binding.id === nextBinding.id) && !nextBinding.shortcut) {
      setDraftBindings((bindings) => bindings.map((binding) =>
        binding.id === nextBinding.id ? nextBinding : binding,
      ));
      return;
    }

    updateBinding(nextBinding);
  }, [draftBindings, editingDisabled, updateBinding]);

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
      console.error("Failed to remove shortcut:", error);
      toast.error("Could not remove shortcut", { description: formatError(error) });
    } finally {
      endMutation();
    }
  }, [beginMutation, draftBindings, editingDisabled, endMutation, persistSettings, settings.bindings]);

  const addDraftBinding = useCallback((action: ShortcutActionDefinition) => {
    if (editingDisabled || savingBindingIdRef.current !== null) {
      return;
    }

    setDraftBindings((bindings) => [...bindings, createBinding(action)]);
  }, [editingDisabled]);

  return (
    <div className="h-full min-h-0 flex flex-col">
      <div className="shrink-0 border-b border-border/40 px-6 py-4">
        <div className="flex items-center gap-2">
          <h1 className="text-2xl font-semibold">Shortcuts</h1>
        </div>
        <p className="mt-1 text-sm text-muted-foreground">
          Configure global shortcuts for recording, history, formatting modes, and the dashboard.
        </p>
      </div>

      <ScrollArea className="flex-1 min-h-0">
        <div className="space-y-5 p-6">
          <div className="rounded-lg border border-amber-500/25 bg-amber-500/10 p-3 text-sm text-amber-900 dark:text-amber-300">
            <div className="flex gap-2">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
              <p>
                Global shortcuts may conflict with macOS, Windows, or other apps. VoiceTypr tests registration and will refuse unavailable shortcuts.
              </p>
            </div>
          </div>

          {actionLoadError && (
            <div role="alert" className="rounded-lg border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
              Shortcut actions could not be loaded: {actionLoadError}. You can still review saved shortcuts once the app reconnects.
            </div>
          )}

          {settingsLoadError && (
            <div role="alert" className="rounded-lg border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
              Shortcut settings could not be loaded: {settingsLoadError}. Reload shortcut settings before editing; controls are read-only to avoid overwriting existing shortcuts.
            </div>
          )}

          {loading ? (
            <div className="flex items-center gap-2 rounded-xl border border-border/60 bg-card p-4 text-sm text-muted-foreground">
              <Spinner className="h-4 w-4" />
              Loading shortcuts…
            </div>
          ) : groupedActions.length === 0 ? (
            <div className="rounded-xl border border-border/60 bg-card p-6 text-sm text-muted-foreground">
              No shortcut actions are available.
            </div>
          ) : (
            groupedActions.map(([section, sectionActions]) => {
              const sectionBindingCount = sectionActions.reduce(
                (count, action) => count + (bindingsByAction.get(action.action)?.length ?? 0),
                0,
              );

              return (
              <section key={section} className="rounded-xl border border-border/60 bg-card">
                <div className="border-b border-border/60 px-4 py-3">
                  <div className="flex items-center gap-2">
                    <div className="rounded-md bg-primary/10 p-1.5">
                      <Keyboard className="h-4 w-4 text-primary" />
                    </div>
                    <div>
                      <h2 className="font-medium">{section}</h2>
                      <p className="text-xs text-muted-foreground">
                        {sectionBindingCount === 1 ? "1 binding configured" : `${sectionBindingCount} bindings configured`}
                      </p>
                    </div>
                  </div>
                </div>

                <div className="divide-y divide-border/60">
                  {sectionActions.map((action) => {
                    const bindings = bindingsByAction.get(action.action) ?? [];

                    return (
                      <div key={action.action} role="group" aria-label={action.label} className="space-y-3 p-4">
                        <div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
                          <div className="min-w-0">
                            <h3 className="font-medium">{action.label}</h3>
                            <p className="mt-1 text-sm text-muted-foreground">{action.description}</p>
                          </div>
                          <Button type="button" variant="outline" size="sm" disabled={editingDisabled} onClick={() => addDraftBinding(action)}>
                            <Plus className="h-3.5 w-3.5" />
                            Add shortcut
                          </Button>
                        </div>

                        {bindings.length === 0 ? (
                          <div className="rounded-lg border border-dashed border-border/70 px-3 py-2 text-sm text-muted-foreground">
                            No shortcut set
                          </div>
                        ) : (
                          <div className="space-y-3">
                            {bindings.map((binding) => {
                              const isHoldToRecord = binding.action === "hold_to_record";
                              const canUseSingleKey = isHoldToRecord && binding.allow_risky_combo;
                              const isSaving = savingBindingId === binding.id;

                              return (
                                <div key={binding.id} className="rounded-lg border border-border/60 p-3">
                                  <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
                                    <div className="min-w-[220px] flex-1">
                                      {editingDisabled ? (
                                        <div className="flex min-h-9 items-center rounded-md border border-input bg-muted/30 px-3 text-sm text-muted-foreground" aria-label={`${action.label} shortcut read-only`}>
                                          {binding.shortcut || "No shortcut configured"}
                                        </div>
                                      ) : (
                                        <HotkeyInput
                                          value={binding.shortcut}
                                          onChange={(shortcut) => changeBinding({ ...binding, shortcut })}
                                          placeholder="Click to set shortcut"
                                          validationRules={canUseSingleKey ? singleKeyValidation : ValidationPresets.standard()}
                                        />
                                      )}
                                    </div>

                                    <div className="flex flex-wrap items-center gap-3">
                                      {isSaving && <Spinner className="h-4 w-4 text-muted-foreground" />}
                                      <span className="rounded-full border border-border/70 bg-muted/50 px-2 py-0.5 text-xs font-medium text-muted-foreground">
                                        {formatTrigger(binding.trigger)}
                                      </span>
                                      <label className="flex items-center gap-2 text-sm text-muted-foreground">
                                        <Switch
                                          aria-label={`${action.label} enabled`}
                                          checked={binding.enabled}
                                          disabled={!binding.shortcut || editingDisabled}
                                          onCheckedChange={(enabled) => changeBinding({ ...binding, enabled })}
                                        />
                                        Enabled
                                      </label>
                                      <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon-sm"
                                        aria-label={`Delete ${action.label} shortcut`}
                                        disabled={editingDisabled}
                                        onClick={() => deleteBinding(binding.id)}
                                      >
                                        <Trash2 className="h-4 w-4" />
                                      </Button>
                                    </div>
                                  </div>

                                  {isHoldToRecord && (
                                    <div className="mt-3 rounded-md bg-muted/40 p-3">
                                      <label className="flex items-start gap-3 text-sm">
                                        <Switch
                                          aria-label="Allow single-key push-to-talk"
                                          checked={binding.allow_risky_combo}
                                          disabled={editingDisabled}
                                          onCheckedChange={(allow_risky_combo) =>
                                            changeBinding({ ...binding, allow_risky_combo })
                                          }
                                        />
                                        <span>
                                          <span className="block font-medium text-foreground">Allow single-key push-to-talk</span>
                                          <span className="block text-muted-foreground">
                                            Single keys are captured globally. Registration may still be refused by macOS, Windows, or another app that already owns the key.
                                          </span>
                                          {canUseSingleKey && (
                                            <span className="mt-1 block text-xs text-amber-700 dark:text-amber-400">
                                              Single-key validation is enabled for this push-to-talk binding.
                                            </span>
                                          )}
                                        </span>
                                      </label>
                                    </div>
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </section>
              );
            })
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
