import { PermissionErrorBoundary } from "@/components/PermissionErrorBoundary";
import { Button } from "@/components/ui/button";
import { usePermissions } from "@/hooks/usePermissions";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import {
  CheckCircle,
  Keyboard,
  Loader2,
  Mic,
  TextCursor,
  Trash2
} from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

export function AdvancedSection() {
  const [isResetting, setIsResetting] = useState(false);
  const {
    permissions,
    checkPermissions,
    requestPermission,
    isChecking,
    isRequesting,
    error,
    allGranted
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
    },
    {
      type: "automation" as const,
      icon: TextCursor,
      title: "Automation",
      description: "To automatically paste transcribed text at cursor",
      status: permissions.automation
    }
  ];


  return (
    <PermissionErrorBoundary>
      <div className="space-y-6 p-6">
      <div>
        <div className="space-y-1">
          <h2 className="text-2xl font-semibold flex items-center gap-2">

            Advanced
          </h2>
          <p className="text-sm text-muted-foreground">
            System permissions and advanced settings
          </p>
        </div>
      </div>

      <div className="space-y-6">
        <div>
          <h3 className="text-lg font-medium mb-4">System Permissions</h3>

          {error && (
            <div className="mb-4 text-sm text-red-600">
              Failed to check permissions. Please try again.
            </div>
          )}

          <div className="space-y-3">
            {permissionData.map((perm) => (
              <div
                key={perm.type}
                className="flex items-center justify-between py-3 px-4 rounded-lg border bg-card"
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
                      variant="ghost"
                      onClick={() => requestPermission(perm.type)}
                      disabled={isRequesting === perm.type}
                      className="text-xs"
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

        </div>

        {!allGranted && (
          <div className="text-sm text-muted-foreground space-y-2">
            <p>
              <strong>Note:</strong> Without all permissions, some features may not work properly.
            </p>
            <ul className="list-disc list-inside space-y-1 ml-2">
              <li>Microphone: Required for voice recording</li>
              <li>Accessibility: Required for global hotkeys</li>
              <li>Automation: Required for auto-paste at cursor</li>
            </ul>
          </div>
        )}

        <div className="pt-4The permission reset runs asynchronously, so it might still be executing when the app relaunches.">
          <h3 className="text-lg font-medium mb-4">Reset App</h3>
          <div className="space-y-4">
            <p className="text-sm text-muted-foreground">
              Reset VoiceTypr to its initial state. This will:
            </p>
            <ul className="text-sm text-muted-foreground list-disc list-inside ml-2 space-y-1">
              <li>Delete all transcription history</li>
              <li>Remove all downloaded models</li>
              <li>Clear all settings and preferences</li>
              <li>Remove saved window positions</li>
              <li>Clear all cached data and logs</li>
              <li>Reset system permissions (requires admin password)</li>
            </ul>
            <Button
              variant="destructive"
              size="sm"
              disabled={isResetting}
              onClick={async () => {
                const confirmed = await ask(
                  "This action cannot be undone. This will permanently delete all your VoiceTypr data including transcription history, downloaded models, and settings.\n\nYou will be prompted for your admin password to reset system permissions.\n\nThe app will restart after reset.\n\nAre you absolutely sure?",
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
                    // Wait a bit for the toast to show
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
      </div>
    </PermissionErrorBoundary>
  );
}