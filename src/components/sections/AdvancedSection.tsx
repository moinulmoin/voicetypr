import { PermissionErrorBoundary } from "@/components/PermissionErrorBoundary";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { usePermissions } from "@/hooks/usePermissions";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import {
  CheckCircle,
  Key,
  Keyboard,
  Loader2,
  Mic,
  RefreshCw,
  Trash2
} from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

export function AdvancedSection() {
  const [isResetting, setIsResetting] = useState(false);
  const {
    permissions,
    requestPermission,
    isRequesting,
    error
  } = usePermissions({
    checkOnMount: true,
    checkInterval: 0, // No auto-refresh, user can manually recheck
    showToasts: false // No toasts needed, we have visual indicators
  });

  const permissionData = [
    {
      type: "microphone" as const,
      icon: Mic,
      title: "Microphone",
      description: "To record your voice for transcription",
      status: permissions.microphone
    },
    {
      type: "accessibility" as const,
      icon: Keyboard,
      title: "Accessibility",
      description: "For global hotkeys to trigger recording",
      status: permissions.accessibility
    }
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
      const settings = await invoke<any>('get_settings');
      await invoke('save_settings', {
        settings: {
          ...settings,
          onboarding_completed: false,
        },
      });
      toast.success("Onboarding reset! Restarting the app.");
      setTimeout(() => {
        window.location.reload();
      }, 1000);
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
            {/* Permissions Section */}
            <div className="rounded-lg border bg-card p-4 space-y-4">
              <div className="flex items-center gap-2 mb-2">
                <Key className="h-4 w-4 text-muted-foreground" />
                <h3 className="text-sm font-medium">Permissions</h3>
              </div>

              {error && (
                <div className="text-sm text-red-600">
                  Failed to check permissions. Please try again.
                </div>
              )}

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
                          onClick={() => requestPermission(perm.type)}
                          disabled={isRequesting === perm.type}
                        >
                          {isRequesting === perm.type ? (
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

              {(permissions.microphone !== 'granted' || permissions.accessibility !== 'granted') && (
                <div className="text-xs text-muted-foreground space-y-1 pt-2">
                  <p className="font-medium">Missing permissions:</p>
                  <ul className="list-disc list-inside space-y-0.5 ml-2">
                    {permissions.microphone !== 'granted' && <li>Microphone: Required for voice recording</li>}
                    {permissions.accessibility !== 'granted' && <li>Accessibility: Required for global hotkeys</li>}
                  </ul>
                </div>
              )}
            </div>

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
            {/* <div className="rounded-lg border bg-card p-4 space-y-4">
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
                      toast.error("Failed to open logs folder");
                    }
                  }}
                  className="w-full justify-start"
                >
                  <FileText className="mr-2 h-4 w-4" />
                  View Logs
                </Button>

                <div className="text-xs text-muted-foreground">
                  <p>Logs location:</p>
                  <code className="text-xs bg-muted px-1 py-0.5 rounded">~/Library/Logs/com.voicetypr</code>
                </div>
              </div>
            </div> */}
          </div>
        </ScrollArea>
      </div>
    </PermissionErrorBoundary>
  );
}