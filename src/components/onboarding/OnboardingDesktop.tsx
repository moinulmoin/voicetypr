import { HotkeyInput } from "@/components/HotkeyInput";
import { ModelCard } from "@/components/ModelCard";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { formatHotkey } from "@/lib/hotkey-utils";
import { cn } from "@/lib/utils";
import { useModelManagement } from "@/hooks/useModelManagement";
import type { AppSettings } from "@/types";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import {
  Accessibility,
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
}

type Step = "welcome" | "permissions" | "models" | "setup" | "success";

const STEPS = [
  { id: "welcome" as const },
  { id: "permissions" as const },
  { id: "models" as const },
  { id: "setup" as const },
  { id: "success" as const }
];

export const OnboardingDesktop = function OnboardingDesktop({ onComplete }: OnboardingDesktopProps) {
  const [currentStep, setCurrentStep] = useState<Step>("welcome");
  const [permissions, setPermissions] = useState({
    microphone: "checking" as "checking" | "granted" | "denied",
    accessibility: "checking" as "checking" | "granted" | "denied"
  });
  const [hotkey, setHotkey] = useState("cmd+shift+space");
  const [isRequesting, setIsRequesting] = useState<string | null>(null);
  
  // Use the shared model management hook OR props
  // Always call the hook to satisfy React rules
  const hookData = useModelManagement({ windowId: "onboarding", showToasts: true });
  const [localSelectedModel, setLocalSelectedModel] = useState<string | null>(null);
  
  // Use hookData for now since modelManagement from props might not be loaded
  const {
    models,
    modelOrder,
    downloadProgress,
    selectedModel: _selectedModel,
    setSelectedModel: _setSelectedModel,
    loadModels,
    downloadModel,
    cancelDownload,
  } = hookData;
  
  // Use local state for selectedModel since onboarding needs its own selection
  const selectedModel = localSelectedModel;
  const setSelectedModel = setLocalSelectedModel;
  

  const steps = STEPS;

  const currentIndex = steps.findIndex((s) => s.id === currentStep);
  // const progress = ((currentIndex + 1) / steps.length) * 100;

  useEffect(() => {
    if (currentStep === "permissions") {
      checkPermissions();
      const interval = setInterval(checkPermissions, 2000);
      return () => clearInterval(interval);
    }
  }, [currentStep]);
  
  useEffect(() => {
    const hasModel = Object.values(models).some((m) => m.downloaded);
    if (hasModel && !selectedModel) {
      // Pre-select the first downloaded model
      const downloadedModel = Object.entries(models).find(([_, m]) => m.downloaded);
      if (downloadedModel) {
        setSelectedModel(downloadedModel[0]);
      }
    }
  }, [models, selectedModel, setSelectedModel]); // Re-check when models change
  


  const checkPermissions = async () => {
    try {
      const [mic, accessibility] = await Promise.all([
        invoke<boolean>("check_microphone_permission"),
        invoke<boolean>("check_accessibility_permission")
      ]);

      setPermissions({
        microphone: mic ? "granted" : "denied",
        accessibility: accessibility ? "granted" : "denied"
      });
    } catch (error) {
      console.error("Failed to check permissions:", error);
    }
  };

  const requestPermission = async (type: "microphone" | "accessibility") => {
    setIsRequesting(type);
    try {
      if (type === "microphone") {
        const granted = await invoke<boolean>("request_microphone_permission");
        if (!granted) {
          await open("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
        }
      } else {
        await invoke("request_accessibility_permission");
        await open("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
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
          await loadModels();
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
        return permissions.microphone === "granted" && permissions.accessibility === "granted";
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

                <div className="grid grid-cols-2 gap-4">
                  {[
                    {
                      type: "microphone" as const,
                      icon: Mic,
                      title: "Microphone",
                      desc: "Record your voice",
                      status: permissions.microphone
                    },
                    {
                      type: "accessibility" as const,
                      icon: Accessibility,
                      title: "Accessibility",
                      desc: "Insert text at cursor",
                      status: permissions.accessibility
                    }
                  ].map((perm) => (
                    <Card
                      key={perm.type}
                      className={cn(
                        "p-6 transition-colors",
                        perm.status === "granted" && "bg-green-500/5 border-green-500/50"
                      )}
                    >
                      <div className="flex items-center justify-between">
                        <div className="flex items-center gap-3">
                          <div
                            className={cn(
                              "p-2 rounded-lg",
                              perm.status === "granted"
                                ? "bg-green-500/10 text-green-500"
                                : "bg-primary/10 text-primary"
                            )}
                          >
                            <perm.icon className="h-5 w-5" />
                          </div>
                          <div>
                            <h3 className="font-medium">{perm.title}</h3>
                            <p className="text-sm text-muted-foreground">{perm.desc}</p>
                          </div>
                        </div>

                        <div className="flex items-center">
                          {perm.status === "checking" ? (
                            <Loader2 className="h-4 w-4 animate-spin" />
                          ) : perm.status === "granted" ? (
                            <div className="flex items-center gap-2 text-green-500">
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
                      {modelOrder.map((name) => {
                        const model = models[name];
                        if (!model) return null;
                        const progress = downloadProgress[name];
                        return (
                          <div key={name} className="relative">
                            <ModelCard
                            name={name}
                            model={model}
                            downloadProgress={progress}
                            isSelected={selectedModel === name}
                            onDownload={downloadModel}
                            onSelect={setSelectedModel}
                            onCancelDownload={cancelDownload}
                            showSelectButton={model.downloaded}
                          />
                          </div>
                        );
                      })}
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
