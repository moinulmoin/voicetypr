import { HotkeyInput } from "@/components/HotkeyInput";
import { LanguageSelection } from "@/components/LanguageSelection";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useCanAutoInsert } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { AlertCircle, Globe, Mic, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";

export function GeneralSettings() {
  const { settings, updateSettings } = useSettings();
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [autostartLoading, setAutostartLoading] = useState(false);
  const canAutoInsert = useCanAutoInsert();

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
      await updateSettings({ launch_at_startup: checked });
    } catch (error) {
      console.error('Failed to toggle autostart:', error);
      // Revert the state if there was an error
      setAutostartEnabled(!checked);
    } finally {
      setAutostartLoading(false);
    }
  };

  return (
    <div className="h-full flex flex-col p-6">
      <div className="flex-shrink-0 mb-4 space-y-3">
        <h2 className="text-lg font-semibold">General Settings</h2>
        <p className="text-sm text-muted-foreground">
          Configure your recording preferences and app behavior
        </p>
      </div>

      <ScrollArea className="flex-1 min-h-0">
        <div className="space-y-6">
          {/* Recording Section */}
          <div className="rounded-lg border bg-card p-4 space-y-4">
            <div className="flex items-center gap-2 mb-2">
              <Mic className="h-4 w-4 text-muted-foreground" />
              <h3 className="text-sm font-medium">Recording</h3>
            </div>

            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <div>
                  <Label htmlFor="hotkey">Hotkey</Label>
                  <p className="text-xs text-muted-foreground mt-0.5">Global shortcut to start recording</p>
                </div>
                <HotkeyInput
                  value={settings.hotkey || ""}
                  onChange={(hotkey) => updateSettings({ hotkey })}
                  placeholder="Click to set"
                />
              </div>

              {!canAutoInsert && (
                <div className="flex items-center gap-2 p-2 text-sm text-amber-600 bg-amber-50 rounded-md">
                  <AlertCircle className="w-4 h-4 flex-shrink-0" />
                  <span>Grant accessibility permission in Advanced settings for hotkeys to work</span>
                </div>
              )}

              <div className="flex items-center justify-between">
                <div>
                  <Label htmlFor="compact-recording">Compact status</Label>
                  <p className="text-xs text-muted-foreground mt-0.5">Show minimal recording indicator</p>
                </div>
                <Switch
                  id="compact-recording"
                  checked={settings.compact_recording_status !== false}
                  onCheckedChange={async (checked) => await updateSettings({ compact_recording_status: checked })}
                />
              </div>
            </div>

            <div className="text-xs text-muted-foreground pt-2">
              💡 Tip: Press <kbd className="px-1 py-0.5 rounded text-xs bg-muted">ESC</kbd> twice while recording to cancel
            </div>
          </div>

          {/* Language Section */}
          <div className="rounded-lg border bg-card p-4 space-y-4">
            <div className="flex items-center gap-2 mb-2">
              <Globe className="h-4 w-4 text-muted-foreground" />
              <h3 className="text-sm font-medium">Language</h3>
            </div>

            <div className="flex items-center justify-between">
              <div>
                <Label htmlFor="language">Spoken language</Label>
                <p className="text-xs text-muted-foreground mt-0.5">Language you speak for transcription</p>
              </div>
              <LanguageSelection
                value={settings.language || "en"}
                onValueChange={(value) => updateSettings({ language: value })}
              />
            </div>

            {/* {settings.language !== 'en' && (
              <div className="flex items-center justify-between pl-4">
                <Label htmlFor="translate" className="text-sm">Translate to English</Label>
                <Switch
                  id="translate"
                  checked={settings.translate_to_english || false}
                  onCheckedChange={async (checked) => await updateSettings({ ...settings, translate_to_english: checked })}
                />
              </div>
            )} */}
          </div>

          {/* Startup Section */}
          <div className="rounded-lg border bg-card p-4 space-y-4">
            <div className="flex items-center gap-2 mb-2">
              <RefreshCw className="h-4 w-4 text-muted-foreground" />
              <h3 className="text-sm font-medium">Startup</h3>
            </div>

            <div className="flex items-center justify-between">
              <div>
                <Label htmlFor="autostart">Launch at startup</Label>
                <p className="text-xs text-muted-foreground mt-0.5">Start VoiceTypr when you log in</p>
              </div>
              <Switch
                id="autostart"
                checked={autostartEnabled}
                onCheckedChange={handleAutostartToggle}
                disabled={autostartLoading}
              />
            </div>
          </div>

        </div>
      </ScrollArea>
    </div>
  );
}