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
import { Progress } from "@/components/ui/progress";
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

interface OnboardingDesktopProps {
  onCompletionStart?: () => void;
  onCompletionError?: () => void;
  onComplete: () => void;
  modelManagement: ReturnType<typeof useModelManagement>;
}

type Step =
  | "welcome"
  | "source"
  | "permissions"
  | "readiness"
  | "hotkey"
  | "first_transcription"
  | "success";

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
  eyebrow: string;
  description: string;
  icon: typeof Laptop;
  bullets: string[];
}> = [
  {
    id: "local",
    title: "Use this device",
    eyebrow: "Local source",
    description: "Download a local model and transcribe on this device.",
    icon: Laptop,
    bullets: ["Raw audio stays here", "Works offline after setup", "Best default for one device"],
  },
  {
    id: "remote",
    title: "Use another VoiceTypr",
    eyebrow: "Network source",
    description: "Connect to a stronger device or workstation running VoiceTypr on your network.",
    icon: Network,
    bullets: ["Skip local model download", "Use a faster machine", "Great for weak laptops"],
  },
];

const isRemoteServerOnline = (server?: SavedConnection | null) =>
  server?.status === "Online";

const sourceLabel = (sourceType: SourceType, confirmed: boolean) =>
  confirmed ? (sourceType === "local" ? "This device" : "Remote VoiceTypr") : "Choose source";
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
  const [sourceType, setSourceType] = useState<SourceType>("local");
  const [sourceConfirmed, setSourceConfirmed] = useState(false);
  const sourceConfirmedRef = useRef(false);
  const [hotkey, setHotkey] = useState(
    settings?.hotkey || "CommandOrControl+Shift+Space",
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
          ] satisfies Step[]
        : [
            "welcome",
            "source",
            "readiness",
            "hotkey",
            "first_transcription",
            "success",
          ] satisfies Step[],
    [],
  );

  const currentIndex = steps.indexOf(currentStep);
  const progress = ((currentIndex + 1) / steps.length) * 100;
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
      ? "The selected remote VoiceTypr is offline. Make sure sharing is enabled on that device, keep both devices on the same network, then go back and choose an online server."
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
      toast.error("Remote VoiceTypr is not online yet");
      return;
    }

    await invoke("set_active_remote_server", { serverId });
    setActiveRemoteServerId(serverId);
    confirmSource("remote");
    toast.success("Remote VoiceTypr selected");
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
      toast.error("Remote VoiceTypr was added, but could not be selected");
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
      toast.error(error instanceof Error ? error.message : "Failed to add remote VoiceTypr");
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
      //    holdToTalk OFF → isolated_tap  / toggle_recording (tap-to-toggle).
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
            trigger_kind: "isolated_tap",
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

  const completeOnboarding = async () => {
    setIsSavingCompletion(true);
    onCompletionStart?.();
    try {
      await updateSettings({ onboarding_completed: true });
      onComplete();
    } catch (error) {
      onCompletionError?.();
      log.error("Failed to complete onboarding:", error);
      toast.error("Failed to finish onboarding. Please try again.");
    } finally {
      setIsSavingCompletion(false);
    }
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
    <div className="min-h-screen overflow-hidden bg-[radial-gradient(circle_at_top_left,var(--color-voice-wash),transparent_34%),linear-gradient(180deg,var(--color-background),var(--color-muted))] text-foreground">
      {currentStep !== "success" && (
        <div className="mx-auto flex w-full max-w-5xl items-center gap-4 px-8 py-5">
          <div>
            <p className="text-sm font-semibold tracking-tight">VoiceTypr Setup</p>
            <p className="text-xs text-muted-foreground">{sourceLabel(sourceType, sourceConfirmed)}</p>
          </div>
          <Progress value={progress} className="h-2 flex-1" />
          <p className="text-xs tabular-nums text-muted-foreground">
            {currentIndex + 1}/{steps.length}
          </p>
        </div>
      )}

      <main className="mx-auto flex min-h-[calc(100vh-76px)] w-full max-w-5xl items-center justify-center px-8 pb-10">
        {currentStep === "welcome" && (
          <section className="grid w-full gap-8 lg:grid-cols-[1.1fr_0.9fr] lg:items-center">
            <div className="flex flex-col gap-6">
              <div className="flex flex-col gap-4">
                <h1 className="max-w-3xl text-5xl font-semibold tracking-[-0.045em] text-balance sm:text-6xl">
                  Welcome to VoiceTypr
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

            <Card className="border-border/70 bg-card/80 shadow-xl shadow-foreground/10 backdrop-blur">
              <CardHeader>
                <CardTitle className="flex items-center gap-2 text-xl">
                  <Sparkles className="size-5 text-primary" />
                  Setup completes when this works
                </CardTitle>
                <CardDescription>
                  No fake green check. VoiceTypr is ready only after the first recording succeeds.
                </CardDescription>
              </CardHeader>
              <CardContent className="flex flex-col gap-3">
                {[
                  ["1", "Pick local or remote transcription"],
                  ["2", "Prepare the selected source"],
                  ["3", "Record one sample and see the transcript"],
                ].map(([number, text]) => (
                  <div key={number} className="flex items-center gap-3 rounded-xl bg-muted/70 px-3 py-2">
                    <span className="flex size-7 items-center justify-center rounded-md bg-background text-sm font-medium ring-1 ring-border">
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
            eyebrow="Step 1"
            title="Where should transcription run?"
            description="Choose local transcription on this device or remote transcription from another VoiceTypr device."
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
                      "cursor-pointer border-border/70 bg-card/80 transition hover:-translate-y-0.5 hover:shadow-lg",
                      selected && "border-primary/60 bg-primary/5 ring-2 ring-primary/20",
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
                          <CircleCheck className="size-5 text-primary" />
                        ) : null}
                      </CardAction>
                      <div className="mb-2 flex size-11 items-center justify-center rounded-xl bg-muted text-primary">
                        <Icon className="size-5" />
                      </div>
                      <CardTitle className="text-xl">{option.title}</CardTitle>
                      <CardDescription>{option.description}</CardDescription>
                    </CardHeader>
                    <CardContent>
                      <Badge variant="secondary" className="mb-4 rounded-md">
                        {option.eyebrow}
                      </Badge>
                      <div className="flex flex-col gap-2">
                        {option.bullets.map((bullet) => (
                          <div key={bullet} className="flex items-center gap-2 text-sm text-muted-foreground">
                            <CheckCircle2 className="size-4 text-primary" />
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
            eyebrow="Step 2"
            title="Grant the permissions VoiceTypr actually needs"
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
                <Card key={perm.type} className="border-border/70 bg-card/80">
                  <CardHeader>
                    <div className="flex items-start justify-between gap-4">
                      <div className="flex gap-3">
                        <div
                          className={cn(
                            "flex size-10 items-center justify-center rounded-xl bg-muted text-primary",
                            perm.status === "granted" && "bg-primary/10",
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
                        <Badge variant="secondary" className="border-green-500/25 bg-green-500/10 text-green-700 dark:text-green-400">
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
            eyebrow="Step 3"
            title={sourceType === "local" ? "Prepare this device" : "Connect a remote VoiceTypr"}
            description={
              sourceType === "local"
                ? "Pick a downloaded local model, or download one now. The button unlocks as soon as the model is usable."
                : "Choose an online VoiceTypr server. Password-protected servers must pass the connection test before onboarding continues."
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
                <Card className="border-border/70 bg-card/80 py-0">
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
                        <EmptyState title="No local models available" description="Remote VoiceTypr is still available if you have another machine ready." />
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
                  <div className="flex items-center justify-between gap-3 rounded-lg border border-border/70 bg-card/80 px-4 py-3">
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

                <Card className="border-border/70 bg-card/80 py-0">
                  <ScrollArea className="h-[320px]">
                    <div className="flex flex-col gap-3 p-4">
                      {isLoadingRemoteServers && remoteServers.length === 0 ? (
                        <LoadingState label="Checking remote servers" />
                      ) : null}
                      {!isLoadingRemoteServers && remoteServers.length === 0 ? (
                        <div className="flex flex-col items-center gap-3">
                          <EmptyState
                            title="No remote servers saved"
                            description="Add a VoiceTypr server, or set up this device with a local model instead."
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
                          className="border-border/70 bg-background/60"
                        >
                          <CardHeader>
                            <CardAction>
                              <Badge variant={server.auth_required ? "outline" : "secondary"}>
                                {server.auth_required ? "Password required" : "Found on LAN"}
                              </Badge>
                            </CardAction>
                            <div className="flex items-start gap-3">
                              <div className="flex size-10 items-center justify-center rounded-xl bg-muted text-primary">
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
                              "border-border/70 bg-background/60",
                              selected && "border-primary/60 bg-primary/5 ring-2 ring-primary/20",
                            )}
                          >
                            <CardHeader>
                              <CardAction>
                                <Badge variant={online ? "secondary" : "outline"}>
                                  {online ? "Online" : server.status || "Unknown"}
                                </Badge>
                              </CardAction>
                              <div className="flex items-start gap-3">
                                <div className="flex size-10 items-center justify-center rounded-xl bg-muted text-primary">
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
            eyebrow="Step 4"
            title="Pick your hotkey and recording mode"
            description="This is the system-wide shortcut for triggering VoiceTypr. You can change both later in Settings."
            footer={
              <StepFooter
                onBack={handleBack}
                onNext={handleNext}
                nextDisabled={!canProceed()}
                nextLabel="Save hotkey"
              />
            }
          >
            <Card className="mx-auto w-full max-w-xl border-border/70 bg-card/80">
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Keyboard className="size-5 text-primary" />
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
                      : `Tap ${formatBareModifierLabel(capturedBareModifier)} · tap to toggle`
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
                      <AlertTitle>Tap to toggle on/off</AlertTitle>
                      <AlertDescription>
                        Tap {formatBareModifierLabel(capturedBareModifier)} once to start recording, tap again to stop.
                      </AlertDescription>
                    </Alert>
                  )
                ) : (
                  <Alert>
                    <Info className="size-4" />
                    <AlertTitle>Tip</AlertTitle>
                    <AlertDescription>
                      Use a shortcut you can hit without looking down. VoiceTypr works best when recording feels invisible.
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
            eyebrow="Final check"
            title="Do your first transcription"
            description="Say one short sentence. Onboarding only finishes after VoiceTypr returns real text."
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
            <Card className="mx-auto w-full max-w-2xl border-border/70 bg-card/80">
              <CardHeader>
                <CardAction>
                  <Badge variant="outline">{sourceLabel(sourceType, true)}</Badge>
                </CardAction>
                <CardTitle className="flex items-center gap-2 text-xl">
                  <Mic className="size-5 text-primary" />
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

                <div className="rounded-2xl border bg-muted/40 p-4 text-sm">
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
            <div className="flex size-16 items-center justify-center rounded-3xl bg-primary text-primary-foreground shadow-xl shadow-primary/20">
              <ShieldCheck className="size-8" />
            </div>
            <div className="flex flex-col gap-3">
              <Badge variant="secondary" className="mx-auto rounded-md uppercase tracking-[0.16em]">
                Ready
              </Badge>
              <h1 className="text-4xl font-semibold tracking-[-0.04em]">
                VoiceTypr is ready on {sourceLabel(sourceType, true).toLowerCase()}.
              </h1>
              <p className="text-muted-foreground">
                {capturedBareModifier
                  ? holdToTalk
                    ? <>Hold {formatBareModifierLabel(capturedBareModifier)} anywhere to start recording — release to stop.</>
                    : <>Tap {formatBareModifierLabel(capturedBareModifier)} anywhere to start or stop recording.</>
                  : holdToTalk
                    ? <>Hold {formatHotkey(hotkey)} anywhere to start recording — release to stop.</>
                    : <>Press {formatHotkey(hotkey)} anywhere to start recording.</>}
              </p>
            </div>
            <Button size="lg" onClick={() => void completeOnboarding()} disabled={isSavingCompletion}>
              {isSavingCompletion ? <Spinner /> : null}
              Go to dashboard
            </Button>
          </section>
        )}
      </main>
    </div>
  );
};

function OnboardingPanel({
  eyebrow,
  title,
  description,
  children,
  footer,
}: {
  eyebrow: string;
  title: string;
  description: string;
  children: React.ReactNode;
  footer: React.ReactNode;
}) {
  return (
    <section className="flex w-full flex-col gap-6 animate-fade-in">
      <div className="mx-auto flex max-w-3xl flex-col items-center gap-3 text-center">
        <Badge variant="secondary" className="rounded-md uppercase tracking-[0.16em]">
          {eyebrow}
        </Badge>
        <h2 className="text-4xl font-semibold tracking-[-0.04em] text-balance">
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
        <Zap className="size-3.5 text-primary" />
        Speed
      </span>
      <span className="flex items-center gap-1.5">
        <CheckCircle2 className="size-3.5 text-primary" />
        Accuracy
      </span>
      <span className="flex items-center gap-1.5">
        <HardDrive className="size-3.5 text-primary" />
        Size
      </span>
      <span className="flex items-center gap-1.5">
        <Star className="size-3.5 fill-primary text-primary" />
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
