import { PermissionErrorBoundary } from "@/components/PermissionErrorBoundary";
import { Button } from "@/components/ui/button";
import { usePermissions } from "@/hooks/usePermissions";
import {
  CheckCircle,
  Keyboard,
  Loader2,
  Mic,
  TextCursor
} from "lucide-react";

export function AdvancedSection() {
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
      <div className="flex items-center justify-between">
        <div className="space-y-1">
          <h2 className="text-2xl font-semibold flex items-center gap-2">

            Advanced
          </h2>
          <p className="text-sm text-muted-foreground">
            System permissions and advanced settings
          </p>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={checkPermissions}
          disabled={isChecking}
        >
          {isChecking ? (
            <>
              Checking...
            </>
          ) : (
            <>
              Recheck
            </>
          )}
        </Button>
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

          {allGranted && (
            <p className="mt-4 text-sm text-green-600">
              ✓ All permissions granted
            </p>
          )}
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
      </div>
      </div>
    </PermissionErrorBoundary>
  );
}