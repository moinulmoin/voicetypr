import { Check, Edit2, X } from "lucide-react";
import React, { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "./ui/button";
import { formatHotkey } from "@/lib/hotkey-utils";
import { isMacOS } from "@/lib/platform";
import {
  normalizeShortcutKeys,
  validateKeyCombinationWithRules,
  formatKeyForDisplay,
  KeyValidationRules,
  ValidationPresets
} from "@/lib/keyboard-normalizer";
import { mapCodeToKey } from "@/lib/keyboard-mapper";
import { checkForSystemConflict, formatConflictMessage } from "@/lib/hotkey-conflicts";

export interface BareModifierSpec {
  /** One of: "alt" | "control" | "meta" | "shift" */
  modifier: string;
  /** One of: "left" | "right" | "either" */
  side: string;
}

interface HotkeyInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  validationRules?: KeyValidationRules;
  label?: string; // e.g., "Recording Hotkey", "Custom Hotkey"
  onEditingChange?: (isEditing: boolean) => void; // Notify parent when edit mode changes
  /** When true, a lone side-specific modifier (e.g. Right-Option) is accepted as a
   *  valid selection and reported via onBareModifier instead of onChange. */
  allowBareModifier?: boolean;
  /** Called when the user confirms a bare modifier selection. */
  onBareModifier?: (spec: BareModifierSpec) => void;
  /** When true, render as a bare always-capturing field — no internal display/edit
   *  toggle and no Save/Cancel/Edit buttons. Selections report live via onChange /
   *  onBareModifier so the parent owns the controls. */
  inline?: boolean;
}

const BARE_MOD_ICONS: Record<string, string> = {
  alt: "⌥", meta: "⌘", control: "⌃", shift: "⇧",
};

function formatBareModifierDisplay(spec: BareModifierSpec): string {
  const sideLabel = spec.side === "right" ? "Right " : spec.side === "left" ? "Left " : "";
  const modLabel = isMacOS
    ? (BARE_MOD_ICONS[spec.modifier] ?? spec.modifier)
    : spec.modifier.charAt(0).toUpperCase() + spec.modifier.slice(1);
  return `${sideLabel}${modLabel} · hold to record`;
}

export const HotkeyInput = React.memo(function HotkeyInput({
  value,
  onChange,
  placeholder,
  validationRules = ValidationPresets.standard(),
  onEditingChange,
  allowBareModifier = false,
  onBareModifier,
  inline = false,
}: HotkeyInputProps) {
  const [mode, setMode] = useState<"display" | "edit">("display");
  const [keys, setKeys] = useState<Set<string>>(new Set());
  const [pendingHotkey, setPendingHotkey] = useState("");
  const [pendingBareModifier, setPendingBareModifier] = useState<BareModifierSpec | null>(null);
  const [saveStatus, setSaveStatus] = useState<"idle" | "success" | "error">("idle");
  const [validationError, setValidationError] = useState<string>("");
  const [currentKeysDisplay, setCurrentKeysDisplay] = useState<string>("");
  const onChangeRef = useRef(onChange);
  const onBareModifierRef = useRef(onBareModifier);
  useEffect(() => {
    onChangeRef.current = onChange;
    onBareModifierRef.current = onBareModifier;
  });

  const handleCancel = useCallback(() => {
    setPendingHotkey("");
    setPendingBareModifier(null);
    setKeys(new Set());
    setMode("display");
    setSaveStatus("idle");
    setValidationError("");
    setCurrentKeysDisplay("");
    onEditingChange?.(false);
  }, [onEditingChange]);

  useEffect(() => {
    if (mode !== "edit" && !inline) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const key = e.key;
      const code = e.code || ""; // Get physical key code with fallback for older browsers

      // Handle Escape to cancel
      if (key === "Escape") {
        handleCancel();
        return;
      }

      const newKeys = new Set(keys);

      // Add modifier keys - handle platform differences correctly
      if (isMacOS) {
        // On macOS, Command maps to CommandOrControl and Control is tracked
        // separately, so both can coexist in a single combo (e.g. Cmd+Ctrl+K).
        // A lone Command → CommandOrControl; a lone Control → Control.
        if (e.metaKey) newKeys.add("CommandOrControl");
        if (e.ctrlKey) newKeys.add("Control");
      } else {
        // On Windows/Linux, Control key maps to CommandOrControl
        if (e.ctrlKey) newKeys.add("CommandOrControl");
      }
      if (e.shiftKey) newKeys.add("Shift");
      if (e.altKey) newKeys.add("Alt");

      // Add the actual key using physical position (e.code) for international keyboard support
      if (!["Control", "Shift", "Alt", "Meta"].includes(key)) {
        // Use physical key code when available, fallback to key for older browsers
        const mappedKey = code ? mapCodeToKey(code) : key;

        // Special handling for keys that need it
        if (mappedKey === "Enter") {
          newKeys.add("Return"); // Tauri expects "Return" not "Enter"
        } else if (mappedKey === "Up" || mappedKey === "Down" || mappedKey === "Left" || mappedKey === "Right") {
          newKeys.add(mappedKey); // Arrow keys
        } else if (mappedKey.startsWith("F") && mappedKey.length <= 3) {
          newKeys.add(mappedKey); // Function keys
        } else if (["PageUp", "PageDown", "Home", "End", "Insert", "Delete", "Backspace", "Tab", "Space", "Escape"].includes(mappedKey)) {
          newKeys.add(mappedKey); // Special keys
        } else if (mappedKey.startsWith("Numpad")) {
          newKeys.add(mappedKey); // Numpad keys
        } else if (["Comma", "Period", "Semicolon", "Quote", "BracketLeft", "BracketRight",
                    "Backslash", "Slash", "Equal", "Minus", "Backquote"].includes(mappedKey)) {
          // Punctuation keys - use the physical position name
          newKeys.add(mappedKey);
        } else if (mappedKey.length === 1) {
          // Single character (letter or number)
          newKeys.add(mappedKey.toUpperCase());
        } else {
          // Fallback to the mapped key
          newKeys.add(mappedKey);
        }
      }

      // Check max keys limit
      if (newKeys.size > validationRules.maxKeys) {
        setValidationError(`Maximum ${validationRules.maxKeys} keys allowed`);
        return;
      }

      setKeys(newKeys);
      setValidationError("");

      // Update pending hotkey preview
      const modifiers: string[] = [];
      const regularKeys: string[] = [];

      newKeys.forEach((k) => {
        if (["CommandOrControl", "Control", "Shift", "Alt"].includes(k)) {
          modifiers.push(k);
        } else {
          regularKeys.push(k);
        }
      });

      // ── Bare modifier path ────────────────────────────────────────────────
      // When the caller opts in (allowBareModifier), a single side-specific
      // modifier key pressed alone is a valid selection.  We read the
      // physical side from e.code (AltRight/AltLeft etc.).
      if (allowBareModifier && modifiers.length === 1 && regularKeys.length === 0) {
        const side: string = code.endsWith("Right") ? "right"
          : code.endsWith("Left") ? "left"
          : "either";
        const modMap: Record<string, string> = {
          Alt: "alt", Control: "control", Meta: "meta", Shift: "shift",
        };
        // key is the modifier being pressed here (already checked regularKeys.length===0)
        const modifierKind = modMap[key] ?? "alt";
        const spec: BareModifierSpec = { modifier: modifierKind, side };
        setPendingBareModifier(spec);
        setPendingHotkey("");
        setValidationError("");
        setCurrentKeysDisplay(formatBareModifierDisplay(spec));
        return;
      }

      // Not a bare modifier (has a regular key, or multiple modifiers without key).
      // Clear any previously captured bare modifier so the paths stay exclusive.
      setPendingBareModifier(null);
      // ─────────────────────────────────────────────────────────────────────

      const orderedModifiers = ["CommandOrControl", "Control", "Alt", "Shift"].filter((mod) =>
        modifiers.includes(mod)
      );
      const shortcut = [...orderedModifiers, ...regularKeys].join("+");
      if (shortcut) {
        // Update current keys display
        const displayKeys = [];
        if (modifiers.includes("CommandOrControl"))
          displayKeys.push(formatKeyForDisplay("CommandOrControl", isMacOS));
        if (modifiers.includes("Control"))
          displayKeys.push(formatKeyForDisplay("Control", isMacOS));
        if (modifiers.includes("Alt")) displayKeys.push(formatKeyForDisplay("Alt", isMacOS));
        if (modifiers.includes("Shift")) displayKeys.push(formatKeyForDisplay("Shift", isMacOS));
        displayKeys.push(...regularKeys.map(k => formatKeyForDisplay(k, isMacOS)));
        setCurrentKeysDisplay(displayKeys.join(" + "));

        // Validate with rules
        const validation = validateKeyCombinationWithRules(shortcut, validationRules);
        if (!validation.valid) {
          setValidationError(validation.error || "Invalid key combination");
          setPendingHotkey("");
        } else {
          // Check for system conflicts
          const conflict = checkForSystemConflict(shortcut);
          if (conflict) {
            setValidationError(formatConflictMessage(conflict));
            // Still show the hotkey but with warning for 'warning' severity
            if (conflict.severity === 'warning') {
              setPendingHotkey(shortcut);
            } else {
              setPendingHotkey("");
            }
          } else {
            setPendingHotkey(shortcut);
            setValidationError("");
          }
        }
      }
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      if (keys.size > 0) {
        // Format the shortcut
        const modifiers: string[] = [];
        const regularKeys: string[] = [];

        keys.forEach((key) => {
          if (["CommandOrControl", "Control", "Shift", "Alt"].includes(key)) {
            modifiers.push(key);
          } else {
            regularKeys.push(key);
          }
        });

        // ── Bare modifier release ─────────────────────────────────────────
        // When allowBareModifier is true and the released sequence has no
        // regular key, the bare modifier set during keydown is already the
        // selection.  Just clear the keys state so the display stays clean.
        if (allowBareModifier && regularKeys.length === 0) {
          setKeys(new Set());
          return;
        }
        // ─────────────────────────────────────────────────────────────────

        // Standard order: CommandOrControl+Control+Alt+Shift+Key
        const orderedModifiers = ["CommandOrControl", "Control", "Alt", "Shift"].filter((mod) =>
          modifiers.includes(mod)
        );
        const shortcut = [...orderedModifiers, ...regularKeys].join("+");

        const validation = validateKeyCombinationWithRules(shortcut, validationRules);
        if (validation.valid) {
          // Check for system conflicts
          const conflict = checkForSystemConflict(shortcut);
          if (conflict) {
            setValidationError(formatConflictMessage(conflict));
            // Still allow setting it, but with warning
            if (conflict.severity === 'warning') {
              setPendingHotkey(shortcut);
              setKeys(new Set());
              setCurrentKeysDisplay("");
            }
          } else {
            setPendingHotkey(shortcut);
            setKeys(new Set());
            setCurrentKeysDisplay("");
            setValidationError("");
          }
        } else {
          setValidationError(validation.error || "Invalid key combination");
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
    };
  }, [mode, keys, allowBareModifier, inline]);

  // Inline mode: report captured selections live; the parent persists.
  useEffect(() => {
    if (!inline) return;
    if (pendingHotkey && !validationError) {
      onChangeRef.current(normalizeShortcutKeys(pendingHotkey));
    } else if (validationError) {
      onChangeRef.current("");
    }
  }, [inline, pendingHotkey, validationError]);

  useEffect(() => {
    if (inline && pendingBareModifier) {
      onBareModifierRef.current?.(pendingBareModifier);
    }
  }, [inline, pendingBareModifier]);


  const handleSave = useCallback(() => {
    // ── Bare modifier save ────────────────────────────────────────────────
    if (allowBareModifier && pendingBareModifier) {
      onBareModifier?.(pendingBareModifier);
      setSaveStatus("success");
      setMode("display");
      setSaveStatus("idle");
      setPendingBareModifier(null);
      setPendingHotkey("");
      setKeys(new Set());
      setCurrentKeysDisplay("");
      onEditingChange?.(false);
      return;
    }
    // ─────────────────────────────────────────────────────────────────────

    if (pendingHotkey && !validationError) {
      // Normalize the shortcut before saving
      const normalizedShortcut = normalizeShortcutKeys(pendingHotkey);

      // Final validation
      const validation = validateKeyCombinationWithRules(normalizedShortcut, validationRules);
      if (!validation.valid) {
        setValidationError(validation.error || "Invalid key combination");
        return;
      }

      onChange(normalizedShortcut);
      setSaveStatus("success");
      setMode("display");
      setSaveStatus("idle");
      setPendingHotkey("");
      setKeys(new Set());
      setCurrentKeysDisplay("");
      onEditingChange?.(false); // Notify parent that editing is done
    }
  }, [allowBareModifier, pendingBareModifier, pendingHotkey, validationError, onChange, onBareModifier, onEditingChange, validationRules]);

  const handleEdit = useCallback(() => {
    setPendingHotkey("");
    setPendingBareModifier(null);
    setMode("edit");
    setSaveStatus("idle");
    setValidationError("");
    setCurrentKeysDisplay("");
    setKeys(new Set());
    onEditingChange?.(true); // Notify parent that editing has started
  }, [onEditingChange]);

  // Reset save status after showing success
  useEffect(() => {
    if (saveStatus === "success") {
      const timer = setTimeout(() => setSaveStatus("idle"), 3000);
      return () => clearTimeout(timer);
    }
  }, [saveStatus]);


  if (inline) {
    return (
      <div className="min-w-0 flex-1">
        <div className="flex min-h-8 items-center font-mono text-sm">
          {pendingBareModifier ? (
            <span className="text-foreground">{formatBareModifierDisplay(pendingBareModifier)}</span>
          ) : pendingHotkey ? (
            formatHotkey(pendingHotkey)
          ) : currentKeysDisplay ? (
            <span className="text-foreground">{currentKeysDisplay}</span>
          ) : value ? (
            formatHotkey(value)
          ) : (
            <span className="text-muted-foreground">{placeholder || "Press keys…"}</span>
          )}
        </div>
        {validationError && (
          <p className="mt-1 text-xs text-destructive">{validationError}</p>
        )}
      </div>
    );
  }

  if (mode === "display") {
    return (
      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <div className="flex-1 flex items-center">
            {value ? (
              formatHotkey(value)
            ) : (
              <span className="text-muted-foreground">{placeholder || "No hotkey set"}</span>
            )}
          </div>
          <Button size="icon" onClick={handleEdit} title="Change hotkey">
            <Edit2 />
          </Button>
        </div>
        {saveStatus === "success" && (
          <div className="flex items-center gap-1 text-sm text-green-600">
            <Check className="w-3 h-3" />
            <span>Hotkey updated successfully</span>
          </div>
        )}
      </div>
    );
  }

  // Edit mode: the bare modifier display takes priority over the combo preview.
  const editLabel: string | null = pendingBareModifier
    ? formatBareModifierDisplay(pendingBareModifier)
    : null;

  const canSave = (allowBareModifier && !!pendingBareModifier) || (!!pendingHotkey && !validationError);

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <div className="flex-1 flex items-center">
          {editLabel ? (
            <span className="text-foreground">{editLabel}</span>
          ) : pendingHotkey ? (
            formatHotkey(pendingHotkey)
          ) : currentKeysDisplay ? (
            <span className="text-foreground">{currentKeysDisplay}</span>
          ) : (
            <span className="text-muted-foreground">Press keys to set hotkey</span>
          )}
        </div>
        <Button
          size="icon"
          variant="default"
          onClick={handleSave}
          disabled={!canSave}
          title="Save hotkey"
        >
          <Check className="w-4 h-4" />
        </Button>
        <Button
          size="icon"
          variant="outline"
          onClick={handleCancel}
          title="Cancel"
        >
          <X className="w-4 h-4" />
        </Button>
      </div>
      {canSave && !validationError && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">Click ✓ to save</span>
        </div>
      )}
      {validationError && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-destructive">{validationError}</span>
        </div>
      )}
    </div>
  );
});
