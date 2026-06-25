import { PermissionErrorBoundary } from "@/components/PermissionErrorBoundary";
import { TelemetrySection } from "./TelemetrySection";
import {
  SettingsCard,
  SettingsHeader,
  SettingsPage,
  SettingRow,
} from "@/components/settings/settings-ui";
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
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useReadiness } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { isMacOS } from "@/lib/platform";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import {
  CheckCircle,
  Keyboard,
  HelpCircle,
  Loader2,
  Mic,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  Trash2
} from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { createLogger } from "@/lib/logger";

const log = createLogger("advanced");

export function AdvancedSection() {
  const { updateSettings } = useSettings();
  const [isResetting, setIsResetting] = useState(false);
  const [isRequestingPermission, setIsRequestingPermission] = useState<string | null>(null);
  const [showAccessibility, setShowAccessibility] = useState(true);
  const {
    hasAccessibilityPermission,
    hasMicrophonePermission,
    isLoading,
    requestAccessibilityPermission,
    requestMicrophonePermission,
    checkAccessibilityPermission,
    checkMicrophonePermission
  } = useReadiness();

  useEffect(() => {
    setShowAccessibility(isMacOS);
  }, []);

  const handleRequestPermission = async (type: "microphone" | "accessibility") => {
    setIsRequestingPermission(type);
    try {
      if (type === "microphone") {
        await requestMicrophonePermission();
      } else {
        await requestAccessibilityPermission();
      }
    } finally {
      setIsRequestingPermission(null);
    }
  };

  const refresh = async () => {
    await Promise.all([
      checkAccessibilityPermission(),
      checkMicrophonePermission()
    ]);
  };

  const permissionData = [
    {
      type: "microphone" as const,
      icon: Mic,
      title: "Microphone",
      description: "To record your voice for transcription",
      status: hasMicrophonePermission ? "granted" : isLoading ? "checking" : "denied"
    },
    ...(showAccessibility ? [{
      type: "accessibility" as const,
      icon: Keyboard,
      title: "Accessibility",
      description: "For global hotkeys to trigger recording",
      status: hasAccessibilityPermission ? "granted" : isLoading ? "checking" : "denied"
    }] : [])
    // Automation permission removed for now
    // Can be re-enabled later if needed:
    // {
    //   type: "automation" as const,
    //   icon: TextCursor,
    //   title: "Automation",
    //   description: "To automatically paste transcribed text at cursor",
    //   status: permissions.automation
    // }
  ];


  const handleResetOnboarding = async () => {
    try {
      await updateSettings({
        onboarding_completed: false,
      });
      toast.success("Onboarding reset!");
      // No need to reload - the settings update will trigger the UI change
    } catch (error) {
      log.error('Failed to reset onboarding:', error);
      toast.error("Failed to reset onboarding");
    }
  };

  return (
    <PermissionErrorBoundary>
      <SettingsPage>
        <SettingsHeader
          title={
            <span className="flex items-center gap-2">
              Advanced
              <Dialog>
                <DialogTrigger asChild>
                  <Button type="button" variant="ghost" size="icon-sm" aria-label="Advanced guide" className="size-7 rounded-full text-muted-foreground">
                    <HelpCircle className="h-4 w-4" />
                  </Button>
                </DialogTrigger>
                <DialogContent className="sm:max-w-lg">
                  <DialogHeader>
                    <DialogTitle>Advanced guide</DialogTitle>
                    <DialogDescription>
                      Advanced settings are for permissions, onboarding recovery, and clearing app state.
                    </DialogDescription>
                  </DialogHeader>
                  <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                    <p><strong className="text-foreground">Permissions</strong> refreshes microphone and accessibility access after macOS changes.</p>
                    <p><strong className="text-foreground">Onboarding reset</strong> reruns setup without deleting your transcript history.</p>
                    <p><strong className="text-foreground">Factory reset</strong> is destructive and should only be used when you want to clear local app data.</p>
                  </div>
                </DialogContent>
              </Dialog>
            </span>
          }
          description="Permissions, diagnostics, and resetting the app."
        />

        {/* Permissions Section - Only show on macOS */}
        {showAccessibility && (
          <SettingsCard
            icon={ShieldCheck}
            title="Permissions"
            description="System access Voicetypr needs to record and trigger hotkeys."
            action={
              <TooltipProvider>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => refresh()}
                      disabled={isLoading}
                      className="h-8 px-2"
                    >
                      {isLoading ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <RefreshCw className="h-4 w-4" />
                      )}
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>
                    <p>Refresh permission status</p>
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
            }
          >
            {permissionData.map((perm) => (
              <SettingRow
                key={perm.type}
                title={
                  <span className="flex items-center gap-2.5">
                    <perm.icon className="h-4 w-4 text-muted-foreground" />
                    {perm.title}
                  </span>
                }
                description={perm.description}
              >
                {perm.status === "checking" ? (
                  <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
                ) : perm.status === "granted" ? (
                  <div className="flex items-center gap-1.5 text-green-600">
                    <CheckCircle className="h-4 w-4" />
                    <span className="text-sm">Granted</span>
                  </div>
                ) : (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => handleRequestPermission(perm.type)}
                    disabled={isRequestingPermission === perm.type}
                  >
                    {isRequestingPermission === perm.type ? (
                      <Loader2 className="h-3 w-3 animate-spin" />
                    ) : (
                      "Grant"
                    )}
                  </Button>
                )}
              </SettingRow>
            ))}

            {(hasMicrophonePermission === false || (showAccessibility && hasAccessibilityPermission === false)) && (
              <div className="mt-4 border-t border-border pt-4 text-xs text-muted-foreground space-y-1">
                <p className="font-medium">Missing permissions:</p>
                <ul className="list-disc list-inside space-y-0.5 ml-2">
                  {hasMicrophonePermission === false && <li>Microphone: Required for voice recording</li>}
                  {showAccessibility && hasAccessibilityPermission === false && <li>Accessibility: Required for global hotkeys</li>}
                </ul>
              </div>
            )}
          </SettingsCard>
        )}

        <TelemetrySection />

        {/* Reset Options Section */}
        <SettingsCard
          icon={RotateCcw}
          title="Reset options"
          description="Re-run setup or wipe Voicetypr back to a clean state."
        >
          <SettingRow
            title="Reset Onboarding"
            description="Re-run the initial setup wizard"
          >
            <Button
              variant="outline"
              size="sm"
              onClick={handleResetOnboarding}
            >
              <RefreshCw className="h-3 w-3" />
              Reset
            </Button>
          </SettingRow>

          <div className="mt-4 border-t border-border pt-4">
            <p className="text-[13.5px] font-semibold text-foreground mb-1">Reset App Data</p>
            <p className="text-[12.5px] text-muted-foreground mb-2">
              Completely reset Voicetypr to its initial state
            </p>
            <ul className="text-xs text-muted-foreground list-disc list-inside mb-3 space-y-0.5">
              <li>Delete all transcription history</li>
              <li>Remove all downloaded models</li>
              <li>Clear all settings and preferences</li>
              <li>Reset system permissions</li>
            </ul>
            <Button
              variant="destructive"
              size="sm"
              disabled={isResetting}
              onClick={async () => {
                const confirmed = await ask(
                  "This action cannot be undone. This will permanently delete all your Voicetypr data.\n\nThe app will restart after reset.\n\nAre you absolutely sure?",
                  {
                    title: "Reset App Data",
                    okLabel: "Reset Everything",
                    cancelLabel: "Cancel",
                    kind: "warning"
                  }
                );

                if (confirmed) {
                  setIsResetting(true);
                  try {
                    await invoke("reset_app_data");
                    toast.success("App data reset successfully. Restarting...");
                    setTimeout(() => {
                      relaunch();
                    }, 1000);
                  } catch (error) {
                    log.error("Failed to reset app data:", error);
                    toast.error("Failed to reset app data");
                    setIsResetting(false);
                  }
                }
              }}
              className="w-full"
            >
              {isResetting ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  Resetting...
                </>
              ) : (
                <>
                  <Trash2 className="mr-2 h-4 w-4" />
                  Reset App Data
                </>
              )}
            </Button>
          </div>
        </SettingsCard>
      </SettingsPage>
    </PermissionErrorBoundary>
  );
}
