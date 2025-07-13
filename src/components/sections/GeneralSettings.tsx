import { HotkeyInput } from "@/components/HotkeyInput";
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

interface GeneralSettingsProps {
  settings: AppSettings | null;
  onSettingsChange: (settings: AppSettings) => void;
}

export function GeneralSettings({ settings, onSettingsChange }: GeneralSettingsProps) {
  if (!settings) return null;

  return (
    <div className="p-6 space-y-6">
      <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4">General Settings</h2>
      
      {/* Hotkey Setting */}
      <div className="space-y-2">
        <Label htmlFor="hotkey">Hotkey</Label>
        <HotkeyInput
          value={settings.hotkey || ""}
          onChange={(hotkey) => onSettingsChange({ ...settings, hotkey })}
          placeholder="Click to set hotkey"
        />
      </div>

      {/* Language Setting */}
      <div className="space-y-2">
        <Label htmlFor="language">Language</Label>
        <Select
          value={settings.language || "auto"}
          onValueChange={(value) => onSettingsChange({ ...settings, language: value })}
        >
          <SelectTrigger id="language">
            <SelectValue placeholder="Select language" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="auto">Auto-detect</SelectItem>
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

      {/* Auto Insert Toggle */}
      <div className="flex items-center justify-between">
        <Label htmlFor="auto-insert" className="flex-1">
          Auto-insert text at cursor
        </Label>
        <Switch
          id="auto-insert"
          checked={settings.auto_insert || false}
          onCheckedChange={(checked) =>
            onSettingsChange({ ...settings, auto_insert: checked })
          }
        />
      </div>

      {/* Show Window on Record Toggle */}
      <div className="flex items-center justify-between">
        <Label htmlFor="show-window" className="flex-1">
          Show window when recording
        </Label>
        <Switch
          id="show-window"
          checked={settings.show_window_on_record || false}
          onCheckedChange={(checked) =>
            onSettingsChange({ ...settings, show_window_on_record: checked })
          }
        />
      </div>

      {/* Show Pill Widget Toggle */}
      <div className="flex items-center justify-between">
        <Label htmlFor="show-pill" className="flex-1">
          Show floating pill when recording
        </Label>
        <Switch
          id="show-pill"
          checked={settings.show_pill_widget ?? true}
          onCheckedChange={(checked) =>
            onSettingsChange({ ...settings, show_pill_widget: checked })
          }
        />
      </div>
    </div>
  );
}