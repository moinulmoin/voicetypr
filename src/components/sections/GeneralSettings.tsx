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
        <div className="space-y-1">
          <Label htmlFor="hotkey" className="text-sm font-medium">Hotkey</Label>
          <p className="text-xs text-muted-foreground">
            Press anywhere
          </p>
        </div>
        <HotkeyInput
          value={settings.hotkey || ""}
          onChange={(hotkey) => onSettingsChange({ ...settings, hotkey })}
          placeholder="Click to set hotkey"
        />
      </div>

      {/* Output Setting */}
      <div className="flex items-center justify-between gap-4">
        <div className="space-y-1">
          <Label htmlFor="language" className="text-sm font-medium">Language</Label>
          <p className="text-xs text-muted-foreground">
            In which you speak
          </p>
        </div>
        <Select
          value={settings.language || "en"}
          onValueChange={(value) => onSettingsChange({ ...settings, language: value })}
        >
          <SelectTrigger id="language" className="w-64">
            <SelectValue placeholder="Select language" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="auto">Auto Detect</SelectItem>
            <SelectItem value="en">English</SelectItem>
            <SelectItem value="zh">Chinese</SelectItem>
            <SelectItem value="de">German</SelectItem>
            <SelectItem value="es">Spanish</SelectItem>
            <SelectItem value="ru">Russian</SelectItem>
            <SelectItem value="ko">Korean</SelectItem>
            <SelectItem value="fr">French</SelectItem>
            <SelectItem value="ja">Japanese</SelectItem>
            <SelectItem value="pt">Portuguese</SelectItem>
            <SelectItem value="tr">Turkish</SelectItem>
            <SelectItem value="pl">Polish</SelectItem>
            <SelectItem value="ca">Catalan</SelectItem>
            <SelectItem value="nl">Dutch</SelectItem>
            <SelectItem value="ar">Arabic</SelectItem>
            <SelectItem value="sv">Swedish</SelectItem>
            <SelectItem value="it">Italian</SelectItem>
            <SelectItem value="id">Indonesian</SelectItem>
            <SelectItem value="hi">Hindi</SelectItem>
            <SelectItem value="fi">Finnish</SelectItem>
            <SelectItem value="vi">Vietnamese</SelectItem>
            <SelectItem value="he">Hebrew</SelectItem>
            <SelectItem value="uk">Ukrainian</SelectItem>
            <SelectItem value="el">Greek</SelectItem>
            <SelectItem value="ms">Malay</SelectItem>
            <SelectItem value="cs">Czech</SelectItem>
            <SelectItem value="ro">Romanian</SelectItem>
            <SelectItem value="da">Danish</SelectItem>
            <SelectItem value="hu">Hungarian</SelectItem>
            <SelectItem value="ta">Tamil</SelectItem>
            <SelectItem value="no">Norwegian</SelectItem>
            <SelectItem value="th">Thai</SelectItem>
            <SelectItem value="ur">Urdu</SelectItem>
            <SelectItem value="hr">Croatian</SelectItem>
            <SelectItem value="bg">Bulgarian</SelectItem>
            <SelectItem value="lt">Lithuanian</SelectItem>
            <SelectItem value="la">Latin</SelectItem>
            <SelectItem value="mi">Maori</SelectItem>
            <SelectItem value="ml">Malayalam</SelectItem>
            <SelectItem value="cy">Welsh</SelectItem>
            <SelectItem value="sk">Slovak</SelectItem>
            <SelectItem value="te">Telugu</SelectItem>
            <SelectItem value="fa">Persian</SelectItem>
            <SelectItem value="lv">Latvian</SelectItem>
            <SelectItem value="bn">Bengali</SelectItem>
            <SelectItem value="sr">Serbian</SelectItem>
            <SelectItem value="az">Azerbaijani</SelectItem>
            <SelectItem value="sl">Slovenian</SelectItem>
            <SelectItem value="kn">Kannada</SelectItem>
            <SelectItem value="et">Estonian</SelectItem>
            <SelectItem value="mk">Macedonian</SelectItem>
            <SelectItem value="br">Breton</SelectItem>
            <SelectItem value="eu">Basque</SelectItem>
            <SelectItem value="is">Icelandic</SelectItem>
            <SelectItem value="hy">Armenian</SelectItem>
            <SelectItem value="ne">Nepali</SelectItem>
            <SelectItem value="mn">Mongolian</SelectItem>
            <SelectItem value="bs">Bosnian</SelectItem>
            <SelectItem value="kk">Kazakh</SelectItem>
            <SelectItem value="sq">Albanian</SelectItem>
            <SelectItem value="sw">Swahili</SelectItem>
            <SelectItem value="gl">Galician</SelectItem>
            <SelectItem value="mr">Marathi</SelectItem>
            <SelectItem value="pa">Punjabi</SelectItem>
            <SelectItem value="si">Sinhala</SelectItem>
            <SelectItem value="km">Khmer</SelectItem>
            <SelectItem value="sn">Shona</SelectItem>
            <SelectItem value="yo">Yoruba</SelectItem>
            <SelectItem value="so">Somali</SelectItem>
            <SelectItem value="af">Afrikaans</SelectItem>
            <SelectItem value="oc">Occitan</SelectItem>
            <SelectItem value="ka">Georgian</SelectItem>
            <SelectItem value="be">Belarusian</SelectItem>
            <SelectItem value="tg">Tajik</SelectItem>
            <SelectItem value="sd">Sindhi</SelectItem>
            <SelectItem value="gu">Gujarati</SelectItem>
            <SelectItem value="am">Amharic</SelectItem>
            <SelectItem value="yi">Yiddish</SelectItem>
            <SelectItem value="lo">Lao</SelectItem>
            <SelectItem value="uz">Uzbek</SelectItem>
            <SelectItem value="fo">Faroese</SelectItem>
            <SelectItem value="ht">Haitian Creole</SelectItem>
            <SelectItem value="ps">Pashto</SelectItem>
            <SelectItem value="tk">Turkmen</SelectItem>
            <SelectItem value="nn">Nynorsk</SelectItem>
            <SelectItem value="mt">Maltese</SelectItem>
            <SelectItem value="sa">Sanskrit</SelectItem>
            <SelectItem value="lb">Luxembourgish</SelectItem>
            <SelectItem value="my">Myanmar</SelectItem>
            <SelectItem value="bo">Tibetan</SelectItem>
            <SelectItem value="tl">Tagalog</SelectItem>
            <SelectItem value="mg">Malagasy</SelectItem>
            <SelectItem value="as">Assamese</SelectItem>
            <SelectItem value="tt">Tatar</SelectItem>
            <SelectItem value="haw">Hawaiian</SelectItem>
            <SelectItem value="ln">Lingala</SelectItem>
            <SelectItem value="ha">Hausa</SelectItem>
            <SelectItem value="ba">Bashkir</SelectItem>
            <SelectItem value="jw">Javanese</SelectItem>
            <SelectItem value="su">Sundanese</SelectItem>
            <SelectItem value="yue">Cantonese</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {/* Translation Setting - Show only when language is not English */}
      {settings.language !== 'en' && (
        <div className="flex items-center justify-between gap-4">
          <div className="space-y-1">
            <Label htmlFor="translate" className="text-sm font-medium">Translate to English</Label>
            <p className="text-xs text-muted-foreground">
              Translate transcriptions to English
            </p>
          </div>
          <Switch
            id="translate"
            checked={settings.translate_to_english || false}
            onCheckedChange={(checked) => onSettingsChange({ ...settings, translate_to_english: checked })}
          />
        </div>
      )}

      {/* Launch at Startup Setting */}
      <div className="flex items-center justify-between gap-4">
        <div className="space-y-1">
          <Label htmlFor="autostart" className="text-sm font-medium">Launch at startup</Label>
          <p className="text-xs text-muted-foreground">
            Start with your computer
          </p>
        </div>
        <Switch
          id="autostart"
          checked={autostartEnabled}
          onCheckedChange={handleAutostartToggle}
          disabled={autostartLoading}
        />
      </div>

      {/* Compact Recording Status Setting */}
      <div className="flex items-center justify-between gap-4">
        <div className="space-y-1">
          <Label htmlFor="compact-recording" className="text-sm font-medium">Compact recording status</Label>
          <p className="text-xs text-muted-foreground">
            Hide text labels in recording indicator
          </p>
        </div>
        <Switch
          id="compact-recording"
          checked={settings.compact_recording_status !== false}
          onCheckedChange={(checked) => onSettingsChange({ ...settings, compact_recording_status: checked })}
        />
      </div>

      {/* Tips Section */}
      <Alert className="mt-8">
        <Info className="h-4 w-4" />
        <AlertDescription className=" flex items-center">
          While recording, press <kbd className=" inline bg-accent px-1 ">esc</kbd> twice to cancel
        </AlertDescription>
      </Alert>
    </div>
  );
}