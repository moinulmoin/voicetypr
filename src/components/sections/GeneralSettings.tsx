import { HotkeyInput } from "@/components/HotkeyInput";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/select";
import { AppSettings } from "@/types";

interface GeneralSettingsProps {
  settings: AppSettings | null;
  onSettingsChange: (settings: AppSettings) => void;
}

export function GeneralSettings({ settings, onSettingsChange }: GeneralSettingsProps) {
  if (!settings) return null;

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
          className="w-64"
        />
      </div>

      {/* Language Setting */}
      <div className="flex items-center justify-between gap-4">
        <Label htmlFor="language" className="text-sm font-medium">Language</Label>
        <Select
          value={settings.language || "auto"}
          onValueChange={(value) => onSettingsChange({ ...settings, language: value })}
        >
          <SelectTrigger id="language" className="w-64">
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
    </div>
  );
}