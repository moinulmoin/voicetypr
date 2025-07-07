import { Check, Edit2, Keyboard, X } from "lucide-react";
import React, { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";

interface HotkeyInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}

export const HotkeyInput = React.memo(function HotkeyInput({ value, onChange, placeholder }: HotkeyInputProps) {
  const [mode, setMode] = useState<"display" | "edit">("display");
  const [isRecording, setIsRecording] = useState(false);
  const [keys, setKeys] = useState<Set<string>>(new Set());
  const [pendingHotkey, setPendingHotkey] = useState("");
  const [saveStatus, setSaveStatus] = useState<"idle" | "success" | "error">("idle");
  const [validationError, setValidationError] = useState<string>("");
  const [currentKeysDisplay, setCurrentKeysDisplay] = useState<string>("");
  const inputRef = useRef<HTMLInputElement>(null);
  const isEscapePressed = useRef(false);

  useEffect(() => {
    if (!isRecording) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const key = e.key;

      // Handle Escape to cancel
      if (key === "Escape") {
        isEscapePressed.current = true;
        setIsRecording(false);
        setKeys(new Set());
        inputRef.current?.blur();
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
        } else {
          newKeys.add(key.length === 1 ? key.toUpperCase() : key);
        }
      }

      // Check max keys limit
      if (newKeys.size > 3) {
        setValidationError("Maximum 3 keys allowed");
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
          displayKeys.push(formatShortcutDisplay("CommandOrControl"));
        if (modifiers.includes("Alt")) displayKeys.push(formatShortcutDisplay("Alt"));
        if (modifiers.includes("Shift")) displayKeys.push(formatShortcutDisplay("Shift"));
        displayKeys.push(...regularKeys.map((k) => (k === "Space" ? "␣" : k)));
        setCurrentKeysDisplay(displayKeys.join(" + "));

        // Check minimum keys
        if (newKeys.size < 2) {
          setValidationError("Minimum 2 keys required");
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

        if (keys.size >= 2 && keys.size <= 3) {
          setPendingHotkey(shortcut);
          setIsRecording(false);
          setKeys(new Set());
          setCurrentKeysDisplay("");
        } else {
          setValidationError(keys.size < 2 ? "Minimum 2 keys required" : "Maximum 3 keys allowed");
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
    };
  }, [isRecording, keys, onChange]);

  const formatShortcutDisplay = useCallback((shortcut: string) => {
    const isMac = navigator.userAgent.toUpperCase().indexOf("MAC") >= 0;
    return shortcut
      .replace("CommandOrControl", isMac ? "⌘" : "Ctrl")
      .replace("Shift", "⇧")
      .replace("Alt", isMac ? "⌥" : "Alt")
      .replace("Plus", "+")
      .replace("Space", "␣");
  }, []);

  const handleSave = useCallback(() => {
    if (pendingHotkey && !validationError) {
      onChange(pendingHotkey);
      setSaveStatus("success");
      
      setTimeout(() => {
        setMode("display");
        setSaveStatus("idle");
      }, 1500);
    }
  }, [pendingHotkey, validationError, onChange]);

  const handleCancel = useCallback(() => {
    setPendingHotkey("");
    setIsRecording(false);
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

    setTimeout(() => {
      inputRef.current?.focus();
      setIsRecording(true);
    }, 100);
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
          <div className="relative flex-1">
            <Input
              value={formatShortcutDisplay(value)}
              readOnly
              className="pr-10"
              placeholder={placeholder || "No hotkey set"}
            />
            <Keyboard className="absolute right-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
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
        <div className="relative flex-1">
          <Input
            ref={inputRef}
            value={
              isRecording && currentKeysDisplay
                ? currentKeysDisplay
                : pendingHotkey
                ? formatShortcutDisplay(pendingHotkey)
                : ""
            }
            onClick={() => {
              if (!isRecording && mode === "edit") {
                setIsRecording(true);
                inputRef.current?.focus();
              }
            }}
            onBlur={() => {
              if (!isEscapePressed.current) {
                setIsRecording(false);
                setKeys(new Set());
                setCurrentKeysDisplay("");
              }
              isEscapePressed.current = false;
            }}
            readOnly
            className="pr-10"
            placeholder="Click here to set hotkey"
          />
          <Keyboard className="absolute right-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
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
      {isRecording && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">Press keys now</span>
        </div>
      )}
      {!isRecording && pendingHotkey && !validationError && (
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
