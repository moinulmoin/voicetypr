import { HotkeyInput } from "@/components/HotkeyInput";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { AppSettings } from "@/types";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { Info } from "lucide-react";
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
      <h2 className="text-lg font-semibold mb-6">General Settings</h2>

      {/* Hotkey Setting */}
      <div className="flex items-center justify-between gap-4">
        <Label htmlFor="hotkey" className="text-sm font-medium">Hotkey</Label>
        <HotkeyInput
          value={settings.hotkey || ""}
          onChange={(hotkey) => onSettingsChange({ ...settings, hotkey })}
          placeholder="Click to set hotkey"
        />
      </div>

      {/* Output Setting */}
      <div className="flex items-center justify-between gap-4">
        <Label htmlFor="language" className="text-sm font-medium">Your language</Label>
        <Select
          value={settings.language || "en"}
          onValueChange={(value) => onSettingsChange({ ...settings, language: value })}
        >
          <SelectTrigger id="language" className="w-64">
            <SelectValue placeholder="Select language" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="en">English</SelectItem>
            <SelectItem value="es">Spanish</SelectItem>
            <SelectItem value="fr">French</SelectItem>
            <SelectItem value="de">German</SelectItem>
            <SelectItem value="it">Italian</SelectItem>
            <SelectItem value="pt">Portuguese</SelectItem>
            <SelectItem value="ru">Russian</SelectItem>
            <SelectItem value="ja">Japanese</SelectItem>
            <SelectItem value="ko">Korean</SelectItem>
            <SelectItem value="zh">Chinese</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {/* Launch at Startup Setting */}
      <div className="flex items-center justify-between gap-4">
        <Label htmlFor="autostart" className="text-sm font-medium">Launch at startup</Label>
        <Switch
          id="autostart"
          checked={autostartEnabled}
          onCheckedChange={handleAutostartToggle}
          disabled={autostartLoading}
        />
      </div>

      {/* Tips Section */}
      <Alert className="mt-8">
        <Info className="h-4 w-4" />
        <AlertDescription>
          <strong>Tips:</strong>
          <ul className="mt-1 space-y-1 text-sm">
            <li>• Use the hotkey to start recording from anywhere</li>
            <li>• While recording, press ESC twice to cancel</li>
          </ul>
        </AlertDescription>
      </Alert>
    </div>
  );
}