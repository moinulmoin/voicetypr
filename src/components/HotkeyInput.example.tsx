/**
 * Example usage of HotkeyInput with different validation rules
 */

import { HotkeyInput } from "./HotkeyInput";
import { ValidationPresets } from "@/lib/keyboard-normalizer";
import { useState } from "react";

export function HotkeyInputExamples() {
  const [recordingHotkey, setRecordingHotkey] = useState("CommandOrControl+Shift+Space");
  const [customHotkey, setCustomHotkey] = useState("Alt+1");

  return (
    <div className="space-y-8 p-6">
      {/* Standard recording hotkey - 2-5 keys with at least one modifier */}
      <div className="space-y-2">
        <h3 className="text-lg font-semibold">Recording Hotkey</h3>
        <p className="text-sm text-muted-foreground">
          Requires 2-5 keys with at least one modifier (e.g., Cmd+R, Alt+Shift+Space)
        </p>
        <HotkeyInput
          value={recordingHotkey}
          onChange={setRecordingHotkey}
          placeholder="Set recording hotkey"
          validationRules={ValidationPresets.standard()}
          label="Recording Hotkey"
        />
      </div>

      {/* Custom validation rules */}
      <div className="space-y-2">
        <h3 className="text-lg font-semibold">Custom Rules</h3>
        <p className="text-sm text-muted-foreground">
          Custom: 1-3 keys max (example of restricted rules)
        </p>
        <HotkeyInput
          value={customHotkey}
          onChange={setCustomHotkey}
          placeholder="Set custom hotkey"
          validationRules={ValidationPresets.custom({
            minKeys: 1,
            maxKeys: 3,
            requireModifier: false,
            requireModifierForMultiKey: false, // Only require for 3+ keys
          })}
          label="Custom Hotkey"
        />
      </div>
    </div>
  );
}