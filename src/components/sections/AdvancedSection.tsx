import { PermissionErrorBoundary } from "@/components/PermissionErrorBoundary";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
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
  FileText,
  HelpCircle,
  Key,
  Keyboard,
  Loader2,
  Mic,
  RefreshCw,
  Trash2
} from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

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
      console.error('Failed to reset onboarding:', error);
      toast.error("Failed to reset onboarding");
    }
  };

  return (
    <PermissionErrorBoundary>
      <div className="h-full flex flex-col p-6">
        <div className="flex-shrink-0 mb-4 space-y-3">
          <h2 className="text-lg font-semibold">Advanced Settings</h2>
          <p className="text-sm text-muted-foreground">
            System permissions and app management
          </p>
        </div>

        <ScrollArea className="flex-1 min-h-0">
          <div className="space-y-6">
            {/* Permissions Section - Only show on macOS */}
            {showAccessibility && (
              <div className="rounded-lg border bg-card p-4 space-y-4">
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2">
                    <Key className="h-4 w-4 text-muted-foreground" />
                    <h3 className="text-sm font-medium">Permissions</h3>
                  </div>
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
                </div>


                <div className="space-y-3">
                  {permissionData.map((perm) => (
                    <div
                      key={perm.type}
                      className="flex items-center justify-between"
                    >
                      <div className="flex items-center gap-3">
                        <perm.icon className="h-4 w-4 text-muted-foreground" />
                        <div>
                          <p className="text-sm font-medium">{perm.title}</p>
                          <p className="text-xs text-muted-foreground">
                            {perm.description}
                          </p>
                        </div>
                      </div>

                      <div className="flex items-center">
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
                      </div>
                    </div>
                  ))}
                </div>

                {(hasMicrophonePermission === false || (showAccessibility && hasAccessibilityPermission === false)) && (
                  <div className="text-xs text-muted-foreground space-y-1 pt-2">
                    <p className="font-medium">Missing permissions:</p>
                    <ul className="list-disc list-inside space-y-0.5 ml-2">
                      {hasMicrophonePermission === false && <li>Microphone: Required for voice recording</li>}
                      {showAccessibility && hasAccessibilityPermission === false && <li>Accessibility: Required for global hotkeys</li>}
                    </ul>
                  </div>
                )}
              </div>
            )}

            {/* Reset Options Section */}
            <div className="rounded-lg border bg-card p-4 space-y-4">
              <div className="flex items-center gap-2 mb-2">
                <RefreshCw className="h-4 w-4 text-muted-foreground" />
                <h3 className="text-sm font-medium">Reset Options</h3>
              </div>

              <div className="space-y-4">
                <div>
                  <p className="text-sm font-medium mb-1">Reset Onboarding</p>
                  <p className="text-xs text-muted-foreground mb-3">
                    Re-run the initial setup wizard
                  </p>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleResetOnboarding}
                    className="w-full"
                  >
                    <RefreshCw className="mr-2 h-4 w-4" />
                    Reset Onboarding
                  </Button>
                </div>

                <div className="pt-3">
                  <p className="text-sm font-medium mb-1">Reset App Data</p>
                  <p className="text-xs text-muted-foreground mb-2">
                    Completely reset VoiceTypr to its initial state
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
                        "This action cannot be undone. This will permanently delete all your VoiceTypr data.\n\nThe app will restart after reset.\n\nAre you absolutely sure?",
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
                          console.error("Failed to reset app data:", error);
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
              </div>
            </div>

            {/* Diagnostics Section */}
            <div className="rounded-lg border bg-card p-4 space-y-4">
              <div className="flex items-center gap-2 mb-2">
                <HelpCircle className="h-4 w-4 text-muted-foreground" />
                <h3 className="text-sm font-medium">Diagnostics</h3>
              </div>

              <div className="space-y-3">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={async () => {
                    try {
                      await invoke("open_logs_folder");
                    } catch (error) {
                      console.error("Failed to open logs folder:", error);
                      toast.error("Failed to open logs folder");
                    }
                  }}
                  className="w-full justify-start"
                >
                  <FileText className="mr-2 h-4 w-4" />
                  Open Logs Folder
                </Button>

                <div className="text-xs text-muted-foreground space-y-1">
                  <p>• Debug logs are saved automatically with daily rotation</p>
                  <p>• Logs older than 30 days are automatically deleted</p>
                  <p>• Share logs when reporting issues</p>
                </div>
              </div>
            </div>
          </div>
        </ScrollArea>
      </div>
    </PermissionErrorBoundary>
  );
}