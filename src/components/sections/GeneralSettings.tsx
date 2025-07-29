import { HotkeyInput } from "@/components/HotkeyInput";
import { Combobox } from "@/components/ui/combobox";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { languages } from "@/lib/languages";
import { AppSettings } from "@/types";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { useEffect, useState } from "react";

interface GeneralSettingsProps {
  settings: AppSettings | null;
  onSettingsChange: (settings: AppSettings) => void;
}

export function GeneralSettings({ settings, onSettingsChange }: GeneralSettingsProps) {
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [autostartLoading, setAutostartLoading] = useState(false);

  useEffect(() => {
    // Check if autostart is enabled on component mount
    const checkAutostart = async () => {
      try {
        const enabled = await isEnabled();
        setAutostartEnabled(enabled);
      } catch (error) {
        console.error('Failed to check autostart status:', error);
      }
    };
    checkAutostart();
  }, []);

  if (!settings) return null;

  const handleAutostartToggle = async (checked: boolean) => {
    setAutostartLoading(true);
    try {
      // Use the plugin API to enable/disable autostart
      if (checked) {
        await enable();
      } else {
        await disable();
      }
      setAutostartEnabled(checked);

      // Update settings to keep them in sync (backend is source of truth)
      onSettingsChange({ ...settings, launch_at_startup: checked });
    } catch (error) {
      console.error('Failed to toggle autostart:', error);
      // Revert the state if there was an error
      setAutostartEnabled(!checked);
    } finally {
      setAutostartLoading(false);
    }
  };

  return (
    <div className="p-6 space-y-6">
      <h2 className="text-lg font-semibold">General Settings</h2>

      {/* Recording Section */}
      <div className="space-y-4">
        <h3 className="text-sm font-medium text-muted-foreground">Recording</h3>

        <div className="flex items-center justify-between">
          <Label htmlFor="hotkey">Hotkey</Label>
          <HotkeyInput
            value={settings.hotkey || ""}
            onChange={(hotkey) => onSettingsChange({ ...settings, hotkey })}
            placeholder="Click to set"
          />
        </div>

        <div className="flex items-center justify-between">
          <Label htmlFor="compact-recording">Compact status</Label>
          <Switch
            id="compact-recording"
            checked={settings.compact_recording_status !== false}
            onCheckedChange={(checked) => onSettingsChange({ ...settings, compact_recording_status: checked })}
          />
        </div>
      </div>

      {/* Language Section */}
      <div className="space-y-4">
        <h3 className="text-sm font-medium text-muted-foreground">Language</h3>

        <div className="flex items-center justify-between">
          <Label htmlFor="language">Spoken language</Label>
          <Combobox
            options={languages}
            value={settings.language || "en"}
            onValueChange={(value) => onSettingsChange({ ...settings, language: value })}
            placeholder="Select language"
            searchPlaceholder="Search languages..."
            className="w-48"
          />
        </div>

        {settings.language !== 'en' && (
          <div className="flex items-center justify-between pl-4">
            <Label htmlFor="translate" className="text-sm">Translate to English</Label>
            <Switch
              id="translate"
              checked={settings.translate_to_english || false}
              onCheckedChange={(checked) => onSettingsChange({ ...settings, translate_to_english: checked })}
            />
          </div>
        )}
      </div>

      {/* System Section */}
      <div className="space-y-4">
        <h3 className="text-sm font-medium text-muted-foreground">System</h3>

        <div className="flex items-center justify-between">
          <Label htmlFor="autostart">Launch at startup</Label>
          <Switch
            id="autostart"
            checked={autostartEnabled}
            onCheckedChange={handleAutostartToggle}
            disabled={autostartLoading}
          />
        </div>
      </div>

      {/* Tip - Contextual to recording */}
      <div className="text-sm text-muted-foreground pt-4">
        Tip: Press <kbd className="px-1 py-0.5 rounded text-xs">ESC</kbd> twice while recording to cancel
      </div>
    </div>
  );
}