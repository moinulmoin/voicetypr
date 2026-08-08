import { ApiKeyModal } from "@/components/ApiKeyModal";
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
import { ScrollArea } from "@/components/ui/scroll-area";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useSettings } from "@/contexts/SettingsContext";
import { useAccessibilityPermission } from "@/hooks/useAccessibilityPermission";
import { useMicrophonePermission } from "@/hooks/useMicrophonePermission";
import type { useModelManagement } from "@/hooks/useModelManagement";
import { formatHotkey } from "@/lib/hotkey-utils";
import { isMacOS, isWindows } from "@/lib/platform";
import { getModelDisplayName } from "@/lib/model-display";
import {
  getCloudProviderByModel,
  isCloudEngine,
} from "@/lib/cloudProviders";
import { findActivePrimaryBinding } from "@/lib/shortcut-display";
import { cn } from "@/lib/utils";
import { ValidationPresets } from "@/lib/keyboard-normalizer";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import {
  Cloud,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
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
import { isCloudModel, isLocalModel, type ModelInfo } from "@/types";
import { createLogger } from "@/lib/logger";

const log = createLogger("onboarding");

const UPGRADE_URL = "https://voicetypr.com/#pricing"; // [Upgrade to Pro] opens this externally

interface OnboardingDesktopProps {
  onCompletionStart?: () => void;
  onCompletionError?: () => void;
  onComplete: (target?: "license") => void;
  licensed?: boolean;
  licenseLoading?: boolean;
  modelManagement: ReturnType<typeof useModelManagement>;
}

type Step =
  | "welcome"
  | "source"
  | "permissions"
  | "readiness"
  | "hotkey"
  | "success"
  | "upgrade";

type SourceType = "local" | "cloud" | "remote";
type PermissionStatus = "checking" | "granted" | "denied" | "error";

interface PermissionState {
  status: PermissionStatus;
  error?: string;
}

interface DiscoveredRemoteServer {
  name: string;
  host: string;
  port: number;
  model: string;
  auth_required: boolean;
  machine_id: string;
}

const READINESS_COPY: Record<SourceType, { title: string; description: string }> = {
  local: {
    title: "Choose a local model",
    description: "Select a downloaded model, or download one now.",
  },
  cloud: {
    title: "Connect a cloud provider",
    description: "Add an API key, then select that provider for transcription.",
  },
  remote: {
    title: "Connect a remote Voicetypr",
    description: "Choose an online Voicetypr server on your network.",
  },
};

const isRemoteServerOnline = (server?: SavedConnection | null) =>
  server?.status === "Online";

const sourceLabel = (sourceType: SourceType) =>
  sourceType === "local"
    ? "Local setup"
    : sourceType === "cloud"
      ? "Cloud setup"
      : "Remote setup";

const ONBOARDING_HOTKEY_VALIDATION = ValidationPresets.custom({
  minKeys: 1,
  requireModifier: false,
  requireModifierForMultiKey: true,
});

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
  licensed = false,
  licenseLoading = false,
  modelManagement,
}: OnboardingDesktopProps) {
  const { settings, updateSettings } = useSettings();
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
    loadModels,
    modelOrder,
    downloadProgress,
    verifyingModels,
    downloadErrors = {},
    downloadModel,
    cancelDownload,
    deleteModel,
    repairModel,
    isLoading,
  } = modelManagement;

  const [currentStep, setCurrentStep] = useState<Step>("welcome");
  // Both independent privacy choices are opt-out and default to checked.
  const [telemetryOptIn, setTelemetryOptIn] = useState(true);
  const [analyticsOptIn, setAnalyticsOptIn] = useState(true);
  const [sourceType, setSourceType] = useState<SourceType>(() =>
    isCloudEngine(settings?.current_model_engine ?? "") ? "cloud" : "local",
  );
  const [hotkey, setHotkey] = useState(settings?.hotkey || "");
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
  const [isSavingCompletion, setIsSavingCompletion] = useState(false);
  const [holdToTalk, setHoldToTalk] = useState(false);
  const [cloudModelSetup, setCloudModelSetup] = useState<string | null>(null);
  const [isSavingCloudKey, setIsSavingCloudKey] = useState(false);
  const hotkeyHydrated = useRef(false);
  const sourceChosenByUser = useRef(false);


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
            "success",
            "upgrade",
          ] satisfies Step[]
        : [
            "welcome",
            "source",
            "readiness",
            "hotkey",
            "success",
            "upgrade",
          ] satisfies Step[],
    [],
  );

  const currentIndex = steps.indexOf(currentStep);
  const selectedModelName = settings?.current_model || null;
  const selectedModel = selectedModelName ? models[selectedModelName] : null;
  const localModelNames = useMemo(
    () => modelOrder.filter((name) => models[name] && isLocalModel(models[name])),
    [modelOrder, models],
  );
  const cloudModelNames = useMemo(
    () => modelOrder.filter((name) => models[name] && isCloudModel(models[name])),
    [modelOrder, models],
  );
  const activeCloudProvider = cloudModelSetup
    ? getCloudProviderByModel(cloudModelSetup)
    : undefined;
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
  const localReady = Boolean(
    sourceType === "local" &&
      selectedModelName &&
      selectedModel &&
      isLocalModel(selectedModel) &&
      isModelReady(selectedModelName),
  );
  const cloudReady = Boolean(
    sourceType === "cloud" &&
      selectedModelName &&
      selectedModel &&
      isCloudModel(selectedModel) &&
      isModelReady(selectedModelName),
  );
  const hasDownloadedLocalModel = localModelNames.some((name) => isModelReady(name));
  const remoteReady = sourceType === "remote" && isRemoteServerOnline(activeRemoteServer);
  const sourceReady =
    sourceType === "local" ? localReady : sourceType === "cloud" ? cloudReady : remoteReady;

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
    if (currentStep !== "readiness" || sourceType !== "remote") return;
    void loadRemoteServers();
  }, [currentStep, sourceType, loadRemoteServers]);

  useEffect(() => {
    let cancelled = false;
    void invoke<string | null>("get_active_remote_server")
      .then((serverId) => {
        if (!cancelled && serverId) {
          setActiveRemoteServerId(serverId);
          if (!sourceChosenByUser.current) {
            setSourceType("remote");
          }
        }
      })
      .catch((error) => {
        log.error("[OnboardingDesktop] Failed to restore active remote server:", error);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!settings || hotkeyHydrated.current) return;
    if (settings.hotkey) {
      setHotkey(settings.hotkey);
      setCapturedBareModifier(null);
      hotkeyHydrated.current = true;
      return;
    }

    let cancelled = false;
    void invoke<ShortcutSettings>("get_shortcut_settings")
      .then((shortcutSettings) => {
        if (cancelled) return;
        const primary = findActivePrimaryBinding(shortcutSettings.bindings);
        if (primary?.modifier) {
          setCapturedBareModifier(primary.modifier);
          setHotkey("");
          setHoldToTalk(primary.action === "hold_to_record");
        } else {
          setHotkey("Alt+Space");
        }
        hotkeyHydrated.current = true;
      })
      .catch((error) => {
        if (cancelled) return;
        log.error("[OnboardingDesktop] Failed to restore configured hotkey:", error);
        setHotkey("Alt+Space");
        hotkeyHydrated.current = true;
      });
    return () => {
      cancelled = true;
    };
  }, [settings]);

  const checkPermissions = async () => {
    await Promise.all([checkMicPermission(), checkAccessPermission()]);
  };

  const confirmSource = (nextSourceType: SourceType) => {
    sourceChosenByUser.current = true;
    setSourceType(nextSourceType);
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
          await invoke("open_microphone_settings");
        }
      } else {
        const granted = await requestAccessPermission();
        if (!granted) {
          await invoke("open_accessibility_settings");
        }
      }
    } catch (error) {
      log.error(`Failed to request ${type} permission:`, error);
    } finally {
      setIsRequestingPermission(null);
    }
  };

  const selectModel = async (modelName: string, source: "local" | "cloud") => {
    const info: ModelInfo | undefined = models[modelName];
    if (!info) {
      toast.error("That model is not available");
      return;
    }
    await invoke("set_active_remote_server", { serverId: null });
    setActiveRemoteServerId(null);
    setSourceType(source);
    await updateSettings({
      current_model: modelName,
      current_model_engine: info.engine ?? "whisper",
    });
  };

  const selectLocalModel = (modelName: string) => selectModel(modelName, "local");

  const selectCloudModel = (modelName: string) => selectModel(modelName, "cloud");

  const handleCloudKeySubmit = async (apiKey: string) => {
    if (!activeCloudProvider) return;
    setIsSavingCloudKey(true);
    try {
      await activeCloudProvider.addKey(apiKey);
      await loadModels();
      await selectCloudModel(activeCloudProvider.modelName);
      setCloudModelSetup(null);
      toast.success(`${activeCloudProvider.providerName} connected`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast.error(`Failed to connect ${activeCloudProvider.providerName}: ${message}`);
    } finally {
      setIsSavingCloudKey(false);
    }
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
      //    holdToTalk OFF → isolated_tap / toggle_recording (single tap to toggle).
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
      // Save diagnostics first; analytics consent and its acknowledgement are
      // persisted atomically by the second command.
      try {
        await invoke("set_telemetry_consent", { enabled: telemetryOptIn });
        await invoke("set_product_analytics_consent", {
          enabled: analyticsOptIn,
        });
        await invoke("record_onboarding_completed");
      } catch (privacyError) {
        log.error("Failed to persist privacy choices:", privacyError);
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
        if (sourceType !== "remote") {
          await invoke("set_active_remote_server", { serverId: null });
          setActiveRemoteServerId(null);
        }
        setCurrentStep("hotkey");
        return;
      }

      if (currentStep === "hotkey") {
        await saveHotkeySettings();
        setCurrentStep("success");
        return;
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

  const canProceed = () => {
    switch (currentStep) {
      case "source":
        return true;
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
            <p className="text-xs text-muted-foreground">{sourceLabel(sourceType)}</p>
          </div>
          <StepDots currentIndex={currentIndex} total={steps.length} />
        </div>
      )}

      <main className="mx-auto flex min-h-[calc(100vh-76px)] w-full max-w-5xl items-center justify-center px-8 pb-10">
        {currentStep === "welcome" && (
          <section className="mx-auto flex w-full max-w-3xl flex-col items-center gap-8 text-center">
            <div className="flex flex-col gap-6">
              <div className="flex flex-col gap-4">
                <h1 className="max-w-3xl text-5xl font-semibold tracking-[-0.045em] text-balance sm:text-6xl">
                  Welcome to Voicetypr
                </h1>
                <p className="mx-auto max-w-2xl text-base leading-7 text-muted-foreground">
                  Choose where transcription runs, confirm system access, and keep or change your
                  current recording hotkey.
                </p>
                <p className="text-sm text-muted-foreground">
                  By continuing, you agree to our Terms and Privacy Policy.
                </p>
              </div>
              <div className="flex justify-center">
                <Button size="lg" onClick={handleNext}>
                  Start setup
                  <ChevronRight />
                </Button>
              </div>
            </div>
          </section>
        )}

        {currentStep === "source" && (
          <OnboardingPanel
            title="Choose where transcription runs"
            description="You can change this later from Models. Onboarding prepares the source you choose now."
            footer={
              <StepFooter
                onBack={handleBack}
                onNext={handleNext}
                nextDisabled={!canProceed()}
                nextLabel="Continue"
              />
            }
          >
            <ToggleGroup
              type="single"
              variant="outline"
              spacing={3}
              value={sourceType}
              onValueChange={(value) => {
                if (value === "local" || value === "cloud" || value === "remote") {
                  confirmSource(value);
                }
              }}
              aria-label="Transcription source"
              className="mx-auto grid h-auto w-full max-w-3xl grid-cols-1 bg-transparent md:grid-cols-3"
            >
              <ToggleGroupItem
                value="local"
                aria-label="Use a local model"
                className="h-full min-h-44 flex-col items-start justify-start gap-3 whitespace-normal rounded-2xl p-5 text-left data-[state=on]:border-sage/60 data-[state=on]:bg-sage-bg/50 data-[state=on]:text-foreground"
              >
                <span className="flex size-10 items-center justify-center rounded-xl bg-sage-bg text-sage">
                  <Laptop className="size-5" />
                </span>
                <span className="text-base font-semibold">Local</span>
                <span className="text-sm leading-6 text-muted-foreground">
                  Download a model and transcribe on this device. Works offline.
                </span>
              </ToggleGroupItem>
              <ToggleGroupItem
                value="cloud"
                aria-label="Use a cloud provider"
                className="h-full min-h-44 flex-col items-start justify-start gap-3 whitespace-normal rounded-2xl p-5 text-left data-[state=on]:border-sage/60 data-[state=on]:bg-sage-bg/50 data-[state=on]:text-foreground"
              >
                <span className="flex size-10 items-center justify-center rounded-xl bg-sage-bg text-sage">
                  <Cloud className="size-5" />
                </span>
                <span className="text-base font-semibold">Cloud</span>
                <span className="text-sm leading-6 text-muted-foreground">
                  Connect a provider with an API key. Audio is sent to that provider.
                </span>
              </ToggleGroupItem>
              <ToggleGroupItem
                value="remote"
                aria-label="Use another Voicetypr"
                className="h-full min-h-44 flex-col items-start justify-start gap-3 whitespace-normal rounded-2xl p-5 text-left data-[state=on]:border-sage/60 data-[state=on]:bg-sage-bg/50 data-[state=on]:text-foreground"
              >
                <span className="flex size-10 items-center justify-center rounded-xl bg-sage-bg text-sage">
                  <Network className="size-5" />
                </span>
                <span className="text-base font-semibold">Remote Voicetypr</span>
                <span className="text-sm leading-6 text-muted-foreground">
                  Use a model running in Voicetypr on another device on your network.
                </span>
              </ToggleGroupItem>
            </ToggleGroup>
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
                  desc: "Record your voice for transcription.",
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
            title={READINESS_COPY[sourceType].title}
            description={READINESS_COPY[sourceType].description}
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
                      {localModelNames.map((name: string) => {
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
                            onRepair={repairModel}
                            showSelectButton={isModelReady(name)}
                          />
                        );
                      })}
                      {isLoading && localModelNames.length === 0 ? (
                        <LoadingState label="Loading local models" />
                      ) : null}
                      {!isLoading && localModelNames.length === 0 ? (
                        <EmptyState title="No local models available" description="Choose Cloud or Remote Voicetypr to continue without a local model." />
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
            ) : null}

            {sourceType === "cloud" ? (
              <div className="flex flex-col gap-4">
                <Card className="rounded-2xl border border-border bg-card py-0 shadow-sm">
                  <ScrollArea className="h-[320px]">
                    <div className="flex flex-col gap-3 p-4">
                      {cloudModelNames.map((name) => {
                        const model = models[name];
                        const provider = getCloudProviderByModel(name);
                        if (!model || !provider) return null;
                        const ready = isModelReady(name);
                        const selected = settings?.current_model === name;
                        return (
                          <Card
                            key={name}
                            size="sm"
                            className={cn(
                              "rounded-xl border border-border bg-muted/30",
                              selected && "border-sage/50 bg-sage-bg/40 ring-1 ring-sage/30",
                            )}
                          >
                            <CardHeader>
                              <CardAction>
                                <Badge variant={ready ? "secondary" : "outline"}>
                                  {ready ? "Connected" : "API key required"}
                                </Badge>
                              </CardAction>
                              <div className="flex items-start gap-3">
                                <div className="flex size-10 items-center justify-center rounded-xl bg-sage-bg text-sage">
                                  <Cloud className="size-5" />
                                </div>
                                <div>
                                  <CardTitle>{provider.displayName}</CardTitle>
                                  <CardDescription>{provider.description}</CardDescription>
                                </div>
                              </div>
                            </CardHeader>
                            <CardFooter className="justify-end">
                              <Button
                                size="sm"
                                variant={selected ? "default" : "outline"}
                                onClick={() => {
                                  if (ready) {
                                    void selectCloudModel(name);
                                  } else {
                                    setCloudModelSetup(name);
                                  }
                                }}
                              >
                                {selected ? "Selected" : ready ? "Use provider" : "Add API key"}
                              </Button>
                            </CardFooter>
                          </Card>
                        );
                      })}
                      {isLoading && cloudModelNames.length === 0 ? (
                        <LoadingState label="Loading cloud providers" />
                      ) : null}
                      {!isLoading && cloudModelNames.length === 0 ? (
                        <EmptyState
                          title="No cloud providers available"
                          description="Choose Local or Remote Voicetypr to continue."
                        />
                      ) : null}
                    </div>
                  </ScrollArea>
                </Card>
                {activeCloudProvider ? (
                  <ApiKeyModal
                    isOpen
                    onClose={() => {
                      if (!isSavingCloudKey) setCloudModelSetup(null);
                    }}
                    onSubmit={(apiKey) => void handleCloudKeySubmit(apiKey)}
                    providerName={activeCloudProvider.providerName}
                    isLoading={isSavingCloudKey}
                    description={`Enter your ${activeCloudProvider.providerName} API key. It is stored securely in the system keychain.`}
                    docsUrl={activeCloudProvider.docsUrl}
                  />
                ) : null}
              </div>
            ) : null}

            {sourceType === "remote" ? (
              <div className="flex flex-col gap-4">
                <div className="flex items-center justify-between gap-4">
                  <div>
                    <p className="text-sm font-medium">Saved remote servers</p>
                    <p className="text-sm text-muted-foreground">
                      Online servers can be selected for transcription.
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
                            Choose local instead
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
            ) : null}
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
                      : `Tap ${formatBareModifierLabel(capturedBareModifier)} · toggle on/off`
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
                        Tap {formatBareModifierLabel(capturedBareModifier)} to start recording, tap again to stop.
                      </AlertDescription>
                    </Alert>
                  )
                ) : null}
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

        {currentStep === "success" && (
          <section className="mx-auto flex w-full max-w-xl flex-col items-center gap-6 text-center">
            <div className="flex size-16 items-center justify-center rounded-3xl bg-sage text-sage-foreground shadow-sm">
              <ShieldCheck className="size-8" />
            </div>
            <div className="flex flex-col gap-3">
              <h1 className="text-4xl font-semibold tracking-[-0.04em]">
                You're all set
              </h1>
              <p className="text-muted-foreground">
                Voicetypr is ready to use.{" "}
                {capturedBareModifier
                  ? holdToTalk
                    ? <>Hold {formatBareModifierLabel(capturedBareModifier)} anywhere to start recording; release to stop.</>
                    : <>Tap {formatBareModifierLabel(capturedBareModifier)} anywhere to start or stop recording.</>
                  : holdToTalk
                    ? <>Hold {formatHotkey(hotkey)} anywhere to start recording; release to stop.</>
                    : <>Press {formatHotkey(hotkey)} anywhere to start recording.</>}
              </p>
            </div>

            <p className="w-full rounded-2xl border border-border bg-card p-4 text-left text-sm text-muted-foreground shadow-sm">
              Tip: turn on Polish in Settings to clean up your dictation automatically.
            </p>

            <div className="flex w-full flex-col gap-3 text-left text-sm">
              <label className="flex items-start gap-3 rounded-2xl border border-border bg-card p-4 shadow-sm">
                <input
                  type="checkbox"
                  checked={telemetryOptIn}
                  onChange={(event) => setTelemetryOptIn(event.target.checked)}
                  className="mt-0.5 size-4 shrink-0 accent-[var(--sage)]"
                />
                <span className="text-muted-foreground">
                  <strong className="block font-medium text-foreground">
                    Crash &amp; error reporting
                  </strong>
                  Anonymous crash details go to GlitchTip. No audio, transcripts,
                  clipboard contents, or prompts.
                </span>
              </label>

              <label className="flex items-start gap-3 rounded-2xl border border-border bg-card p-4 shadow-sm">
                <input
                  type="checkbox"
                  checked={analyticsOptIn}
                  onChange={(event) => setAnalyticsOptIn(event.target.checked)}
                  className="mt-0.5 size-4 shrink-0 accent-[var(--sage)]"
                />
                <span className="text-muted-foreground">
                  <strong className="block font-medium text-foreground">
                    Usage analytics
                  </strong>
                  Anonymous feature usage, outcomes, and performance buckets with
                  PostHog. No session replay.
                </span>
              </label>
            </div>

            <Button
              size="lg"
              disabled={licenseLoading || (licensed && isSavingCompletion)}
              onClick={() => {
                if (licensed) {
                  void completeOnboarding();
                } else {
                  setCurrentStep("upgrade");
                }
              }}
            >
              {licenseLoading || (licensed && isSavingCompletion) ? <Spinner /> : null}
              {licenseLoading ? "Checking license..." : "Continue"}
              {!licenseLoading && !licensed ? <ChevronRight /> : null}
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
                You can upgrade to Pro anytime
              </h1>
              <p className="text-muted-foreground">
                Continue now, or connect an existing license.
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
                Continue
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
