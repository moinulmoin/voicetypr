import { ResetSection } from "@/components/sections/ResetSection";
import { TelemetrySection } from "@/components/sections/TelemetrySection";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldTitle,
} from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { useSettings } from "@/contexts/SettingsContext";
import { normalizeTheme } from "@/hooks/useTheme";
import { createLogger } from "@/lib/logger";
import { updateService } from "@/services/updateService";
import type { UpdateChannel } from "@/types";
import {
  isStoreDistribution,
  type DistributionInfo,
} from "@/types/distribution";
import { invoke } from "@tauri-apps/api/core";
import { HelpCircle, RefreshCw, Rocket, Sun } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

const log = createLogger("settings");


export function GeneralSettings() {
  const { settings, updateSettings } = useSettings();
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [autostartLoading, setAutostartLoading] = useState(false);
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);
  const [updateDistribution, setUpdateDistribution] =
    useState<"loading" | "direct" | "store" | "error">("loading");
  const [isChangingUpdateChannel, setIsChangingUpdateChannel] = useState(false);

  useEffect(() => {
    const checkAutostart = async () => {
      try {
        const enabled = await invoke<boolean>("get_autostart_status");
        setAutostartEnabled(enabled);
      } catch (error) {
        log.error("Failed to check autostart status:", error);
      }
    };

    checkAutostart();
  }, []);

  useEffect(() => {
    invoke<DistributionInfo>("get_distribution_info")
      .then((info) =>
        setUpdateDistribution(isStoreDistribution(info) ? "store" : "direct"),
      )
      .catch((error) => {
        log.error("Failed to check update distribution:", error);
        setUpdateDistribution("error");
      });
  }, []);


  if (!settings) return null;

  const handleAutostartToggle = async (checked: boolean) => {
    setAutostartLoading(true);
    try {
      const actualState = await invoke<boolean>("set_autostart", {
        enabled: checked,
      });
      setAutostartEnabled(actualState);
      await updateSettings({ launch_at_startup: actualState });

      if (actualState !== checked) {
        toast.warning(
          `Autostart ${checked ? "enable" : "disable"} failed. Current state: ${actualState ? "enabled" : "disabled"}.`,
        );
      }
    } catch (error) {
      log.error("Failed to toggle autostart:", error);
    } finally {
      setAutostartLoading(false);
    }
  };

  const handleCheckUpdate = async () => {
    setIsCheckingUpdate(true);
    try {
      await updateService.checkForUpdatesManually();
    } finally {
      setIsCheckingUpdate(false);
    }
  };

  const handleUpdateChannelChange = async (value: string) => {
    if (value !== "stable" && value !== "beta") {
      toast.error("Invalid update channel");
      return;
    }

    setIsChangingUpdateChannel(true);
    try {
      await updateSettings({ update_channel: value });
      const channel: UpdateChannel = value;
      toast.success(
        channel === "beta" ? "Beta updates enabled" : "Stable updates enabled",
        {
          description:
            channel === "beta"
              ? "Checking for the latest beta. Beta builds may be less stable."
              : "Future checks use stable releases. Switching does not downgrade an installed beta.",
        },
      );
      await handleCheckUpdate();
    } catch (error) {
      log.error("Failed to change update channel:", error);
      toast.error("Failed to change update channel");
    } finally {
      setIsChangingUpdateChannel(false);
    }
  };

  return (
    <div className="h-full min-h-0 overflow-auto">
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-3.5 px-6 py-7 md:px-8">
        <div className="mb-1 flex flex-wrap items-start gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
              <Dialog>
                <DialogTrigger render={<Button type="button" variant="ghost" size="icon-sm" aria-label="Settings guide" className="size-7 rounded-full text-muted-foreground"/>}><HelpCircle className="h-4 w-4" /></DialogTrigger>
                <DialogContent className="sm:max-w-lg">
                  <DialogHeader>
                    <DialogTitle>Settings guide</DialogTitle>
                    <DialogDescription>
                      Manage global appearance, startup, privacy, and reset options.
                    </DialogDescription>
                  </DialogHeader>
                  <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                    <p><strong className="text-foreground">Appearance</strong> controls the app theme.</p>
                    <p><strong className="text-foreground">App behavior</strong> controls launch-at-login and update checks.</p>
                    <p><strong className="text-foreground">Privacy</strong> controls anonymous diagnostics and analytics.</p>
                    <p><strong className="text-foreground">Reset</strong> lets you repeat onboarding or erase app data.</p>
                  </div>
                </DialogContent>
              </Dialog>
            </div>
            <p className="mt-0.5 text-sm text-muted-foreground">
              Manage appearance, startup, privacy, and app-wide options.
            </p>
          </div>
        </div>
          <div className="rounded-2xl border border-border bg-card p-4">
            <div className="mb-4 flex items-center gap-2">
              <div className="rounded-md bg-sage-bg p-1.5">
                <Sun className="h-4 w-4 text-sage" />
              </div>
              <div>
                <h3 className="font-medium">Appearance</h3>
                <p className="text-xs text-muted-foreground">Light, dark, or follow your system</p>
              </div>
            </div>
            <FieldGroup className="gap-4">
              <Field orientation="responsive" className="items-center gap-4">
                <FieldContent>
                  <FieldTitle>Theme</FieldTitle>
                  <FieldDescription>Light, dark, or follow your system.</FieldDescription>
                </FieldContent>
                <Select
                  items={[{ value: "system", label: "System" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }]}
                      value={normalizeTheme(settings.theme)}
                  onValueChange={(value) => void updateSettings({ theme: normalizeTheme(value) })}
                >
                  <SelectTrigger className="w-full md:w-[190px]" aria-label="Theme">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="system">System</SelectItem>
                    <SelectItem value="light">Light</SelectItem>
                    <SelectItem value="dark">Dark</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
            </FieldGroup>
          </div>

          <div className="rounded-2xl border border-border bg-card p-4">
            <div className="mb-4 flex items-center gap-2">
              <div className="rounded-md bg-sage-bg p-1.5">
                <Rocket className="h-4 w-4 text-sage" />
              </div>
              <div>
                <h3 className="font-medium">App behavior</h3>
                <p className="text-xs text-muted-foreground">Startup and update defaults</p>
              </div>
            </div>
            <FieldGroup className="gap-4">
              <Field orientation="responsive" className="items-center gap-4">
                <FieldContent>
                  <FieldTitle>Launch at Startup</FieldTitle>
                  <FieldDescription>Start Voicetypr automatically after login.</FieldDescription>
                </FieldContent>
                <div className="flex items-center justify-end gap-2">
                  {autostartLoading && <Spinner className="h-4 w-4 text-muted-foreground" />}
                  <Switch
                    id="autostart"
                    checked={autostartEnabled}
                    onCheckedChange={handleAutostartToggle}
                    disabled={autostartLoading}
                  />
                </div>
              </Field>

              {updateDistribution === "store" ? (
                <Field>
                  <FieldContent>
                    <FieldTitle>Updates managed by Microsoft Store</FieldTitle>
                    <FieldDescription>
                      Update channels and installation are controlled by Microsoft Store.
                    </FieldDescription>
                  </FieldContent>
                </Field>
              ) : updateDistribution === "direct" ? (
                <>
                  <Field orientation="responsive" className="items-center gap-4">
                    <FieldContent>
                      <FieldTitle>Update channel</FieldTitle>
                      <FieldDescription>
                        Beta gets early builds for testing. Switching to Stable changes future checks and does not downgrade an installed beta.
                      </FieldDescription>
                    </FieldContent>
                    <Select
                      items={[{ value: "stable", label: "Stable" }, { value: "beta", label: "Beta" }]}
                      value={settings.update_channel ?? "stable"}
                      onValueChange={(value) => value != null && void handleUpdateChannelChange(value)}
                      disabled={isChangingUpdateChannel || isCheckingUpdate}
                    >
                      <SelectTrigger className="w-full md:w-[190px]" aria-label="Update channel">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="stable">Stable</SelectItem>
                        <SelectItem value="beta">Beta</SelectItem>
                      </SelectContent>
                    </Select>
                  </Field>

                  <Field orientation="responsive" className="items-center gap-4">
                    <FieldContent>
                      <FieldTitle>Check for updates automatically</FieldTitle>
                      <FieldDescription>
                        Check the selected channel daily and ask before downloading or installing anything.
                      </FieldDescription>
                    </FieldContent>
                    <div className="flex flex-col items-end gap-2">
                      <Switch
                        id="check-updates-automatically"
                        checked={settings.check_updates_automatically ?? true}
                        onCheckedChange={async (checked) =>
                          await updateSettings({
                            check_updates_automatically: checked,
                          })
                        }
                      />
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={handleCheckUpdate}
                        disabled={isCheckingUpdate}
                      >
                        <RefreshCw className={`h-3.5 w-3.5 ${isCheckingUpdate ? "animate-spin" : ""}`} />
                        {isCheckingUpdate ? "Checking" : "Check updates"}
                      </Button>
                    </div>
                  </Field>
                </>
              ) : (
                <Field>
                  <FieldContent>
                    <FieldTitle>
                      {updateDistribution === "loading"
                        ? "Loading update options"
                        : "Update options unavailable"}
                    </FieldTitle>
                    <FieldDescription>
                      {updateDistribution === "loading"
                        ? "Checking how this installation receives updates."
                        : "Could not verify this installation type. Restart Voicetypr to try again."}
                    </FieldDescription>
                  </FieldContent>
                </Field>
              )}
            </FieldGroup>
          </div>

          <TelemetrySection />
          <ResetSection />

        </div>
    </div>
  );
}
