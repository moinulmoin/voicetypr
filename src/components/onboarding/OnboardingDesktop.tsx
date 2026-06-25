import { HotkeyInput, type BareModifierSpec } from "@/components/HotkeyInput";
import { ModelCard } from "@/components/ModelCard";
import type { SavedConnection } from "@/components/RemoteServerCard";
import { AddServerModal } from "@/components/sections/AddServerModal";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { useSettings } from "@/contexts/SettingsContext";
import { useAccessibilityPermission } from "@/hooks/useAccessibilityPermission";
import { useMicrophonePermission } from "@/hooks/useMicrophonePermission";
import type { useModelManagement } from "@/hooks/useModelManagement";
import { useRecording } from "@/hooks/useRecording";
import { formatHotkey } from "@/lib/hotkey-utils";
import { isMacOS, isWindows } from "@/lib/platform";
import { getModelDisplayName } from "@/lib/model-display";
import { cn } from "@/lib/utils";
import { ValidationPresets } from "@/lib/keyboard-normalizer";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-shell";
import {
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  CircleAlert,
  CircleCheck,
  HardDrive,
  Info,
  Keyboard,
  Laptop,
  Mic,
  Network,
  Rocket,
  Server,
  ShieldCheck,
  Sparkles,
  Star,
  Wifi,
  WifiOff,
  Zap,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import type { ModifierKind, ModifierSide, ShortcutBinding, ShortcutSettings } from "@/types/shortcuts";
import { createLogger } from "@/lib/logger";

const log = createLogger("onboarding");

const UPGRADE_URL = "https://voicetypr.com/#pricing"; // [Upgrade to Pro] opens this externally
const CONTACT_WEBHOOK_URL =
  "https://discord.com/api/webhooks/1519563693501710376/kR6Cylv3kkZpLeMESRyn_t7lCWiGvichiCTn9LyYRUs2sXvZvYbVN5_5sXGm7A--2v_E"; // Discord webhook for onboarding contact; empty string = skip the POST

interface OnboardingDesktopProps {
  onCompletionStart?: () => void;
  onCompletionError?: () => void;
  onComplete: (target?: "license") => void;
  modelManagement: ReturnType<typeof useModelManagement>;
}

type Step =
  | "welcome"
  | "source"
  | "permissions"
  | "readiness"
  | "hotkey"
  | "first_transcription"
  | "success"
  | "upgrade";

type SourceType = "local" | "remote";
type PermissionStatus = "checking" | "granted" | "denied" | "error";

interface PermissionState {
  status: PermissionStatus;
  error?: string;
}

interface TranscriptionAddedPayload {
  text?: string;
  model?: string;
  timestamp?: string;
  status?: "completed" | "in_progress" | "failed";
}

interface DiscoveredRemoteServer {
  name: string;
  host: string;
  port: number;
  model: string;
  auth_required: boolean;
  machine_id: string;
}

const SOURCE_OPTIONS: Array<{
  id: SourceType;
  title: string;
  description: string;
  icon: typeof Laptop;
  bullets: string[];
}> = [
  {
    id: "local",
    title: "Use this device",
    description: "Download a local model and transcribe on this device.",
    icon: Laptop,
    bullets: ["Raw audio stays here", "Works offline after setup", "Best default for one device"],
  },
  {
    id: "remote",
    title: "Use another Voicetypr",
    description: "Connect to a stronger device or workstation running Voicetypr on your network.",
    icon: Network,
    bullets: ["Skip local model download", "Use a faster machine", "Great for weak laptops"],
  },
];

const isRemoteServerOnline = (server?: SavedConnection | null) =>
  server?.status === "Online";

const sourceLabel = (sourceType: SourceType, confirmed: boolean) =>
  confirmed ? (sourceType === "local" ? "This device" : "Remote Voicetypr") : "Choose source";
const getSampleSelectionKey = (
  sourceType: SourceType,
  selectedModelName: string | null,
  activeRemoteServer: SavedConnection | null,
) =>
  sourceType === "local"
    ? `local:${selectedModelName ?? ""}`
    : `remote:${activeRemoteServer?.id ?? ""}:${activeRemoteServer?.model ?? ""}`;

const FAILED_ONBOARDING_SAMPLE_PLACEHOLDER =
  "Transcription failed - re-transcribe after resolving the issue";

const ONBOARDING_HOTKEY_VALIDATION = ValidationPresets.custom({
  minKeys: 1,
  requireModifier: false,
  requireModifierForMultiKey: true,
});

const SAMPLE_SENTENCE = "The quick brown fox jumps over the lazy dog.";

const isSuccessfulOnboardingSampleEvent = (
  payload: TranscriptionAddedPayload,
  sourceType: SourceType,
  selectedModelName: string | null,
  activeRemoteServer: SavedConnection | null,
): boolean => {
  const text = payload.text?.trim();
  if (!text || text === FAILED_ONBOARDING_SAMPLE_PLACEHOLDER) {
    return false;
  }
  if (payload.status === "failed" || payload.status === "in_progress") {
    return false;
  }
  const eventModel = payload.model?.trim();
  if (!eventModel) {
    return false;
  }
  if (sourceType === "local") {
    return Boolean(selectedModelName && eventModel === selectedModelName);
  }
  return Boolean(
    activeRemoteServer && eventModel === activeRemoteServer.model,
  );
};

/** Format a bare modifier spec as a short human-readable label, e.g. "Right ⌥". */
function formatBareModifierLabel({ modifier, side }: BareModifierSpec): string {
  const sideStr = side === "right" ? "Right " : side === "left" ? "Left " : "";
  const macIcons: Record<string, string> = {
    alt: "⌥", meta: "⌘", control: "⌃", shift: "⇧",
  };
  const modStr = isMacOS
    ? (macIcons[modifier] ?? modifier)
    : modifier.charAt(0).toUpperCase() + modifier.slice(1);
  return `${sideStr}${modStr}`;
}

export const OnboardingDesktop = function OnboardingDesktop({
  onCompletionStart,
  onCompletionError,
  onComplete,
  modelManagement,
}: OnboardingDesktopProps) {
  const { settings, updateSettings } = useSettings();
  const recording = useRecording();
  const {
    hasPermission: hasMicPermission,
    checkPermission: checkMicPermission,
    requestPermission: requestMicPermission,
  } = useMicrophonePermission({ checkOnMount: false });
  const {
    hasPermission: hasAccessPermission,
    checkPermission: checkAccessPermission,
    requestPermission: requestAccessPermission,
  } = useAccessibilityPermission({ checkOnMount: false });

  const {
    models,
    modelOrder,
    downloadProgress,
    verifyingModels,
    downloadErrors = {},
    downloadModel,
    cancelDownload,
    deleteModel,
    isLoading,
  } = modelManagement;

  const [currentStep, setCurrentStep] = useState<Step>("welcome");
  // Anonymous error tracking is opt-out: checkbox defaults to checked on the success screen.
  const [telemetryOptIn, setTelemetryOptIn] = useState(true);
  const [contactName, setContactName] = useState("");
  const [contactEmail, setContactEmail] = useState("");
  const [sourceType, setSourceType] = useState<SourceType>("local");
  const [sourceConfirmed, setSourceConfirmed] = useState(false);
  const sourceConfirmedRef = useRef(false);
  const [hotkey, setHotkey] = useState(
    settings?.hotkey || "Alt+Space",
  );
  const [isEditingHotkey, setIsEditingHotkey] = useState(false);
  const [capturedBareModifier, setCapturedBareModifier] = useState<BareModifierSpec | null>(null);
  const [isRequestingPermission, setIsRequestingPermission] = useState<
    string | null
  >(null);
  const [checkingPermissions, setCheckingPermissions] = useState<Set<string>>(
    new Set(),
  );
  const [remoteServers, setRemoteServers] = useState<SavedConnection[]>([]);
  const [activeRemoteServerId, setActiveRemoteServerId] = useState<
    string | null
  >(null);
  const [isLoadingRemoteServers, setIsLoadingRemoteServers] = useState(false);
  const [showAddRemoteModal, setShowAddRemoteModal] = useState(false);
  const [discoveredRemoteServers, setDiscoveredRemoteServers] = useState<DiscoveredRemoteServer[]>([]);
  const [selectedDiscoveredServer, setSelectedDiscoveredServer] = useState<DiscoveredRemoteServer | null>(null);
  const [sampleTranscript, setSampleTranscript] = useState<{
    text: string;
    selectionKey: string;
  } | null>(null);
  const [sampleError, setSampleError] = useState<string | null>(null);
  const [isSavingCompletion, setIsSavingCompletion] = useState(false);
  const [holdToTalk, setHoldToTalk] = useState(false);

  useEffect(() => {
    sourceConfirmedRef.current = sourceConfirmed;
  }, [sourceConfirmed]);

  const permissions = {
    microphone: {
      status:
        hasMicPermission === null
          ? "checking"
          : hasMicPermission
            ? "granted"
            : "denied",
    } as PermissionState,
    accessibility: {
      status:
        hasAccessPermission === null
          ? "checking"
          : hasAccessPermission
            ? "granted"
            : "denied",
    } as PermissionState,
  };

  const steps = useMemo(
    () =>
      isMacOS
        ? [
            "welcome",
            "source",
            "permissions",
            "readiness",
            "hotkey",
            "first_transcription",
            "success",
            "upgrade",
          ] satisfies Step[]
        : [
            "welcome",
            "source",
            "readiness",
            "hotkey",
            "first_transcription",
            "success",
            "upgrade",
          ] satisfies Step[],
    [],
  );

  const currentIndex = steps.indexOf(currentStep);
  const selectedModelName = settings?.current_model || null;
  const selectedModel = selectedModelName ? models[selectedModelName] : null;
  const activeRemoteServer = useMemo(
    () => remoteServers.find((server) => server.id === activeRemoteServerId) ?? null,
    [activeRemoteServerId, remoteServers],
  );
  const isModelReady = useCallback(
    (name: string) => models[name]?.downloaded === true && !models[name]?.requires_setup,
    [models],
  );
  const handleDeleteModel = useCallback(
    async (modelName: string) => {
      const deleted = await deleteModel(modelName);
      if (deleted && settings?.current_model === modelName) {
        await updateSettings({ current_model: "", current_model_engine: "whisper" });
      }
    },
    [deleteModel, settings, updateSettings],
  );
  const localReady = Boolean(selectedModelName && isModelReady(selectedModelName));
  const hasDownloadedLocalModel = modelOrder.some((name) => isModelReady(name));
  const remoteReady = isRemoteServerOnline(activeRemoteServer);
  const sourceReady = sourceType === "local" ? localReady : remoteReady;
  const sampleErrorDescription =
    sourceType === "remote" && sampleError && !remoteReady
      ? "The selected remote Voicetypr is offline. Make sure sharing is enabled on that device, keep both devices on the same network, then go back and choose an online server."
      : sampleError;
  const sampleSelectionKey = getSampleSelectionKey(
    sourceType,
    selectedModelName,
    activeRemoteServer,
  );
  const previousSampleSelectionKey = useRef(sampleSelectionKey);
  const hasCurrentSampleTranscript =
    sampleTranscript?.selectionKey === sampleSelectionKey;
  const currentSampleTranscript = hasCurrentSampleTranscript
    ? sampleTranscript.text
    : null;

  const loadRemoteServers = useCallback(async () => {
    setIsLoadingRemoteServers(true);
    try {
      const [savedServers, activeServer, discoveredServers] = await Promise.all([
        invoke<SavedConnection[]>("list_remote_servers"),
        invoke<string | null>("get_active_remote_server"),
        invoke<DiscoveredRemoteServer[]>("discover_remote_servers", { timeoutMs: 1200 }).catch(
          (error) => {
            log.error("[OnboardingDesktop] Failed to discover remote servers:", error);
            return [] as DiscoveredRemoteServer[];
          },
        ),
      ]);
      const discoveredCandidates = discoveredServers.filter(
        (server) =>
          !savedServers.some((saved) => saved.host === server.host && saved.port === server.port),
      );

      const allServers = savedServers;

      setActiveRemoteServerId(activeServer);
      setRemoteServers(allServers);
      setDiscoveredRemoteServers(discoveredCandidates);

      const refreshedServers = await Promise.all(
        allServers.map(async (server) => {
          try {
            return await invoke<SavedConnection>("check_remote_server_status", {
              serverId: server.id,
            });
          } catch (error) {
            log.error(
              `[OnboardingDesktop] Failed to refresh remote server ${server.id}:`,
              error,
            );
            return server;
          }
        }),
      );

      setRemoteServers(refreshedServers);
      if (
        !sourceConfirmedRef.current &&
        activeServer &&
        refreshedServers.some((server) => server.id === activeServer)
      ) {
        setSourceType("remote");
      }
    } catch (error) {
      log.error("[OnboardingDesktop] Failed to load remote servers:", error);
    } finally {
      setIsLoadingRemoteServers(false);
    }
  }, []);

  useEffect(() => {
    if (currentStep !== "permissions") return;
    void checkPermissions();
  }, [currentStep]);

  useEffect(() => {
    if (currentStep !== "permissions") return;
    const handleFocus = () => {
      void checkPermissions();
    };

    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
  }, [currentStep]);

  useEffect(() => {
    if (currentStep !== "source" && currentStep !== "readiness") return;
    void loadRemoteServers();
  }, [currentStep, loadRemoteServers]);


  useEffect(() => {
    const setup = async () => {
      const unlisten = await listen<TranscriptionAddedPayload>(
        "transcription-added",
        (event) => {
          if (currentStep !== "first_transcription") {
            return;
          }
          const payload = event.payload ?? {};
          if (
            !isSuccessfulOnboardingSampleEvent(
              payload,
              sourceType,
              selectedModelName,
              activeRemoteServer,
            )
          ) {
            return;
          }
          const text = payload.text!.trim();
          setSampleTranscript({ text, selectionKey: sampleSelectionKey });
          setSampleError(null);
        },
      );
      return unlisten;
    };

    let cleanup: (() => void) | undefined;
    void setup().then((unlisten) => {
      cleanup = unlisten;
    });

    return () => cleanup?.();
  }, [
    activeRemoteServer,
    currentStep,
    sampleSelectionKey,
    selectedModelName,
    sourceType,
  ]);

  useEffect(() => {
    if (recording.state === "error") {
      setSampleError(recording.error || "Recording failed. Try again.");
    }
  }, [recording.error, recording.state]);

  useEffect(() => {
    if (previousSampleSelectionKey.current === sampleSelectionKey) {
      return;
    }

    previousSampleSelectionKey.current = sampleSelectionKey;
    setSampleTranscript(null);
    setSampleError(null);
  }, [sampleSelectionKey]);

  const checkPermissions = async () => {
    await Promise.all([checkMicPermission(), checkAccessPermission()]);
  };

  const confirmSource = (nextSourceType: SourceType) => {
    sourceConfirmedRef.current = true;
    setSourceType(nextSourceType);
    setSourceConfirmed(true);
  };

  const checkSinglePermission = async (type: "microphone" | "accessibility") => {
    setCheckingPermissions((prev) => new Set(prev).add(type));

    try {
      if (type === "microphone") {
        await checkMicPermission();
      } else {
        await checkAccessPermission();
      }
    } catch (error) {
      log.error(`Failed to check ${type} permission:`, error);
    } finally {
      setCheckingPermissions((prev) => {
        const next = new Set(prev);
        next.delete(type);
        return next;
      });
    }
  };

  const requestPermission = async (type: "microphone" | "accessibility") => {
    setIsRequestingPermission(type);
    try {
      if (type === "microphone") {
        const granted = await requestMicPermission();
        if (!granted) {
          await open(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
          );
        }
      } else {
        const granted = await requestAccessPermission();
        if (!granted) {
          await open(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
          );
        }
      }
    } catch (error) {
      log.error(`Failed to request ${type} permission:`, error);
    } finally {
      setIsRequestingPermission(null);
    }
  };

  const selectLocalModel = async (modelName: string) => {
    const info = models[modelName];
    await invoke("set_active_remote_server", { serverId: null });
    setActiveRemoteServerId(null);
    setSourceType("local");
    await updateSettings({
      current_model: modelName,
      current_model_engine: info?.engine ?? "whisper",
      speech_language: "en",
    });
  };

  const switchToLocalReadiness = () => {
    confirmSource("local");
  };

  const selectRemoteServer = async (serverId: string) => {
    const server = remoteServers.find((candidate) => candidate.id === serverId);
    if (!isRemoteServerOnline(server)) {
      toast.error("Remote Voicetypr is not online yet");
      return;
    }

    await invoke("set_active_remote_server", { serverId });
    setActiveRemoteServerId(serverId);
    confirmSource("remote");
    toast.success("Remote Voicetypr selected");
  };

  const handleRemoteServerAdded = (server: SavedConnection) => {
    setRemoteServers((prev) => {
      const existingIndex = prev.findIndex((candidate) => candidate.id === server.id);
      if (existingIndex >= 0) {
        const updated = [...prev];
        updated[existingIndex] = server;
        return updated;
      }
      return [...prev, server];
    });
    setDiscoveredRemoteServers((prev) =>
      prev.filter((candidate) => !(candidate.host === server.host && candidate.port === server.port)),
    );
    setActiveRemoteServerId(server.id);
    void invoke("set_active_remote_server", { serverId: server.id }).catch((error) => {
      log.error("[OnboardingDesktop] Failed to activate remote server:", error);
      toast.error("Remote Voicetypr was added, but could not be selected");
    });
    confirmSource("remote");
    void loadRemoteServers();
  };

  const handleAddDiscoveredRemoteServer = async (server: DiscoveredRemoteServer) => {
    if (server.auth_required) {
      setSelectedDiscoveredServer(server);
      setShowAddRemoteModal(true);
      return;
    }

    try {
      const added = await invoke<SavedConnection>("add_remote_server", {
        host: server.host,
        port: server.port,
        password: null,
        name: server.name,
      });
      handleRemoteServerAdded(added);
      toast.success(`${server.name} selected`);
    } catch (error) {
      log.error("[OnboardingDesktop] Failed to add discovered remote server:", error);
      toast.error(error instanceof Error ? error.message : "Failed to add remote Voicetypr");
    }
  };


  // Stable id for the onboarding-created HoldToRecord modifier_hold binding.
  // Using a fixed id means repeated saves replace the same entry instead of
  // accumulating stale bindings.
  const ONBOARDING_HOLD_ID = "onboarding-primary-hold";

  const saveHotkeySettings = async () => {
    if (capturedBareModifier) {
      // ── Bare modifier path ────────────────────────────────────────────
      // 1. Unregister (and clear) any existing primary global shortcut.
      //    set_global_shortcut("") is the backend's "clear primary" contract.
      await invoke("set_global_shortcut", { shortcut: "" });

      // 2. Upsert the native binding with the stable id, action determined by Hold to talk.
      //    holdToTalk ON  → modifier_hold / hold_to_record (PTT).
      //    holdToTalk OFF → double_tap   / toggle_recording (double-tap to toggle).
      const currentSettings = await invoke<ShortcutSettings>("get_shortcut_settings");
      const newBinding: ShortcutBinding = holdToTalk
        ? {
            id: ONBOARDING_HOLD_ID,
            action: "hold_to_record",
            shortcut: "",
            trigger: "hold",
            enabled: true,
            allow_risky_combo: false,
            trigger_kind: "modifier_hold",
            modifier: {
              modifier: capturedBareModifier.modifier as ModifierKind,
              side: capturedBareModifier.side as ModifierSide,
            },
            double_tap_ms: null,
          }
        : {
            id: ONBOARDING_HOLD_ID,
            action: "toggle_recording",
            shortcut: "",
            trigger: "pressed",
            enabled: true,
            allow_risky_combo: false,
            trigger_kind: "double_tap",
            modifier: {
              modifier: capturedBareModifier.modifier as ModifierKind,
              side: capturedBareModifier.side as ModifierSide,
            },
            double_tap_ms: null,
          };
      await invoke("update_shortcut_settings", {
        settings: {
          bindings: [
            ...(currentSettings?.bindings ?? []).filter((b) => b.id !== ONBOARDING_HOLD_ID),
            newBinding,
          ],
        },
      });

      // 3. Persist the remaining settings.
      await updateSettings({
        hotkey: "",
        recording_mode: holdToTalk ? "push_to_talk" : "toggle",
        current_model: selectedModelName || "",
        current_model_engine: selectedModel?.engine ?? "whisper",
        speech_language: "en",
        onboarding_completed: false,
      });
    } else {
      // ── Combo / safe single-key path ──────────────────────────────────
      // 1. Register the new primary shortcut (also saves hotkey to store).
      await invoke("set_global_shortcut", { shortcut: hotkey });

      // 2. Remove any onboarding-created HoldToRecord binding so there is
      //    never BOTH a primary global shortcut and a modifier_hold binding.
      const currentSettings = await invoke<ShortcutSettings>("get_shortcut_settings");
      if ((currentSettings?.bindings ?? []).some((b) => b.id === ONBOARDING_HOLD_ID)) {
        await invoke("update_shortcut_settings", {
          settings: {
            bindings: (currentSettings?.bindings ?? []).filter((b) => b.id !== ONBOARDING_HOLD_ID),
          },
        });
      }

      // 3. Persist the remaining settings.
      await updateSettings({
        hotkey,
        recording_mode: holdToTalk ? "push_to_talk" : "toggle",
        current_model: selectedModelName || "",
        current_model_engine: selectedModel?.engine ?? "whisper",
        speech_language: "en",
        onboarding_completed: false,
      });
    }
  };

  const handleGpuToggle = async (checked: boolean) => {
    await updateSettings({ transcription_acceleration: checked ? 'auto' : 'cpu' });
  };

  const completeOnboarding = async (target?: "license") => {
    setIsSavingCompletion(true);
    onCompletionStart?.();
    try {
      await updateSettings({ onboarding_completed: true });
      // Persist the diagnostics choice from the success step (checked by default; opt-out).
      try {
        await invoke("set_telemetry_consent", { enabled: telemetryOptIn });
      } catch (telemetryError) {
        log.error("Failed to persist telemetry consent:", telemetryError);
      }
      onComplete(target);
    } catch (error) {
      onCompletionError?.();
      log.error("Failed to complete onboarding:", error);
      toast.error("Failed to finish onboarding. Please try again.");
    } finally {
      setIsSavingCompletion(false);
    }
  };

  // Success screen "Continue": optionally POST contact details (silent-fail), then advance to upgrade.
  const submitContactAndContinue = async () => {
    const name = contactName.trim();
    const email = contactEmail.trim();
    if (CONTACT_WEBHOOK_URL && (name || email)) {
      try {
        await fetch(CONTACT_WEBHOOK_URL, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            username: "Onboarding · New Users",
            content: `**New user onboarded**\nName: ${name || "—"}\nEmail: ${email || "—"}`,
          }),
        });
      } catch (contactError) {
        // Silent-fail: contact capture must never block onboarding.
        log.error("Failed to submit onboarding contact:", contactError);
      }
    }
    setCurrentStep("upgrade");
  };

  const handleNext = async () => {
    try {
      if (currentStep === "welcome") {
        setCurrentStep("source");
        return;
      }

      if (currentStep === "source") {
        setCurrentStep(isMacOS ? "permissions" : "readiness");
        return;
      }

      if (currentStep === "permissions") {
        setCurrentStep("readiness");
        return;
      }

      if (currentStep === "readiness") {
        if (sourceType === "local") {
          await invoke("set_active_remote_server", { serverId: null });
          setActiveRemoteServerId(null);
        }
        setCurrentStep("hotkey");
        return;
      }

      if (currentStep === "hotkey") {
        await saveHotkeySettings();
        setCurrentStep("first_transcription");
        return;
      }

      if (currentStep === "first_transcription" && hasCurrentSampleTranscript) {
        setCurrentStep("success");
      }
    } catch (error) {
      log.error("Failed to advance onboarding:", error);
      toast.error(error instanceof Error ? error.message : "Failed to continue onboarding");
    }
  };

  const handleBack = () => {
    const previousIndex = currentIndex - 1;
    if (previousIndex >= 0) {
      setCurrentStep(steps[previousIndex]);
    }
  };

  const startSampleRecording = async () => {
    setSampleError(null);
    setSampleTranscript(null);
    await recording.startRecording();
  };

  const stopSampleRecording = async () => {
    setSampleError(null);
    await recording.stopRecording();
  };

  const canProceed = () => {
    switch (currentStep) {
      case "source":
        return sourceConfirmed;
      case "permissions":
        if (!isMacOS) return true;
        return (
          permissions.microphone.status === "granted" &&
          permissions.accessibility.status === "granted"
        );
      case "readiness":
        return sourceReady;
      case "hotkey":
        return !isEditingHotkey;
      case "first_transcription":
        return hasCurrentSampleTranscript;
      default:
        return true;
    }
  };

  return (
    <div className="min-h-screen overflow-hidden bg-[radial-gradient(110%_80%_at_50%_-10%,var(--sage-bg),transparent_55%),linear-gradient(180deg,var(--background),var(--background))] text-foreground">
      {currentStep !== "success" && currentStep !== "upgrade" && (
        <div className="mx-auto flex w-full max-w-5xl items-center justify-between gap-4 px-8 py-6">
          <div>
            <p className="text-sm font-semibold tracking-tight">Voicetypr Setup</p>
            <p className="text-xs text-muted-foreground">{sourceLabel(sourceType, sourceConfirmed)}</p>
          </div>
          <StepDots currentIndex={currentIndex} total={steps.length} />
        </div>
      )}

      <main className="mx-auto flex min-h-[calc(100vh-76px)] w-full max-w-5xl items-center justify-center px-8 pb-10">
        {currentStep === "welcome" && (
          <section className="grid w-full gap-8 lg:grid-cols-[1.1fr_0.9fr] lg:items-center">
            <div className="flex flex-col gap-6">
              <div className="flex flex-col gap-4">
                <h1 className="max-w-3xl text-5xl font-semibold tracking-[-0.045em] text-balance sm:text-6xl">
                  Welcome to Voicetypr
                </h1>
                <p className="max-w-2xl text-base leading-7 text-muted-foreground">
                  Choose where transcription runs, set your hotkey, then try one real voice typing test before setup finishes.
                </p>
                <p className="text-sm text-muted-foreground">
                  By continuing, you agree to our Terms and Privacy Policy.
                </p>
              </div>
              <div className="flex flex-wrap gap-3">
                <Button size="lg" onClick={handleNext}>
                  Start setup
                  <ChevronRight />
                </Button>
              </div>
            </div>

            <Card className="rounded-2xl border border-border bg-card shadow-sm">
              <CardHeader>
                <CardTitle className="flex items-center gap-2.5 text-lg">
                  <span className="flex size-8 items-center justify-center rounded-lg bg-sage-bg text-sage">
                    <Sparkles className="size-4" />
                  </span>
                  Setup completes when this works
                </CardTitle>
                <CardDescription>
                  No fake green check. Voicetypr is ready only after the first recording succeeds.
                </CardDescription>
              </CardHeader>
              <CardContent className="flex flex-col gap-2.5">
                {[
                  ["1", "Pick local or remote transcription"],
                  ["2", "Prepare the selected source"],
                  ["3", "Record one sample and see the transcript"],
                ].map(([number, text]) => (
                  <div key={number} className="flex items-center gap-3 rounded-xl border border-border/60 bg-muted/40 px-3 py-2.5">
                    <span className="flex size-7 items-center justify-center rounded-full bg-sage-bg text-sm font-semibold text-sage">
                      {number}
                    </span>
                    <span className="text-sm text-muted-foreground">{text}</span>
                  </div>
                ))}
              </CardContent>
            </Card>
          </section>
        )}

        {currentStep === "source" && (
          <OnboardingPanel
            title="Where should transcription run?"
            description="Choose local transcription on this device or remote transcription from another Voicetypr device."
            footer={
              <StepFooter
                onBack={handleBack}
                onNext={handleNext}
                nextDisabled={!canProceed()}
                nextLabel="Continue"
              />
            }
          >
            <div role="radiogroup" aria-label="Transcription source" className="grid gap-4 md:grid-cols-2">
              {SOURCE_OPTIONS.map((option) => {
                const Icon = option.icon;
                const selected = sourceConfirmed && sourceType === option.id;
                return (
                  <Card
                    key={option.id}
                    role="radio"
                    aria-checked={selected}
                    tabIndex={0}
                    className={cn(
                      "cursor-pointer rounded-2xl border border-border bg-card shadow-sm transition-colors hover:border-sage/40 hover:bg-muted/30",
                      selected && "border-sage/50 bg-sage-bg/40 ring-1 ring-sage/30 hover:bg-sage-bg/40",
                    )}
                    onClick={() => {
                      confirmSource(option.id);
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        confirmSource(option.id);
                      }
                    }}
                  >
                    <CardHeader>
                      <CardAction>
                        {selected ? (
                          <CircleCheck className="size-5 text-sage" />
                        ) : null}
                      </CardAction>
                      <div
                        className={cn(
                          "mb-2 flex size-11 items-center justify-center rounded-xl bg-sage-bg text-sage",
                        )}
                      >
                        <Icon className="size-5" />
                      </div>
                      <CardTitle className="text-xl">{option.title}</CardTitle>
                      <CardDescription>{option.description}</CardDescription>
                    </CardHeader>
                    <CardContent>
                      <div className="flex flex-col gap-2">
                        {option.bullets.map((bullet) => (
                          <div key={bullet} className="flex items-center gap-2 text-sm text-muted-foreground">
                            <CheckCircle2 className="size-4 text-sage" />
                            {bullet}
                          </div>
                        ))}
                      </div>
                    </CardContent>
                  </Card>
                );
              })}
            </div>
          </OnboardingPanel>
        )}

        {currentStep === "permissions" && (
          <OnboardingPanel
            title="Grant the permissions Voicetypr actually needs"
            description="Microphone starts recording. Accessibility lets the global hotkey work while you are in other apps."
            footer={
              <StepFooter
                onBack={handleBack}
                onNext={handleNext}
                nextDisabled={!canProceed()}
                nextLabel="Continue"
              />
            }
          >
            <div className="grid gap-4 md:grid-cols-2">
              {[
                {
                  type: "microphone" as const,
                  icon: Mic,
                  title: "Microphone",
                  desc: "Record your voice for the first transcription.",
                  ...permissions.microphone,
                },
                {
                  type: "accessibility" as const,
                  icon: Keyboard,
                  title: "Accessibility",
                  desc: "Use the recording hotkey system-wide.",
                  ...permissions.accessibility,
                },
              ].map((perm) => (
                <Card key={perm.type} className="rounded-2xl border border-border bg-card shadow-sm">
                  <CardHeader>
                    <div className="flex items-start justify-between gap-4">
                      <div className="flex gap-3">
                        <div
                          className={cn(
                            "flex size-10 items-center justify-center rounded-xl bg-sage-bg text-sage",
                            perm.status === "error" && "bg-destructive/10 text-destructive",
                          )}
                        >
                          <perm.icon className="size-5" />
                        </div>
                        <div>
                          <CardTitle>{perm.title}</CardTitle>
                          <CardDescription>{perm.desc}</CardDescription>
                        </div>
                      </div>
                      {perm.status === "granted" ? (
                        <Badge variant="secondary" className="gap-1 bg-sage-bg text-sage">
                          <CircleCheck className="size-3" />
                          Granted
                        </Badge>
                      ) : null}
                    </div>
                  </CardHeader>
                  <CardFooter className="justify-between gap-3">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => void checkSinglePermission(perm.type)}
                      disabled={checkingPermissions.has(perm.type)}
                    >
                      {checkingPermissions.has(perm.type) ? <Spinner /> : null}
                      Recheck
                    </Button>
                    <Button
                      size="sm"
                      onClick={() => void requestPermission(perm.type)}
                      disabled={isRequestingPermission === perm.type || perm.status === "granted"}
                    >
                      {isRequestingPermission === perm.type ? <Spinner /> : null}
                      Grant access
                    </Button>
                  </CardFooter>
                </Card>
              ))}
            </div>
          </OnboardingPanel>
        )}

        {currentStep === "readiness" && (
          <OnboardingPanel
            title={sourceType === "local" ? "Prepare this device" : "Connect a remote Voicetypr"}
            description={
              sourceType === "local"
                ? "Pick a downloaded local model, or download one now. The button unlocks as soon as the model is usable."
                : "Choose an online Voicetypr server. Password-protected servers must pass the connection test before onboarding continues."
            }
            footer={
              <StepFooter
                onBack={handleBack}
                onNext={handleNext}
                nextDisabled={!canProceed()}
                nextLabel="Continue"
              />
            }
          >
            {sourceType === "local" ? (
              <div className="flex flex-col gap-4">
                <ModelLegend />
                <Card className="rounded-2xl border border-border bg-card py-0 shadow-sm">
                  <ScrollArea className="h-[320px]">
                    <div className="flex flex-col gap-3 p-4">
                      {modelOrder.map((name: string) => {
                        const model = models[name];
                        if (!model) return null;
                        const progressValue = downloadProgress[name];
                        return (
                          <ModelCard
                            key={name}
                            name={name}
                            model={model}
                            downloadProgress={progressValue}
                            isVerifying={verifyingModels.has(name)}
                            downloadError={downloadErrors[name]}
                            isSelected={settings?.current_model === name}
                            onDownload={downloadModel}
                            onSelect={(modelName) => void selectLocalModel(modelName)}
                            onCancelDownload={cancelDownload}
                            onDelete={handleDeleteModel}
                            showSelectButton={isModelReady(name)}
                          />
                        );
                      })}
                      {isLoading && modelOrder.length === 0 ? (
                        <LoadingState label="Loading local models" />
                      ) : null}
                      {!isLoading && modelOrder.length === 0 ? (
                        <EmptyState title="No local models available" description="Remote Voicetypr is still available if you have another machine ready." />
                      ) : null}
                      {hasDownloadedLocalModel && !localReady ? (
                        <Alert>
                          <Info className="size-4" />
                          <AlertTitle>Select a downloaded model</AlertTitle>
                          <AlertDescription>
                            Downloaded models are ready to use, but onboarding needs one selected before continuing.
                          </AlertDescription>
                        </Alert>
                      ) : null}
                    </div>
                  </ScrollArea>
                </Card>
                {isWindows && (
                  <div className="flex items-center justify-between gap-3 rounded-2xl border border-border bg-card px-4 py-3 shadow-sm">
                    <div>
                      <p className="text-sm font-medium">Use GPU acceleration</p>
                      <p className="text-xs text-muted-foreground">Recommended — uses your graphics card for faster transcription.</p>
                    </div>
                    <Switch
                      checked={(settings?.transcription_acceleration ?? 'auto') !== 'cpu'}
                      onCheckedChange={(checked) => void handleGpuToggle(checked)}
                      aria-label="Use GPU acceleration"
                    />
                  </div>
                )}
              </div>
            ) : (
              <div className="flex flex-col gap-4">
                <div className="flex items-center justify-between gap-4">
                  <div>
                    <p className="text-sm font-medium">Saved remote servers</p>
                    <p className="text-sm text-muted-foreground">
                      Online servers can be selected for the first transcription.
                    </p>
                  </div>
                  <div className="flex gap-2">
                    <Button variant="outline" onClick={() => void loadRemoteServers()} disabled={isLoadingRemoteServers}>
                      {isLoadingRemoteServers ? <Spinner /> : null}
                      Refresh
                    </Button>
                    <Button onClick={() => {
                      setSelectedDiscoveredServer(null);
                      setShowAddRemoteModal(true);
                    }}>
                      Add server
                    </Button>
                  </div>
                </div>

                <Card className="rounded-2xl border border-border bg-card py-0 shadow-sm">
                  <ScrollArea className="h-[320px]">
                    <div className="flex flex-col gap-3 p-4">
                      {isLoadingRemoteServers && remoteServers.length === 0 ? (
                        <LoadingState label="Checking remote servers" />
                      ) : null}
                      {!isLoadingRemoteServers && remoteServers.length === 0 ? (
                        <div className="flex flex-col items-center gap-3">
                          <EmptyState
                            title="No remote servers saved"
                            description="Add a Voicetypr server, or set up this device with a local model instead."
                          />
                          <Button variant="outline" onClick={switchToLocalReadiness}>
                            Set up this device instead
                          </Button>
                        </div>
                      ) : null}
                      {discoveredRemoteServers.map((server) => (
                        <Card
                          key={`${server.machine_id}:${server.host}:${server.port}`}
                          size="sm"
                          className="rounded-xl border border-border bg-muted/30"
                        >
                          <CardHeader>
                            <CardAction>
                              <Badge variant={server.auth_required ? "outline" : "secondary"}>
                                {server.auth_required ? "Password required" : "Found on LAN"}
                              </Badge>
                            </CardAction>
                            <div className="flex items-start gap-3">
                              <div className="flex size-10 items-center justify-center rounded-xl bg-sage-bg text-sage">
                                <Wifi className="size-5" />
                              </div>
                              <div>
                                <CardTitle>{server.name || `${server.host}:${server.port}`}</CardTitle>
                                <CardDescription>
                                  {server.host}:{server.port} · {getModelDisplayName(server.model)}
                                </CardDescription>
                              </div>
                            </div>
                          </CardHeader>
                          <CardFooter className="justify-end">
                            <Button size="sm" onClick={() => void handleAddDiscoveredRemoteServer(server)}>
                              {server.auth_required ? "Add with password" : "Use this server"}
                            </Button>
                          </CardFooter>
                        </Card>
                      ))}
                      {remoteServers.map((server) => {
                        const selected = server.id === activeRemoteServerId;
                        const online = isRemoteServerOnline(server);
                        return (
                          <Card
                            key={server.id}
                            size="sm"
                            className={cn(
                              "rounded-xl border border-border bg-muted/30",
                              selected && "border-sage/50 bg-sage-bg/40 ring-1 ring-sage/30",
                            )}
                          >
                            <CardHeader>
                              <CardAction>
                                <Badge variant={online ? "secondary" : "outline"} className={cn(online && "bg-sage-bg text-sage")}>
                                  {online ? "Online" : server.status || "Unknown"}
                                </Badge>
                              </CardAction>
                              <div className="flex items-start gap-3">
                                <div className="flex size-10 items-center justify-center rounded-xl bg-sage-bg text-sage">
                                  {online ? <Wifi className="size-5" /> : <WifiOff className="size-5" />}
                                </div>
                                <div>
                                  <CardTitle>{server.name || `${server.host}:${server.port}`}</CardTitle>
                                  <CardDescription>
                                    {server.host}:{server.port}{server.model ? ` · ${getModelDisplayName(server.model)}` : ""}
                                  </CardDescription>
                                </div>
                              </div>
                            </CardHeader>
                            <CardFooter className="justify-end">
                              <Button
                                size="sm"
                                variant={selected ? "default" : "outline"}
                                disabled={!online}
                                onClick={() => void selectRemoteServer(server.id)}
                              >
                                {selected ? "Selected" : "Use this server"}
                              </Button>
                            </CardFooter>
                          </Card>
                        );
                      })}
                    </div>
                  </ScrollArea>
                </Card>

                <AddServerModal
                  open={showAddRemoteModal}
                  onOpenChange={(open) => {
                    setShowAddRemoteModal(open);
                    if (!open) {
                      setSelectedDiscoveredServer(null);
                    }
                  }}
                  onServerAdded={handleRemoteServerAdded}
                  initialServer={
                    selectedDiscoveredServer
                      ? {
                          host: selectedDiscoveredServer.host,
                          port: selectedDiscoveredServer.port,
                          name: selectedDiscoveredServer.name,
                          authRequired: selectedDiscoveredServer.auth_required,
                        }
                      : null
                  }
                />
              </div>
            )}
          </OnboardingPanel>
        )}

        {currentStep === "hotkey" && (
          <OnboardingPanel
            title="Pick your hotkey and recording mode"
            description="This is the system-wide shortcut for triggering Voicetypr. You can change both later in Settings."
            footer={
              <StepFooter
                onBack={handleBack}
                onNext={handleNext}
                nextDisabled={!canProceed()}
                nextLabel="Save hotkey"
              />
            }
          >
            <Card className="mx-auto w-full max-w-xl rounded-2xl border border-border bg-card shadow-sm">
              <CardHeader>
                <CardTitle className="flex items-center gap-2.5">
                  <span className="flex size-8 items-center justify-center rounded-lg bg-sage-bg text-sage">
                    <Keyboard className="size-4" />
                  </span>
                  Recording hotkey
                </CardTitle>
                <CardDescription>
                  Double tap Esc cancels an active recording.
                </CardDescription>
              </CardHeader>
              <CardContent className="flex flex-col gap-4">

                <HotkeyInput
                  value={hotkey}
                  onChange={(v) => { setHotkey(v); setCapturedBareModifier(null); }}
                  onEditingChange={setIsEditingHotkey}
                  onBareModifier={(spec) => { setCapturedBareModifier(spec); setHotkey(""); }}
                  allowBareModifier
                  validationRules={ONBOARDING_HOTKEY_VALIDATION}
                  placeholder={capturedBareModifier
                    ? holdToTalk
                      ? `Hold ${formatBareModifierLabel(capturedBareModifier)} · push-to-talk`
                      : `Double-tap ${formatBareModifierLabel(capturedBareModifier)} · tap to toggle`
                    : undefined}
                />
                {capturedBareModifier ? (
                  holdToTalk ? (
                    <Alert>
                      <Info className="size-4" />
                      <AlertTitle>Hold to talk</AlertTitle>
                      <AlertDescription>
                        Hold {formatBareModifierLabel(capturedBareModifier)} anywhere to start recording — release to stop.
                      </AlertDescription>
                    </Alert>
                  ) : (
                    <Alert>
                      <Info className="size-4" />
                      <AlertTitle>Double-tap to toggle on/off</AlertTitle>
                      <AlertDescription>
                        Double-tap {formatBareModifierLabel(capturedBareModifier)} to start recording, double-tap again to stop.
                      </AlertDescription>
                    </Alert>
                  )
                ) : (
                  <Alert>
                    <Info className="size-4" />
                    <AlertTitle>Tip</AlertTitle>
                    <AlertDescription>
                      Use a shortcut you can hit without looking down. Voicetypr works best when recording feels invisible.
                    </AlertDescription>
                  </Alert>
                )}
                <div className="flex items-center gap-3">
                  <Switch
                    id="hold-to-talk"
                    checked={holdToTalk}
                    onCheckedChange={setHoldToTalk}
                  />
                  <label htmlFor="hold-to-talk" className="text-sm cursor-pointer select-none">
                    Hold to talk (push-to-talk)
                  </label>
                </div>
              </CardContent>
            </Card>
          </OnboardingPanel>
        )}

        {currentStep === "first_transcription" && (
          <OnboardingPanel
            title="Do your first transcription"
            description="Say one short sentence. Onboarding only finishes after Voicetypr returns real text."
            footer={
              <StepFooter
                onBack={handleBack}
                onNext={handleNext}
                nextDisabled={!canProceed()}
                nextLabel="Review result"
                onSkip={() => setCurrentStep("success")}
                skipLabel="Skip for now"
              />
            }
          >
            <Card className="mx-auto w-full max-w-2xl rounded-2xl border border-border bg-card shadow-sm">
              <CardHeader>
                <CardAction>
                  <Badge variant="outline">{sourceLabel(sourceType, true)}</Badge>
                </CardAction>
                <CardTitle className="flex items-center gap-2.5 text-xl">
                  <span className="flex size-8 items-center justify-center rounded-lg bg-sage-bg text-sage">
                    <Mic className="size-4" />
                  </span>
                  Sample recording
                </CardTitle>
                <CardDescription>
                  Start a short sample, then stop to transcribe it.
                </CardDescription>
              </CardHeader>
              <CardContent className="flex flex-col gap-4">
                <div className="flex flex-wrap gap-3">
                  <Button
                    onClick={() => void startSampleRecording()}
                    disabled={recording.isActive}
                  >
                    {recording.state === "starting" ? <Spinner /> : null}
                    Start sample
                  </Button>
                  <Button
                    variant="outline"
                    onClick={() => void stopSampleRecording()}
                    disabled={!recording.isActive || recording.state === "transcribing"}
                  >
                    {recording.state === "stopping" || recording.state === "transcribing" ? <Spinner /> : null}
                    Stop and transcribe
                  </Button>
                </div>

                <div className="rounded-xl border border-border bg-muted/40 p-4 text-sm">
                  <p className="mb-1 font-medium text-muted-foreground">Read this aloud:</p>
                  <p className="text-foreground">{SAMPLE_SENTENCE}</p>
                </div>

                {recording.state === "transcribing" ? (
                  <Alert>
                    <Spinner />
                    <AlertTitle>Transcribing</AlertTitle>
                    <AlertDescription>
                      Waiting for {sourceLabel(sourceType, true).toLowerCase()} to return your text.
                    </AlertDescription>
                  </Alert>
                ) : null}

                {sampleError ? (
                  <Alert variant="destructive">
                    <CircleAlert className="size-4" />
                    <AlertTitle>Sample failed</AlertTitle>
                    <AlertDescription>{sampleErrorDescription}</AlertDescription>
                  </Alert>
                ) : null}

                <Textarea
                  key={currentSampleTranscript ?? "empty"}
                  defaultValue={currentSampleTranscript ?? ""}
                  placeholder="Your transcription will appear here as you speak."
                  className="min-h-[96px] resize-y text-base leading-7"
                />
              </CardContent>
            </Card>
          </OnboardingPanel>
        )}

        {currentStep === "success" && (
          <section className="mx-auto flex w-full max-w-xl flex-col items-center gap-6 text-center">
            <div className="flex size-16 items-center justify-center rounded-3xl bg-sage text-sage-foreground shadow-sm">
              <ShieldCheck className="size-8" />
            </div>
            <div className="flex flex-col gap-3">
              <h1 className="text-4xl font-semibold tracking-[-0.04em]">
                That&rsquo;s your first transcription 🎉
              </h1>
              <p className="text-muted-foreground">
                You&rsquo;re all set — Voicetypr is ready to use.{" "}
                {capturedBareModifier
                  ? holdToTalk
                    ? <>Hold {formatBareModifierLabel(capturedBareModifier)} anywhere to start recording — release to stop.</>
                    : <>Double-tap {formatBareModifierLabel(capturedBareModifier)} anywhere to start or stop recording.</>
                  : holdToTalk
                    ? <>Hold {formatHotkey(hotkey)} anywhere to start recording — release to stop.</>
                    : <>Press {formatHotkey(hotkey)} anywhere to start recording.</>}
              </p>
            </div>

            <div className="flex w-full flex-col gap-3 rounded-2xl border border-border bg-card p-4 text-left shadow-sm">
              <div className="grid gap-3 sm:grid-cols-2">
                <div className="flex flex-col gap-1.5">
                  <label htmlFor="onboarding-contact-name" className="text-sm font-medium">
                    Name
                  </label>
                  <Input
                    id="onboarding-contact-name"
                    value={contactName}
                    onChange={(event) => setContactName(event.target.value)}
                    placeholder="Your name"
                    autoComplete="name"
                  />
                </div>
                <div className="flex flex-col gap-1.5">
                  <label htmlFor="onboarding-contact-email" className="text-sm font-medium">
                    Email
                  </label>
                  <Input
                    id="onboarding-contact-email"
                    type="email"
                    value={contactEmail}
                    onChange={(event) => setContactEmail(event.target.value)}
                    placeholder="you@example.com"
                    autoComplete="email"
                  />
                </div>
              </div>
              <p className="text-xs text-muted-foreground">
                Optional — so we can reach you if something breaks. We&rsquo;ll never share it.
              </p>
            </div>

            <label className="flex w-full items-start gap-3 rounded-2xl border border-border bg-card p-4 text-left text-sm shadow-sm">
              <input
                type="checkbox"
                checked={telemetryOptIn}
                onChange={(event) => setTelemetryOptIn(event.target.checked)}
                className="mt-0.5 size-4 shrink-0 accent-[var(--sage)]"
              />
              <span className="text-muted-foreground">
                Send anonymous error reports to help make Voicetypr better. Never
                your audio, transcripts, or personal data — just crash details.
                Change this anytime in Settings → Advanced.
              </span>
            </label>

            <Button size="lg" onClick={() => void submitContactAndContinue()}>
              Continue
              <ChevronRight />
            </Button>
          </section>
        )}

        {currentStep === "upgrade" && (
          <section className="mx-auto flex w-full max-w-xl flex-col items-center gap-6 text-center">
            <div className="flex size-16 items-center justify-center rounded-3xl bg-sage text-sage-foreground shadow-sm">
              <Rocket className="size-8" />
            </div>
            <div className="flex flex-col gap-3">
              <h1 className="text-4xl font-semibold tracking-[-0.04em]">
                Make Voicetypr yours
              </h1>
              <p className="text-muted-foreground">
                Unlock Pro to keep it forever.
              </p>
            </div>

            <div className="flex w-full flex-col gap-3">
              <Button
                size="lg"
                className="w-full"
                onClick={() => void open(UPGRADE_URL)}
              >
                <Sparkles className="size-4" />
                Upgrade to Pro
              </Button>
              <Button
                variant="outline"
                size="lg"
                className="w-full"
                onClick={() => void completeOnboarding("license")}
                disabled={isSavingCompletion}
              >
                {isSavingCompletion ? <Spinner /> : null}
                I already have a license
              </Button>
              <Button
                variant="ghost"
                className="w-full text-muted-foreground"
                onClick={() => void completeOnboarding()}
                disabled={isSavingCompletion}
              >
                Maybe later
              </Button>
            </div>
          </section>
        )}
      </main>
    </div>
  );
};

function StepDots({ currentIndex, total }: { currentIndex: number; total: number }) {
  return (
    <div className="flex items-center gap-3">
      <div className="flex items-center gap-1.5">
        {Array.from({ length: total }).map((_, index) => (
          <span
            key={index}
            aria-hidden
            className={cn(
              "h-1.5 rounded-full transition-all",
              index < currentIndex
                ? "w-1.5 bg-sage/60"
                : index === currentIndex
                  ? "w-5 bg-sage"
                  : "w-1.5 bg-border",
            )}
          />
        ))}
      </div>
      <p className="text-xs tabular-nums text-muted-foreground">
        Step {currentIndex + 1} of {total}
      </p>
    </div>
  );
}

function OnboardingPanel({
  title,
  description,
  children,
  footer,
}: {
  title: string;
  description: string;
  children: React.ReactNode;
  footer: React.ReactNode;
}) {
  return (
    <section className="flex w-full flex-col gap-7 animate-fade-in">
      <div className="mx-auto flex max-w-3xl flex-col items-center gap-2.5 text-center">
        <h2 className="text-3xl font-semibold tracking-tight text-balance sm:text-4xl">
          {title}
        </h2>
        <p className="max-w-2xl text-sm leading-6 text-muted-foreground">
          {description}
        </p>
      </div>
      <div>{children}</div>
      <Separator />
      {footer}
    </section>
  );
}

function StepFooter({
  onBack,
  onNext,
  nextDisabled,
  nextLabel,
  onSkip,
  skipLabel,
}: {
  onBack: () => void;
  onNext: () => void | Promise<void>;
  nextDisabled?: boolean;
  nextLabel: string;
  onSkip?: () => void;
  skipLabel?: string;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <Button variant="outline" onClick={onBack}>
        <ChevronLeft />
        Back
      </Button>
      <div className="flex items-center gap-2">
        {onSkip ? (
          <Button variant="ghost" onClick={onSkip}>
            {skipLabel ?? "Skip"}
          </Button>
        ) : null}
        <Button onClick={() => void onNext()} disabled={nextDisabled}>
          {nextLabel}
          <ChevronRight />
        </Button>
      </div>
    </div>
  );
}

function ModelLegend() {
  return (
    <div className="flex flex-wrap items-center justify-center gap-4 text-xs text-muted-foreground">
      <span className="flex items-center gap-1.5">
        <Zap className="size-3.5 text-sage" />
        Speed
      </span>
      <span className="flex items-center gap-1.5">
        <CheckCircle2 className="size-3.5 text-sage" />
        Accuracy
      </span>
      <span className="flex items-center gap-1.5">
        <HardDrive className="size-3.5 text-sage" />
        Size
      </span>
      <span className="flex items-center gap-1.5">
        <Star className="size-3.5 fill-sage text-sage" />
        Recommended
      </span>
    </div>
  );
}

function LoadingState({ label }: { label: string }) {
  return (
    <div className="flex items-center justify-center gap-2 py-10 text-sm text-muted-foreground">
      <Spinner />
      {label}
    </div>
  );
}

function EmptyState({ title, description }: { title: string; description: string }) {
  return (
    <div className="flex flex-col items-center gap-2 py-10 text-center">
      <div className="flex size-10 items-center justify-center rounded-xl bg-muted text-muted-foreground">
        <Server className="size-5" />
      </div>
      <p className="font-medium">{title}</p>
      <p className="max-w-sm text-sm text-muted-foreground">{description}</p>
    </div>
  );
}
