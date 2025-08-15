import { Check, Edit2, X } from "lucide-react";
import React, { useCallback, useEffect, useState } from "react";
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

interface HotkeyInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  validationRules?: KeyValidationRules;
  label?: string; // e.g., "Recording Hotkey", "Custom Hotkey"
}

export const HotkeyInput = React.memo(function HotkeyInput({ 
  value, 
  onChange, 
  placeholder,
  validationRules = ValidationPresets.standard()
}: HotkeyInputProps) {
  const [mode, setMode] = useState<"display" | "edit">("display");
  const [keys, setKeys] = useState<Set<string>>(new Set());
  const [pendingHotkey, setPendingHotkey] = useState("");
  const [saveStatus, setSaveStatus] = useState<"idle" | "success" | "error">("idle");
  const [validationError, setValidationError] = useState<string>("");
  const [currentKeysDisplay, setCurrentKeysDisplay] = useState<string>("");

  useEffect(() => {
    if (mode !== "edit") return;

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const key = e.key;

      // Handle Escape to cancel
      if (key === "Escape") {
        handleCancel();
        return;
      }

      const newKeys = new Set(keys);

      // Add modifier keys
      if (e.metaKey || e.ctrlKey) newKeys.add("CommandOrControl");
      if (e.shiftKey) newKeys.add("Shift");
      if (e.altKey) newKeys.add("Alt");

      // Add the actual key (capitalize it)
      if (!["Control", "Shift", "Alt", "Meta"].includes(key)) {
        if (key === " " || key === "Space") {
          newKeys.add("Space");
        } else if (key === "Tab") {
          newKeys.add("Tab");
        } else if (key === "Enter") {
          newKeys.add("Return");
        } else if (key === "Backspace") {
          newKeys.add("Backspace");
        } else if (key === "Delete") {
          newKeys.add("Delete");
        } else if (key.startsWith("Arrow")) {
          newKeys.add(key); // ArrowUp, ArrowDown, ArrowLeft, ArrowRight
        } else if (key.startsWith("F") && key.length <= 3) {
          newKeys.add(key); // F1-F12
        } else if (key === "PageUp" || key === "PageDown" || key === "Home" || key === "End") {
          newKeys.add(key);
        } else {
          newKeys.add(key.length === 1 ? key.toUpperCase() : key);
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
        if (["CommandOrControl", "Shift", "Alt"].includes(k)) {
          modifiers.push(k);
        } else {
          regularKeys.push(k);
        }
      });

      const orderedModifiers = ["CommandOrControl", "Alt", "Shift"].filter((mod) =>
        modifiers.includes(mod)
      );
      const shortcut = [...orderedModifiers, ...regularKeys].join("+");
      if (shortcut) {
        // Update current keys display
        const displayKeys = [];
        if (modifiers.includes("CommandOrControl"))
          displayKeys.push(formatKeyForDisplay("CommandOrControl", isMacOS));
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
          setPendingHotkey(shortcut);
          setValidationError("");
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
          if (["CommandOrControl", "Shift", "Alt"].includes(key)) {
            modifiers.push(key);
          } else {
            regularKeys.push(key);
          }
        });

        // Standard order: CommandOrControl+Alt+Shift+Key
        const orderedModifiers = ["CommandOrControl", "Alt", "Shift"].filter((mod) =>
          modifiers.includes(mod)
        );
        const shortcut = [...orderedModifiers, ...regularKeys].join("+");

        const validation = validateKeyCombinationWithRules(shortcut, validationRules);
        if (validation.valid) {
          setPendingHotkey(shortcut);
          setKeys(new Set());
          setCurrentKeysDisplay("");
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
  }, [mode, keys]);


  const handleSave = useCallback(() => {
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

      setTimeout(() => {
        setMode("display");
        setSaveStatus("idle");
        setPendingHotkey("");
        setKeys(new Set());
        setCurrentKeysDisplay("");
      }, 1500);
    }
  }, [pendingHotkey, validationError, onChange]);

  const handleCancel = useCallback(() => {
    setPendingHotkey("");
    setKeys(new Set());
    setMode("display");
    setSaveStatus("idle");
    setValidationError("");
    setCurrentKeysDisplay("");
  }, []);

  const handleEdit = useCallback(() => {
    setPendingHotkey("");
    setMode("edit");
    setSaveStatus("idle");
    setValidationError("");
    setCurrentKeysDisplay("");
    setKeys(new Set());
  }, []);

  // Reset save status after showing success
  useEffect(() => {
    if (saveStatus === "success") {
      const timer = setTimeout(() => setSaveStatus("idle"), 3000);
      return () => clearTimeout(timer);
    }
  }, [saveStatus]);


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

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <div className="flex-1 flex items-center">
          {pendingHotkey ? (
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
          disabled={!pendingHotkey || !!validationError}
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
      {pendingHotkey && !validationError && (
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