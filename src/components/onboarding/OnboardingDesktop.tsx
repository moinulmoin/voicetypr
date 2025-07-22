import { HotkeyInput } from "@/components/HotkeyInput";
import { ModelCard } from "@/components/ModelCard";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import type { useModelManagement } from "@/hooks/useModelManagement";
import { formatHotkey } from "@/lib/hotkey-utils";
import { cn } from "@/lib/utils";
import type { AppSettings } from "@/types";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import {
  CheckCircle,
  ChevronLeft,
  ChevronRight,
  Info,
  Keyboard,
  Loader2,
  Mic
} from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

interface OnboardingDesktopProps {
  onComplete: () => void;
  modelManagement: ReturnType<typeof useModelManagement>;
}

type Step = "welcome" | "permissions" | "models" | "setup" | "success";

const STEPS = [
  { id: "welcome" as const },
  { id: "permissions" as const },
  { id: "models" as const },
  { id: "setup" as const },
  { id: "success" as const }
];

type PermissionStatus = "checking" | "granted" | "denied" | "error";

interface PermissionState {
  status: PermissionStatus;
  error?: string;
}

export const OnboardingDesktop = function OnboardingDesktop({ onComplete, modelManagement }: OnboardingDesktopProps) {
  const [currentStep, setCurrentStep] = useState<Step>("welcome");
  const [permissions, setPermissions] = useState<Record<string, PermissionState>>({
    microphone: { status: "checking" },
    accessibility: { status: "checking" }
    // automation: { status: "checking" } // Removed for now, can be re-enabled later
  });
  const [hotkey, setHotkey] = useState("cmd+shift+space");
  const [isRequesting, setIsRequesting] = useState<string | null>(null);
  const [checkingPermissions, setCheckingPermissions] = useState<Set<string>>(new Set());

  // Get model management from props
  const {
    models,
    modelOrder,
    downloadProgress,
    verifyingModels,
    selectedModel,
    setSelectedModel,
    loadModels,
    downloadModel,
    cancelDownload,
    isLoading,
  } = modelManagement;


  const steps = STEPS;

  const currentIndex = steps.findIndex((s) => s.id === currentStep);
  // const progress = ((currentIndex + 1) / steps.length) * 100;

  useEffect(() => {
    if (currentStep !== "permissions") return
      checkPermissions();
      // No auto-retry - user manually retries via buttons

  }, [currentStep]);

  // Add manual recheck when user returns from settings
  useEffect(() => {
    if (currentStep !== "permissions") return
    const handleFocus = () => {
        checkPermissions();
    };

    window.addEventListener('focus', handleFocus);
    return () => window.removeEventListener('focus', handleFocus);
  }, [currentStep]);

  useEffect(() => {
    // Only auto-select if no model is selected yet
    if (!selectedModel) {
      const downloadedModel = Object.entries(models).find(([_, m]) => m.downloaded);
      if (downloadedModel) {
        setSelectedModel(downloadedModel[0]);
      }
    }
  }, [models]); // Only depend on models, not selectedModel to avoid loops



  const checkPermissions = async () => {
    // Check microphone and accessibility
    await Promise.all([
      checkSinglePermission("microphone", "check_microphone_permission"),
      checkSinglePermission("accessibility", "check_accessibility_permission")
    ]);

    // Automation check removed for now
    // Can be re-enabled later if needed
  };

  const checkSinglePermission = async (type: string, command: string) => {
    setCheckingPermissions(prev => new Set(prev).add(type));

    try {
      const granted = await invoke<boolean>(command);
      setPermissions(prev => ({
        ...prev,
        [type]: { status: granted ? "granted" : "denied" }
      }));
    } catch (error) {
      console.error(`Failed to check ${type} permission:`, error);
      setPermissions(prev => ({
        ...prev,
        [type]: {
          status: "error",
          error: `Failed to check ${type} permission`
        }
      }));
    } finally {
      setCheckingPermissions(prev => {
        const next = new Set(prev);
        next.delete(type);
        return next;
      });
    }
  };

  const requestPermission = async (type: "microphone" | "accessibility" | "automation") => {
    setIsRequesting(type);
    try {
      if (type === "microphone") {
        const granted = await invoke<boolean>("request_microphone_permission");
        setPermissions(prev => ({
          ...prev,
          microphone: { status: granted ? "granted" : "denied" }
        }));
        if (!granted) {
          await open("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
        }
      } else if (type === "accessibility") {
        await invoke("request_accessibility_permission");
        await open("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
        // For accessibility, we need to check the status after opening settings
        // as the permission dialog is handled differently
        const granted = await invoke<boolean>("check_accessibility_permission");
        setPermissions(prev => ({
          ...prev,
          accessibility: { status: granted ? "granted" : "denied" }
        }));
      } else if (type === "automation") {
        // This will trigger the system dialog for automation permission
        const granted = await invoke<boolean>("test_automation_permission");
        // Update the permission state immediately based on the result
        setPermissions(prev => ({
          ...prev,
          automation: { status: granted ? "granted" : "denied" }
        }));
        if (!granted) {
          // Open automation settings if permission denied
          await open("x-apple.systempreferences:com.apple.preference.security?Privacy_Automation");
        }
      }
    } catch (error) {
      console.error(`Failed to request ${type} permission:`, error);
    } finally {
      setIsRequesting(null);
    }
  };


  const saveSettings = async () => {
    try {
      await invoke("set_global_shortcut", { shortcut: hotkey });
      const settings = await invoke<AppSettings>("get_settings");
      await invoke("save_settings", {
        settings: {
          ...settings,
          hotkey: hotkey,
          current_model: selectedModel,
          onboarding_completed: true
        }
      });
    } catch (error) {
      console.error("Failed to save settings:", error);
      toast.error("Failed to save settings. Please try again.");
      throw error; // Re-throw to prevent navigation
    }
  };

  const handleNext = async () => {
    try {
      if (currentStep === "setup") {
        await saveSettings();
      }

      const nextIndex = currentIndex + 1;
      if (nextIndex < steps.length) {
        const nextStep = steps[nextIndex].id;
        setCurrentStep(nextStep);

        if (nextStep === "models") {
          console.log("[OnboardingDesktop] Navigating to models step, calling loadModels...");
          await loadModels();
          console.log("[OnboardingDesktop] loadModels completed");
        }
      }
    } catch (error) {
      // Error already handled in saveSettings
    }
  };

  const handleBack = () => {
    const prevIndex = currentIndex - 1;
    if (prevIndex >= 0) {
      const prevStep = steps[prevIndex].id;
      setCurrentStep(prevStep);
    }
  };

  const handleComplete = () => {
    onComplete();
  };

  const canProceed = () => {
    switch (currentStep) {
      case "permissions":
        return permissions.microphone.status === "granted" &&
               permissions.accessibility.status === "granted";
               // automation check removed for now
      case "models":
        // User can proceed if they have selected a model that is downloaded
        return selectedModel !== null && models[selectedModel]?.downloaded === true;
      default:
        return true;
    }
  };

  return (
    <div className="h-screen flex flex-col bg-background overflow-hidden">
      {/* Compact step indicators */}
      {currentStep !== "success" && (
        <div className="flex items-center justify-center gap-2 py-3 bg-muted/30">
          {steps.map((step, index) => (
            <div key={step.id} className="flex items-center">
              <div
                className={cn(
                  "w-2 h-2 rounded-full transition-all duration-300",
                  index < currentIndex
                    ? "bg-primary"
                    : index === currentIndex
                    ? "bg-primary scale-125"
                    : "bg-muted-foreground opacity-50"
                )}
              />
              {index < steps.length - 1 && (
                <div
                  className={cn(
                    "w-12 h-[1px] mx-1",
                    index < currentIndex ? "bg-primary" : "bg-muted-foreground/30"
                  )}
                />
              )}
            </div>
          ))}
        </div>
      )}

      {/* Content - constrained height */}
      <div className="flex-1 flex items-center justify-center p-6 overflow-hidden">
        <div className="w-full transition-opacity duration-300">
          {/* Welcome - Compact */}
          {currentStep === "welcome" && (
            <div className="w-full max-w-2xl mx-auto animate-fade-in">
              <div className="text-center space-y-6">
                <div className="space-y-2">
                  <h1 className="text-4xl font-bold">Welcome to VoiceTypr</h1>
                  <p className="text-lg text-muted-foreground max-w-lg mx-auto">
                    Write 5x faster with your voice
                  </p>
                </div>

                <Button onClick={handleNext} size="lg">
                  Get Started
                  <ChevronRight className="ml-2 h-4 w-4" />
                </Button>
              </div>
            </div>
          )}

          {/* Permissions - Side by side */}
          {currentStep === "permissions" && (
            <div className="w-full max-w-3xl mx-auto animate-fade-in">
              <div className="space-y-6">
                <div className="text-center space-y-1">
                  <h2 className="text-2xl font-bold">System Permissions</h2>
                  <p className="text-muted-foreground">Grant required permissions to continue</p>
                </div>

                <div className="flex flex-col justify-center items-center gap-4">
                  {[
                    {
                      type: "microphone" as const,
                      icon: Mic,
                      title: "Microphone",
                      desc: "Record your voice",
                      ...permissions.microphone
                    },
                    {
                      type: "accessibility" as const,
                      icon: Keyboard,
                      title: "Accessibility",
                      desc: "Global hotkeys",
                      ...permissions.accessibility
                    }
                    // Automation permission removed for now
                    // Can be re-enabled later if needed
                  ].map((perm) => (
                    <Card
                      key={perm.type}
                      className={cn(
                        "p-2.5 transition-colors max-w-fit",
                        perm.status === "granted" && "bg-green-500/5 "
                      )}
                    >
                      <div className="flex items-center justify-between gap-4">
                        <div className="flex items-center gap-3">
                          <div
                            className={cn(
                              "p-2.5 rounded-lg",
                              perm.status === "granted"
                                ? "bg-green-500/10 text-green-500"
                                : perm.status === "error"
                                ? "bg-red-500/10 text-red-500"
                                : "bg-primary/10 text-primary"
                            )}
                          >
                            <perm.icon className="h-5 w-5" />
                          </div>
                          <div>
                            <h3 className="font-medium w-30">{perm.title}</h3>
                            <p className="text-sm text-muted-foreground">{perm.desc}</p>
                            {perm.status === "error" && perm.error && (
                              <p className="text-xs text-red-500 mt-1">{perm.error}</p>
                            )}
                          </div>
                        </div>

                        <div className="flex items-center">
                          {checkingPermissions.has(perm.type) ? (
                            <Loader2 className="h-4 w-4 animate-spin" />
                          ) : perm.status === "granted" ? (
                            <div className="flex items-center gap-2 text-green-500">
                              <CheckCircle className="h-4 w-4" />
                              <span className="text-sm">Granted</span>
                            </div>
                          ) : perm.status === "error" ? (
                            <div className="flex items-center gap-2">
                              <Button
                                size="sm"
                                variant="ghost"
                                onClick={() => checkSinglePermission(
                                  perm.type,
                                  perm.type === "microphone" ? "check_microphone_permission" :
                                  perm.type === "accessibility" ? "check_accessibility_permission" :
                                  "test_automation_permission"
                                )}
                                disabled={checkingPermissions.has(perm.type)}
                              >
                                Retry
                              </Button>
                              <Button
                                size="sm"
                                variant="outline"
                                onClick={() => requestPermission(perm.type)}
                                disabled={isRequesting === perm.type}
                              >
                                {isRequesting === perm.type ? (
                                  <Loader2 className="h-4 w-4 animate-spin" />
                                ) : (
                                  "Grant"
                                )}
                              </Button>
                            </div>
                          ) : (
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => requestPermission(perm.type)}
                              disabled={isRequesting === perm.type}
                            >
                              {isRequesting === perm.type ? (
                                <Loader2 className="h-4 w-4 animate-spin" />
                              ) : (
                                "Grant Access"
                              )}
                            </Button>
                          )}
                        </div>
                      </div>
                    </Card>
                  ))}
                </div>

                <div className="flex gap-3 justify-center">
                  <Button variant="outline" onClick={handleBack} size="sm">
                    <ChevronLeft className="mr-1 h-4 w-4" />
                    Back
                  </Button>
                  <Button onClick={handleNext} disabled={!canProceed()} size="sm">
                    Continue
                    <ChevronRight className="ml-1 h-4 w-4" />
                  </Button>
                </div>
              </div>
            </div>
          )}

          {/* Models - List view */}
          {currentStep === "models" && (
            <div className="w-full max-w-3xl mx-auto animate-fade-in">
              <div className="space-y-6">
                <div className="text-center space-y-1">
                  <h2 className="text-2xl font-bold">Choose AI Model</h2>
                  <p className="text-muted-foreground">
                    Download and select a model for transcription
                  </p>
                </div>

                <div className="bg-card rounded-lg border">
                  <div className="max-h-[220px] overflow-y-auto">
                    <div className="space-y-3 p-4">
                      {modelOrder.map((name: string) => {
                        const model = models[name];
                        if (!model) return null;
                        const progress = downloadProgress[name];
                        return (
                          <div key={name} className="relative">
                            <ModelCard
                            name={name}
                            model={model}
                            downloadProgress={progress}
                            isVerifying={verifyingModels.has(name)}
                            isSelected={selectedModel === name}
                            onDownload={downloadModel}
                            onSelect={setSelectedModel}
                            onCancelDownload={cancelDownload}
                            showSelectButton={model.downloaded}
                          />
                          </div>
                        );
                      })}
                      {isLoading && modelOrder.length === 0 && (
                        <div className="text-center py-8 text-muted-foreground">
                          <div className="flex items-center justify-center">
                            <Loader2 className="h-6 w-6 animate-spin mr-2" />
                            <span>Loading models...</span>
                          </div>
                        </div>
                      )}
                      {!isLoading && modelOrder.length === 0 && (
                        <div className="text-center py-8 text-muted-foreground">
                          <p>No models available</p>
                        </div>
                      )}
                    </div>
                  </div>
                </div>

                <div className="flex gap-3 justify-center">
                  <Button variant="outline" onClick={handleBack} size="sm">
                    <ChevronLeft className="mr-1 h-4 w-4" />
                    Back
                  </Button>
                  <Button onClick={handleNext} disabled={!canProceed()} size="sm">
                    Continue
                    <ChevronRight className="ml-1 h-4 w-4" />
                  </Button>
                </div>
              </div>
            </div>
          )}

          {/* Setup - Compact */}
          {currentStep === "setup" && (
            <div className="w-full max-w-2xl mx-auto animate-fade-in">
              <div className="space-y-6">
                <div className="text-center space-y-1">
                  <h2 className="text-2xl font-bold">Quick Setup</h2>
                  <p className="text-muted-foreground">Configure your hotkey</p>
                </div>

                <div className="max-w-md mx-auto space-y-4">
                  <div className="space-y-2">
                    <label className="text-sm font-medium flex items-center gap-2">
                      <Keyboard className="h-4 w-4 text-primary" />
                      Recording Hotkey
                    </label>
                    <HotkeyInput value={hotkey} onChange={setHotkey} />
                  </div>

                  {/* <Card className="p-4 bg-border-primary/20"> */}
                  <div className="flex items-start gap-3 p-2">
                    <Info className="h-4 w-4 text-primary mt-0.5" />
                    <p className="text-sm text-muted-foreground">
                      Double tap ESC to cancel recording
                    </p>
                  </div>
                  {/* </Card> */}
                </div>

                <div className="flex gap-3 justify-center">
                  <Button variant="outline" onClick={handleBack} size="sm">
                    <ChevronLeft className="mr-1 h-4 w-4" />
                    Back
                  </Button>
                  <Button onClick={handleNext} size="sm">
                    Continue
                    <ChevronRight className="ml-1 h-4 w-4" />
                  </Button>
                </div>
              </div>
            </div>
          )}

          {/* Success - Simple */}
          {currentStep === "success" && (
            <div className="w-full max-w-md mx-auto animate-fade-in">
              <div className="text-center space-y-6">
                <div className="inline-flex p-4 bg-green-500/10 rounded-2xl animate-pulse-once">
                  <CheckCircle className="h-12 w-12 text-green-500" />
                </div>

                <div className="space-y-2">
                  <h1 className="text-3xl font-bold">You're all set!</h1>
                  <p className="text-muted-foreground">
                    Press {formatHotkey(hotkey)} to start recording
                  </p>
                </div>

                <Button onClick={handleComplete} size="lg" className="min-w-[200px]">
                  Go to dashboard
                </Button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
