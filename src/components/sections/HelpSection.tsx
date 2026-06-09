import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import {
  ChevronDown,
  Mail,
  Mic,
  Keyboard,
  Type,
  Download,
  Copy,
  FileText,
  Globe,
  Monitor,
} from "lucide-react";
import XIcon from "@/components/icons/XIcon";
import { useState, useEffect, useCallback, useMemo, useRef, type ComponentType } from "react";
import { toast } from "sonner";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { platform, version as osVersion } from "@tauri-apps/plugin-os";
import { open } from "@tauri-apps/plugin-shell";
import { useSettings } from "@/contexts/SettingsContext";
import { useReadiness } from "@/contexts/ReadinessContext";
import { ReportBugDialog } from "@/components/ReportBugDialog";
import {
  formatHotkeyDiagnosticContext,
  formatHotkeyDiagnosticLines,
  type HotkeyDiagnostics,
} from "@/utils/hotkeyDiagnostics";

interface QuickFix {
  id: string;
  title: string;
  icon: ComponentType<{ className?: string }>;
  issue: string;
  solution: string;
  checkStatus?: () => boolean;
}

type DiagnosticsStatus = "Ready" | "Needs attention" | "Not checked";

interface ReadinessSnapshot {
  has_accessibility_permission: boolean | null;
  has_microphone_permission: boolean | null;
  has_models: boolean | null;
  selected_model_available: boolean | null;
}

interface AccelerationStatus {
  mode: string;
  effective_backend: string;
  gpu_available: boolean | null;
  message: string;
  diagnostic_code: string;
  recommended_action: string;
  last_error?: string | null;
}

interface DiagnosticsSummary {
  status: DiagnosticsStatus;
  issue: string;
  lastChecked: "Just now" | "Not checked";
}

const HOTKEY_TEST_POLL_MS = 500;
const HOTKEY_TEST_TIMEOUT_MS = 10_000;
const HOTKEY_ACTION_TIMEOUT_MS = 2_000;

function buildSystemDiagnostics(params: {
  appVer: string;
  os: string;
  osVer: string;
  deviceId: string;
  model: string;
  canRecord: boolean;
  canAutoInsert: boolean;
  hotkeyDiag: HotkeyDiagnostics | null;
  accelerationStatus: AccelerationStatus | null;
}): string {
  const {
    appVer,
    os,
    osVer,
    deviceId,
    model,
    canRecord,
    canAutoInsert,
    hotkeyDiag,
    accelerationStatus,
  } = params;
  const lines: string[] = [
    `App Version: ${appVer}`,
    `OS: ${os} ${osVer}`,
    `Device ID: ${deviceId}`,
    `Model: ${model}`,
    `Acceleration Mode: ${accelerationStatus?.mode ?? "Unknown"}`,
    `Acceleration Backend: ${accelerationStatus?.effective_backend ?? "Unknown"}`,
    `GPU Available: ${
      accelerationStatus?.gpu_available === null || accelerationStatus?.gpu_available === undefined
        ? "Unknown"
        : String(accelerationStatus.gpu_available)
    }`,
    `GPU Diagnostic: ${accelerationStatus?.diagnostic_code ?? "not_checked"}`,
    `GPU Recommended Action: ${accelerationStatus?.recommended_action ?? "none"}`,
  ];

  if (accelerationStatus?.last_error) {
    lines.push(`GPU Last Error: ${accelerationStatus.last_error}`);
  }

  if (os !== "windows") {
    lines.push(
      `Microphone Permission: ${canRecord ? "Granted" : "Not granted"}`,
      `Accessibility Permission: ${canAutoInsert ? "Granted" : "Not granted"}`
    );
  }

  lines.push(...formatHotkeyDiagnosticLines(hotkeyDiag));
  return lines.join("\n");
}

function getDiagnosticsGuidance(): string {
  return "Readiness checks use permissions, model availability, and shortcut registration. Test Hotkey confirms the actual shortcut press is detected.";
}

function getDiagnosticsSummary(params: {
  canRecord: boolean;
  canAutoInsert: boolean;
  readiness: ReadinessSnapshot | null;
  hotkeyDiag: HotkeyDiagnostics | null;
  hotkeyTestIssue: string | null;
  checkedAt: string | null;
}): DiagnosticsSummary {
  const { canRecord, canAutoInsert, readiness, hotkeyDiag, hotkeyTestIssue, checkedAt } = params;
  const lastChecked = checkedAt ? "Just now" : "Not checked";

  if (!checkedAt) {
    return {
      status: "Not checked",
      issue: "Diagnostics are still loading",
      lastChecked,
    };
  }

  if (readiness?.has_microphone_permission === false) {
    return {
      status: "Needs attention",
      issue: "Microphone permission is missing",
      lastChecked,
    };
  }

  if (readiness?.has_models === false) {
    return {
      status: "Needs attention",
      issue: "No transcription model is installed",
      lastChecked,
    };
  }

  if (readiness?.selected_model_available === false) {
    return {
      status: "Needs attention",
      issue: "Selected transcription model is unavailable",
      lastChecked,
    };
  }

  if (!canRecord) {
    return {
      status: "Needs attention",
      issue: "Recording pipeline is not ready",
      lastChecked,
    };
  }

  if (readiness?.has_accessibility_permission === false || !canAutoInsert) {
    return {
      status: "Needs attention",
      issue: "Accessibility permission is missing",
      lastChecked,
    };
  }

  if (!hotkeyDiag) {
    return {
      status: "Needs attention",
      issue: "Shortcut diagnostics could not be loaded",
      lastChecked,
    };
  }

  if (hotkeyDiag.registrationStatus === "failed" || hotkeyDiag.isRegistered === false) {
    return {
      status: "Needs attention",
      issue: "Global hotkey is not registered",
      lastChecked,
    };
  }

  if (hotkeyDiag.registrationStatus === "restored_after_failure") {
    return {
      status: "Needs attention",
      issue: hotkeyDiag.registrationError || "Previous hotkey restored after update failure",
      lastChecked,
    };
  }

  if (hotkeyTestIssue) {
    return {
      status: "Needs attention",
      issue: hotkeyTestIssue,
      lastChecked,
    };
  }

  return {
    status: "Ready",
    issue: "No readiness issue detected",
    lastChecked,
  };
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

function shouldCancelRecordingAfterHotkeyTest(
  baseline: HotkeyDiagnostics | null,
  current: HotkeyDiagnostics
): boolean {
  const baselineState = baseline?.currentRecordingState;
  const currentState = current.currentRecordingState;

  if (baselineState !== "idle" && baselineState !== "error") {
    return false;
  }

  return Boolean(currentState && currentState !== "idle" && currentState !== "error");
}

async function cancelRecordingStartedByHotkeyTest(
  baseline: HotkeyDiagnostics | null,
  current: HotkeyDiagnostics,
  isCancelled: () => boolean
): Promise<void> {
  if (!shouldCancelRecordingAfterHotkeyTest(baseline, current) || isCancelled()) {
    return;
  }

  try {
    await invoke("cancel_recording");
  } catch (error) {
    if (!isCancelled()) {
      console.error("Failed to clean up recording started by hotkey test:", error);
      toast.error("Stop recording manually");
    }
  }
}

export function HelpSection() {
  const [appVersion, setAppVersion] = useState<string>("");
  const [platformName, setPlatformName] = useState<string>("");
  const [osPlatform, setOsPlatform] = useState<string>("");
  const [osVersionText, setOsVersionText] = useState<string>("");
  const [deviceId, setDeviceId] = useState<string>("Unknown");
  const [openItems, setOpenItems] = useState<string[]>([]);
  const [hotkeyDiagnostics, setHotkeyDiagnostics] = useState<HotkeyDiagnostics | null>(null);
  const [accelerationStatus, setAccelerationStatus] = useState<AccelerationStatus | null>(null);
  const [testingHotkey, setTestingHotkey] = useState(false);
  const [hotkeyTestIssue, setHotkeyTestIssue] = useState<string | null>(null);
  const [lastDiagnosticsCheckAt, setLastDiagnosticsCheckAt] = useState<string | null>(null);
  const [showEmailModal, setShowEmailModal] = useState(false);
  const [emailSubject, setEmailSubject] = useState<string>("");
  const [emailBody, setEmailBody] = useState<string>("");
  const [showReportBugDialog, setShowReportBugDialog] = useState(false);
  const [reportBugInitialMessage, setReportBugInitialMessage] = useState<string | undefined>();
  const [reportBugDiagnosticContext, setReportBugDiagnosticContext] = useState<string | undefined>();
  const { settings } = useSettings();
  const readiness = useReadiness();
  const canRecord = readiness.canRecord;
  const canAutoInsert = readiness.canAutoInsert;
  const readinessState: ReadinessSnapshot = {
    has_accessibility_permission: readiness.hasAccessibilityPermission,
    has_microphone_permission: readiness.hasMicrophonePermission,
    has_models: readiness.hasModels,
    selected_model_available: readiness.selectedModelAvailable,
  };
  const hotkeyTestRunRef = useRef(0);

  useEffect(() => {
    return () => {
      hotkeyTestRunRef.current += 1;
    };
  }, []);

  const loadHotkeyDiagnostics = useCallback(async (): Promise<HotkeyDiagnostics | null> => {
    try {
      const diag = await invoke<HotkeyDiagnostics>("get_hotkey_diagnostics");
      setHotkeyDiagnostics(diag);
      return diag;
    } catch (error) {
      console.error("Failed to get hotkey diagnostics:", error);
      setHotkeyDiagnostics(null);
      return null;
    }
  }, []);

  useEffect(() => {
    const fetchSystemInfo = async () => {
      try {
        const [appVer, os, osVer, deviceId, hotkeyDiag, accelerationDiag] = await Promise.all([
          getVersion(),
          platform(),
          osVersion(),
          invoke<string>("get_device_id").catch(() => "Unknown"),
          invoke<HotkeyDiagnostics>("get_hotkey_diagnostics").catch(() => null),
          invoke<AccelerationStatus>("get_transcription_acceleration_status").catch(() => null),
        ]);

        setAppVersion(appVer);
        setOsPlatform(os);
        setOsVersionText(osVer);
        setDeviceId(deviceId);
        setPlatformName(`${os} ${osVer}`);
        setHotkeyDiagnostics(hotkeyDiag);
        setAccelerationStatus(accelerationDiag);
        setLastDiagnosticsCheckAt(new Date().toISOString());
      } catch (error) {
        console.error("Failed to get system info:", error);
      }
    };

    void fetchSystemInfo();
  }, [settings, canRecord, canAutoInsert]);

  const quickFixes: QuickFix[] = [
    {
      id: "recording",
      title: "Recording not working",
      icon: Mic,
      issue: "Voice recording doesn't start when using hotkey",
      solution:
        "Go to Advanced section and check if Microphone permission is granted. Also check Settings to ensure a recording device is selected.",
      checkStatus: () => canRecord,
    },
    {
      id: "hotkey",
      title: "Hotkey not responding",
      icon: Keyboard,
      issue: "Global hotkey doesn't trigger recording",
      solution:
        osPlatform === "windows"
          ? "Use Test Hotkey to confirm Windows detects the shortcut. If it fails, try another combination and allow VoiceTypr in your security or antivirus software."
          : osPlatform === "macos"
            ? "Use Test Hotkey to confirm macOS detects the shortcut. If it fails, try another combination and check macOS Keyboard Shortcuts for conflicts."
            : "Use Test Hotkey to confirm shortcut input.",
      checkStatus: () =>
        hotkeyDiagnostics?.isRegistered === true ||
        hotkeyDiagnostics?.registrationStatus === "registered",
    },
    {
      id: "insertion",
      title: "Text not inserting",
      icon: Type,
      issue: "Transcribed text doesn't appear at cursor",
      solution:
        "Make sure your cursor is in an active text field. Accessibility permission must be granted in Advanced section.",
      checkStatus: () => canAutoInsert,
    },
    {
      id: "download",
      title: "Model download stuck",
      icon: Download,
      issue: "Whisper model download not progressing",
      solution:
        "Go to Models section, cancel the current download and try again. Check your internet connection.",
    },
  ];

  const toggleItem = (itemId: string) => {
    setOpenItems((prev) =>
      prev.includes(itemId) ? prev.filter((id) => id !== itemId) : [...prev, itemId]
    );
  };

  const handleEmailSupport = () => {
    const subject = "VoiceTypr Support Request";
    const body = `
${diagnostics}

Issue Description:
[Please describe your issue here]

Steps to reproduce:
1. 
2. 
3. 

Expected behavior:


Actual behavior:

`;

    setEmailSubject(subject);
    setEmailBody(body);
    setShowEmailModal(true);
  };

  const handleOpenInGmail = async () => {
    const gmailUrl = `https://mail.google.com/mail/?view=cm&fs=1&to=support@voicetypr.com&su=${encodeURIComponent(emailSubject)}&body=${encodeURIComponent(emailBody)}`;
    try {
      await open(gmailUrl);
      setShowEmailModal(false);
      toast.success("Opening Gmail in browser");
    } catch (error) {
      console.error("Failed to open Gmail:", error);
      toast.error("Failed to open Gmail");
    }
  };

  const handleOpenInDefaultClient = async () => {
    const mailtoUrl = `mailto:support@voicetypr.com?subject=${encodeURIComponent(emailSubject)}&body=${encodeURIComponent(emailBody)}`;
    try {
      await open(mailtoUrl);
      setShowEmailModal(false);
      toast.success("Opening default email client");
    } catch (error) {
      console.error("Failed to open email client:", error);
      toast.error("Failed to open email client");
    }
  };

  const handleXSupport = async () => {
    const xUrl = "https://x.com/voicetypr";
    try {
      await open(xUrl);
    } catch (error) {
      console.error("Failed to open X profile:", error);
      toast.error("Failed to open X profile");
    }
  };

  const handleCopySystemInfo = async () => {
    try {
      await navigator.clipboard.writeText(diagnostics);
      toast.success("System info copied to clipboard");
    } catch (error) {
      console.error("Failed to copy system info:", error);
      toast.error("Failed to copy system info");
    }
  };

  const handleOpenLogs = async () => {
    try {
      await invoke("open_logs_folder");
      toast.info("Please attach the latest log file to your support message");
    } catch (error) {
      console.error("Failed to open logs folder:", error);
      toast.error("Failed to open logs folder");
    }
  };

  const handleTestHotkey = async () => {
    if (testingHotkey) return;

    const runId = hotkeyTestRunRef.current + 1;
    hotkeyTestRunRef.current = runId;
    const isCancelled = () => hotkeyTestRunRef.current !== runId;

    setTestingHotkey(true);
    setHotkeyTestIssue(null);
    setLastDiagnosticsCheckAt(new Date().toISOString());
    toast.info("Press your hotkey now");

    try {
      const baseline = await loadHotkeyDiagnostics();
      if (isCancelled()) return;

      if (!baseline) {
        setHotkeyTestIssue("Shortcut diagnostics unavailable");
        setLastDiagnosticsCheckAt(new Date().toISOString());
        toast.error("Shortcut diagnostics unavailable");
        return;
      }

      const startCount = baseline.eventCount;
      const deadline = Date.now() + HOTKEY_TEST_TIMEOUT_MS;

      while (Date.now() < deadline) {
        if (isCancelled()) return;

        await sleep(HOTKEY_TEST_POLL_MS);
        if (isCancelled()) return;

        const current = await invoke<HotkeyDiagnostics>("get_hotkey_diagnostics");
        if (isCancelled()) return;

        setHotkeyDiagnostics(current);
        setLastDiagnosticsCheckAt(new Date().toISOString());

        if (current.eventCount > startCount) {
          let latest = current;
          const baselineState = baseline?.currentRecordingState;
          const shouldVerifyRecordingStart = baselineState === "idle" || baselineState === "error";

          if (shouldVerifyRecordingStart) {
            const actionDeadline = Date.now() + HOTKEY_ACTION_TIMEOUT_MS;

            while (
              latest.currentRecordingState !== "error" &&
              !shouldCancelRecordingAfterHotkeyTest(baseline, latest) &&
              Date.now() < actionDeadline
            ) {
              await sleep(HOTKEY_TEST_POLL_MS);
              if (isCancelled()) return;

              latest = await invoke<HotkeyDiagnostics>("get_hotkey_diagnostics");
              if (isCancelled()) return;

              setHotkeyDiagnostics(latest);
              setLastDiagnosticsCheckAt(new Date().toISOString());
            }

            if (!shouldCancelRecordingAfterHotkeyTest(baseline, latest)) {
              if (!isCancelled()) {
                setHotkeyTestIssue("Hotkey detected, but recording did not start");
                setLastDiagnosticsCheckAt(new Date().toISOString());
                toast.error("Hotkey detected, but recording did not start");
              }
              return;
            }

            await cancelRecordingStartedByHotkeyTest(baseline, latest, isCancelled);
          }
          if (!isCancelled()) {
            setHotkeyTestIssue(null);
            toast.success("Hotkey detected");
          }
          return;
        }
      }

      if (!isCancelled()) {
        setHotkeyTestIssue("Hotkey press was not detected");
        setLastDiagnosticsCheckAt(new Date().toISOString());
        toast.error("No hotkey was detected");
      }
    } catch (error) {
      if (!isCancelled()) {
        console.error("Failed to test hotkey:", error);
        setHotkeyTestIssue("Hotkey test failed to run");
        setLastDiagnosticsCheckAt(new Date().toISOString());
        toast.error("Failed to test hotkey");
      }
    } finally {
      if (!isCancelled()) {
        setTestingHotkey(false);
      }
    }
  };

  const handleReportHotkeyIssue = async () => {
    const diag = await loadHotkeyDiagnostics();

    setReportBugInitialMessage(
      "I'm having trouble with my global hotkey.\n\nWhat I tried:\n\nWhat I expected:\n\nWhat happened instead:\n"
    );
    setReportBugDiagnosticContext(
      diag ? formatHotkeyDiagnosticContext(diag) : "Hotkey diagnostics unavailable."
    );
    setShowReportBugDialog(true);
  };

  const diagnosticsSummary = getDiagnosticsSummary({
    canRecord,
    canAutoInsert,
    readiness: readinessState,
    hotkeyDiag: hotkeyDiagnostics,
    hotkeyTestIssue,
    checkedAt: lastDiagnosticsCheckAt,
  });

  const diagnostics = useMemo(
    () =>
      buildSystemDiagnostics({
        appVer: appVersion || "Unknown",
        os: osPlatform || "unknown",
        osVer: osVersionText || "Unknown",
        deviceId,
        model: settings?.current_model || "None selected",
        canRecord,
        canAutoInsert,
        hotkeyDiag: hotkeyDiagnostics,
        accelerationStatus,
      }),
    [
      appVersion,
      osPlatform,
      osVersionText,
      deviceId,
      settings?.current_model,
      canRecord,
      canAutoInsert,
      hotkeyDiagnostics,
      accelerationStatus,
    ],
  );

  return (
    <div className="h-full min-h-0 flex flex-col">
      <div className="shrink-0 px-6 py-4 border-b border-border/40">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold">Help & Support</h1>
            <p className="text-sm text-muted-foreground mt-1">
              Quick fixes and support resources
            </p>
          </div>
        </div>
      </div>

      <ScrollArea className="flex-1 min-h-0">
        <div className="p-6 space-y-6">
          <div className="space-y-4">
            <h2 className="text-base font-semibold">Diagnostics</h2>

            <div className="rounded-lg border border-border/50 bg-card p-4 space-y-3">
              <div className="flex items-center gap-2">
                <Monitor className="h-4 w-4 text-muted-foreground" />
                <h3 className="text-sm font-medium">Current status</h3>
              </div>

              <dl className="grid gap-2 text-xs">
                <div className="flex justify-between gap-4">
                  <dt className="text-muted-foreground">Status</dt>
                  <dd className="font-medium text-right">{diagnosticsSummary.status}</dd>
                </div>
                <div className="flex justify-between gap-4">
                  <dt className="text-muted-foreground">Latest issue</dt>
                  <dd className="font-medium text-right">{diagnosticsSummary.issue}</dd>
                </div>
                <div className="flex justify-between gap-4">
                  <dt className="text-muted-foreground">Last checked</dt>
                  <dd className="font-medium text-right">{diagnosticsSummary.lastChecked}</dd>
                </div>
              </dl>

              <p className="text-xs text-muted-foreground">{getDiagnosticsGuidance()}</p>

              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => void handleTestHotkey()}
                  disabled={testingHotkey}
                >
                  {testingHotkey ? "Testing…" : "Test Hotkey"}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => void handleReportHotkeyIssue()}
                >
                  Report Issue
                </Button>
              </div>
            </div>

            <div className="space-y-3">
              <button
                onClick={handleCopySystemInfo}
                className="w-full rounded-lg border border-border/50 bg-card hover:bg-accent/50 transition-colors p-4 flex items-center justify-between group"
              >
                <div className="flex items-center gap-3">
                  <div className="p-2 rounded-lg bg-primary/10 group-hover:bg-primary/20 transition-colors">
                    <Copy className="h-4 w-4" />
                  </div>
                  <div className="text-left">
                    <p className="text-sm font-medium">Copy System Info</p>
                    <p className="text-xs text-muted-foreground">
                      Copy system diagnostics to clipboard
                    </p>
                  </div>
                </div>
                <ChevronDown className="h-4 w-4 text-muted-foreground -rotate-90" />
              </button>

              <button
                onClick={handleOpenLogs}
                className="w-full rounded-lg border border-border/50 bg-card hover:bg-accent/50 transition-colors p-4 flex items-center justify-between group"
              >
                <div className="flex items-center gap-3">
                  <div className="p-2 rounded-lg bg-primary/10 group-hover:bg-primary/20 transition-colors">
                    <FileText className="h-4 w-4" />
                  </div>
                  <div className="text-left">
                    <p className="text-sm font-medium">Open Logs Folder</p>
                    <p className="text-xs text-muted-foreground">
                      Open logs folder to attach to support messages
                    </p>
                  </div>
                </div>
                <ChevronDown className="h-4 w-4 text-muted-foreground -rotate-90" />
              </button>
            </div>
          </div>

          <div className="space-y-4">
            <h2 className="text-base font-semibold">Quick Fixes</h2>

            <div className="space-y-2">
              {quickFixes.map((fix) => {
                const Icon = fix.icon;
                const isOpen = openItems.includes(fix.id);

                return (
                  <Collapsible
                    key={fix.id}
                    open={isOpen}
                    onOpenChange={() => toggleItem(fix.id)}
                  >
                    <div className="rounded-lg border border-border/50 bg-card overflow-hidden">
                      <CollapsibleTrigger className="w-full px-4 py-3 flex items-center justify-between hover:bg-accent/50 transition-colors">
                        <div className="flex items-center gap-3">
                          <Icon className="h-4 w-4 text-muted-foreground" />
                          <span className="text-sm font-medium">{fix.title}</span>
                        </div>
                        <ChevronDown
                          className={`h-4 w-4 text-muted-foreground transition-transform ${isOpen ? "rotate-180" : ""}`}
                        />
                      </CollapsibleTrigger>

                      <CollapsibleContent>
                        <div className="px-4 pb-4 pt-2 space-y-3 border-t border-border/50">
                          <div className="space-y-2">
                            <div className="space-y-1">
                              <p className="text-xs font-medium text-muted-foreground">Issue</p>
                              <p className="text-sm">{fix.issue}</p>
                            </div>

                            <div className="space-y-1 mt-3">
                              <p className="text-xs font-medium text-muted-foreground">Solution</p>
                              <p className="text-sm">{fix.solution}</p>
                            </div>
                          </div>
                        </div>
                      </CollapsibleContent>
                    </div>
                  </Collapsible>
                );
              })}
            </div>
          </div>

          <div className="space-y-4">
            <h2 className="text-base font-semibold">Get Support</h2>

            <div className="space-y-3">
              <button
                onClick={handleXSupport}
                className="w-full rounded-lg border border-border/50 bg-card hover:bg-accent/50 transition-colors p-4 flex items-center justify-between group"
              >
                <div className="flex items-center gap-3">
                  <div className="p-2 rounded-lg bg-primary/10 group-hover:bg-primary/20 transition-colors">
                    <XIcon className="h-4 w-4" />
                  </div>
                  <div className="text-left">
                    <p className="text-sm font-medium">Follow us on X</p>
                    <p className="text-xs text-muted-foreground">
                      @voicetypr - Get updates and support
                    </p>
                  </div>
                </div>
                <ChevronDown className="h-4 w-4 text-muted-foreground -rotate-90" />
              </button>

              <button
                onClick={handleEmailSupport}
                className="w-full rounded-lg border border-border/50 bg-card hover:bg-accent/50 transition-colors p-4 flex items-center justify-between group"
              >
                <div className="flex items-center gap-3">
                  <div className="p-2 rounded-lg bg-primary/10 group-hover:bg-primary/20 transition-colors">
                    <Mail className="h-4 w-4" />
                  </div>
                  <div className="text-left">
                    <p className="text-sm font-medium">Email Support</p>
                    <p className="text-xs text-muted-foreground">
                      Send us an email with diagnostic info
                    </p>
                  </div>
                </div>
                <ChevronDown className="h-4 w-4 text-muted-foreground -rotate-90" />
              </button>
            </div>
          </div>

          <div className="pt-4">
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>VoiceTypr v{appVersion}</span>
              <span>{platformName}</span>
            </div>
          </div>
        </div>
      </ScrollArea>

      <Dialog open={showEmailModal} onOpenChange={setShowEmailModal}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Choose Email Client</DialogTitle>
            <DialogDescription>
              How would you like to send your support request?
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-3 mt-4">
            <Button
              onClick={handleOpenInGmail}
              className="w-full justify-start gap-3 h-auto py-4"
              variant="outline"
            >
              <Globe className="h-4 w-4" />
              <div className="text-left flex-1">
                <p className="font-medium">Open in Gmail</p>
                <p className="text-xs text-muted-foreground">
                  Use Gmail in your web browser
                </p>
              </div>
            </Button>
            <Button
              onClick={handleOpenInDefaultClient}
              className="w-full justify-start gap-3 h-auto py-4"
              variant="outline"
            >
              <Monitor className="h-4 w-4" />
              <div className="text-left flex-1">
                <p className="font-medium">Open in Default App</p>
                <p className="text-xs text-muted-foreground">
                  Use your system's default email client
                </p>
              </div>
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      <ReportBugDialog
        isOpen={showReportBugDialog}
        onClose={() => setShowReportBugDialog(false)}
        initialMessage={reportBugInitialMessage}
        diagnosticContext={reportBugDiagnosticContext}
      />
    </div>
  );
}
