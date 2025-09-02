import { HotkeyInput } from "@/components/HotkeyInput";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Switch } from "@/components/ui/switch";
import { useCanAutoInsert } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { isMacOS } from "@/lib/platform";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { invoke } from "@tauri-apps/api/core";
import { AlertCircle, Globe, Mic, Rocket, Info } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { LanguageSelection } from "../LanguageSelection";
import { MicrophoneSelection } from "../MicrophoneSelection";

export function GeneralSettings() {
  const { settings, updateSettings } = useSettings();
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [autostartLoading, setAutostartLoading] = useState(false);
  const [showAccessibilityWarning, setShowAccessibilityWarning] = useState(true);
  const canAutoInsert = useCanAutoInsert();

  useEffect(() => {
    // Check if autostart is enabled on component mount
    const checkAutostart = async () => {
      try {
        const enabled = await isEnabled();
        setAutostartEnabled(enabled);
      } catch (error) {
        console.error("Failed to check autostart status:", error);
      }
    };
    checkAutostart();

    // Check platform for accessibility warning
    setShowAccessibilityWarning(isMacOS);
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
      console.error("Failed to toggle autostart:", error);
      // Revert the state if there was an error
      setAutostartEnabled(!checked);
    } finally {
      setAutostartLoading(false);
    }
  };

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="px-6 py-4 border-b border-border/40">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold">Settings</h1>
            <p className="text-sm text-muted-foreground mt-1">
              Configure your recording preferences and app behavior
            </p>
          </div>
        </div>
      </div>

      <ScrollArea className="flex-1">
        <div className="p-6 space-y-4">
          {/* Recording Section */}
          <div className="rounded-lg border border-border/50 bg-card">
            <div className="px-4 py-3 border-b border-border/50">
              <div className="flex items-center gap-2">
                <div className="p-1.5 rounded-md bg-primary/10">
                  <Mic className="h-4 w-4 text-primary" />
                </div>
                <div>
                  <h3 className="font-medium">Recording</h3>
                  <p className="text-xs text-muted-foreground">Voice capture and hotkey settings</p>
                </div>
              </div>
            </div>

            <div className="p-4 space-y-4">
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label htmlFor="hotkey" className="text-sm font-medium">Recording Hotkey</Label>
                  <p className="text-xs text-muted-foreground">
                    Global shortcut to start voice typing
                  </p>
                </div>
                <HotkeyInput
                  value={settings.hotkey || ""}
                  onChange={async (hotkey) => {
                    try {
                      // First update the global shortcut in the backend
                      await invoke("set_global_shortcut", { shortcut: hotkey });
                      // Then update the settings
                      await updateSettings({ hotkey });
                      toast.success("Hotkey updated successfully!");
                    } catch (err) {
                      console.error("Failed to update hotkey:", err);
                      toast.error("Failed to update hotkey. Please try a different combination.");
                    }
                  }}
                  placeholder="Click to set"
                />
              </div>

              {!canAutoInsert && showAccessibilityWarning && (
                <div className="flex items-start gap-2 p-3 rounded-lg bg-amber-500/10 border border-amber-500/20">
                  <AlertCircle className="w-4 h-4 flex-shrink-0 text-amber-600 dark:text-amber-500 mt-0.5" />
                  <div className="flex-1 space-y-1">
                    <p className="text-sm font-medium text-amber-900 dark:text-amber-400">
                      Accessibility permission required
                    </p>
                    <p className="text-xs text-amber-800 dark:text-amber-500">
                      Grant permission in Advanced settings for hotkeys to work system-wide
                    </p>
                  </div>
                </div>
              )}

              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label htmlFor="microphone" className="text-sm font-medium">Microphone</Label>
                  <p className="text-xs text-muted-foreground">
                    Select your preferred audio input device
                  </p>
                </div>
                <MicrophoneSelection
                  value={settings.selected_microphone || undefined}
                  onValueChange={async (deviceName) => {
                    try {
                      console.log(`Setting microphone to: ${deviceName || 'Default'}`)
                      // Always call set_audio_device, pass null for default
                      await invoke("set_audio_device", { deviceName: deviceName || null });
                      toast.success(`Microphone changed to: ${deviceName || 'Default'}`);
                    } catch (error) {
                      console.error("Failed to set microphone:", error);
                      toast.error("Failed to change microphone");
                    }
                  }}
                />
              </div>

              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label htmlFor="compact-recording" className="text-sm font-medium">Compact Status</Label>
                  <p className="text-xs text-muted-foreground">
                    Show minimal recording indicator window
                  </p>
                </div>
                <Switch
                  id="compact-recording"
                  checked={settings.compact_recording_status !== false}
                  onCheckedChange={async (checked) =>
                    await updateSettings({ compact_recording_status: checked })
                  }
                />
              </div>
            </div>

            <div className="px-4 pb-4">
              <div className="flex items-center gap-2 p-3 rounded-lg bg-primary/5 border border-primary/10">
                <Info className="h-4 w-4 text-primary" />
                <p className="text-xs text-muted-foreground">
                  Press <kbd className="px-1.5 py-0.5 rounded text-xs bg-background border">ESC</kbd> twice while recording to cancel
                </p>
              </div>
            </div>
          </div>

          {/* Language Section */}
          <div className="rounded-lg border border-border/50 bg-card p-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <div className="p-1.5 rounded-md bg-blue-500/10">
                  <Globe className="h-4 w-4 text-blue-500" />
                </div>
                <div className="space-y-0.5">
                  <Label htmlFor="language" className="text-sm font-medium">Spoken Language</Label>
                  <p className="text-xs text-muted-foreground">
                    The language you'll be speaking in
                  </p>
                </div>
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
          <div className="rounded-lg border border-border/50 bg-card p-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <div className="p-1.5 rounded-md bg-green-500/10">
                  <Rocket className="h-4 w-4 text-green-500" />
                </div>
                <div className="space-y-0.5">
                  <Label htmlFor="autostart" className="text-sm font-medium">Launch at Startup</Label>
                  <p className="text-xs text-muted-foreground">
                    Automatically start VoiceTypr when you log in
                  </p>
                </div>
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