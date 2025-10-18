import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { ModelInfo, isCloudModel } from "../types";
import { useEventCoordinator } from "./useEventCoordinator";

interface UseModelManagementOptions {
  windowId?: "main" | "pill" | "onboarding";
  showToasts?: boolean;
}

const CLOUD_SPEED_FALLBACK = 8;
const CLOUD_ACCURACY_FALLBACK = 9;

const getSpeedScore = (model: ModelInfo) =>
  model.speed_score ?? (isCloudModel(model) ? CLOUD_SPEED_FALLBACK : 0);

const getAccuracyScore = (model: ModelInfo) =>
  model.accuracy_score ?? (isCloudModel(model) ? CLOUD_ACCURACY_FALLBACK : 0);

const getSizeValue = (model: ModelInfo) =>
  isCloudModel(model) ? Number.POSITIVE_INFINITY : model.size ?? Number.POSITIVE_INFINITY;

// Helper function to calculate balanced performance score
function calculateBalancedScore(model: ModelInfo): number {
  const speed = getSpeedScore(model);
  const accuracy = getAccuracyScore(model);
  // Weighted average: 40% speed, 60% accuracy
  return ((speed * 0.4 + accuracy * 0.6) / 10) * 100;
}

// Helper function to sort models by various criteria
export function sortModels(
  models: [string, ModelInfo][],
  sortBy: "balanced" | "speed" | "accuracy" | "size" = "balanced"
): [string, ModelInfo][] {
  return [...models].sort(([, a], [, b]) => {
    // Sort by the specified criteria
    switch (sortBy) {
      case "speed":
        return getSpeedScore(b) - getSpeedScore(a);
      case "accuracy":
        return getAccuracyScore(a) - getAccuracyScore(b);
      case "size":
        if (isCloudModel(a) && isCloudModel(b)) return 0;
        if (isCloudModel(a)) return 1;
        if (isCloudModel(b)) return -1;
        return getSizeValue(a) - getSizeValue(b);
      case "balanced":
      default:
        return calculateBalancedScore(b) - calculateBalancedScore(a);
    }
  });
}

export function useModelManagement(options: UseModelManagementOptions = {}) {
  const { windowId = "main", showToasts = true } = options;
  const { registerEvent } = useEventCoordinator(windowId);

  // Track active downloads to prevent duplicates
  const activeDownloads = useRef(new Set<string>());

  const [models, setModels] = useState<Record<string, ModelInfo>>({});
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({});
  const [verifyingModels, setVerifyingModels] = useState<Set<string>>(new Set());

  // Removed selectedModel state - now using settings.current_model directly
  const [isLoading, setIsLoading] = useState(false);

  // Load models from backend
  const loadModels = useCallback(async () => {
    try {
      setIsLoading(true);
      console.log("[useModelManagement] Calling get_model_status...");
      const response = await invoke<{ models: ModelInfo[] }>("get_model_status");
      console.log("[useModelManagement] Response:", response);

      if (!response || !Array.isArray(response.models)) {
        throw new Error("Invalid response format from get_model_status");
      }

      const modelStatus = Object.fromEntries(
        response.models.map((model) => [model.name, model])
      );
      setModels(modelStatus);

      return response.models;
    } catch (error) {
      console.error("[useModelManagement.loadModels] Failed to load models:", error);
      if (showToasts) {
        toast.error("Failed to load models");
      }
      return [];
    } finally {
      setIsLoading(false);
    }
  }, [showToasts]);

  // Download model
  const downloadModel = useCallback(async (modelName: string) => {
    const target = models[modelName];
    if (target && isCloudModel(target)) {
      if (showToasts) {
        toast.info(`${target.display_name} is a cloud model and does not require downloading.`);
      }
      return;
    }

    // Check if already downloading
    if (activeDownloads.current.has(modelName)) {
      if (showToasts) {
        toast.info(`Model ${modelName} is already downloading`);
      }
      return;
    }

    try {
      // Mark as downloading
      activeDownloads.current.add(modelName);

      // Set initial progress to show download started
      setDownloadProgress((prev) => ({
        ...prev,
        [modelName]: 0
      }));

      // Don't await - let it run async so progress events can update UI
      invoke("download_model", { modelName }).catch((error) => {
        console.error("[useModelManagement.downloadModel] Failed to download model:", error);
        if (showToasts) {
          toast.error(`Failed to download model: ${error}`);
        }
        // Remove from progress on error
        setDownloadProgress((prev) => {
          const newProgress = { ...prev };
          delete newProgress[modelName];
          return newProgress;
        });
        // Remove from active downloads
        activeDownloads.current.delete(modelName);
      });
    } catch (error) {
      console.error("[useModelManagement.downloadModel] Failed to start download:", error);
      if (showToasts) {
        toast.error(`Failed to start download: ${error}`);
      }
      // Remove from active downloads
      activeDownloads.current.delete(modelName);
    }
  }, [models, showToasts]);

  // Cancel download
  const cancelDownload = useCallback(async (modelName: string) => {
    try {
      await invoke("cancel_download", { modelName });

      // Immediately remove from active downloads to allow retry
      activeDownloads.current.delete(modelName);

      // Remove from progress tracking
      setDownloadProgress((prev) => {
        const newProgress = { ...prev };
        delete newProgress[modelName];
        return newProgress;
      });
    } catch (error) {
      console.error("Failed to cancel download:", error);
      if (showToasts) {
        toast.error(`Failed to cancel download: ${error}`);
      }
    }
  }, [showToasts]);

  // Delete model
  const deleteModel = useCallback(async (modelName: string) => {
    const target = models[modelName];
    if (target && isCloudModel(target)) {
      if (showToasts) {
        toast.info(`${target.display_name} is a cloud model and cannot be deleted locally.`);
      }
      return;
    }

    try {
      const confirmed = await ask(`Are you sure you want to delete the ${modelName} model?`, {
        title: "Delete Model",
        kind: "warning"
      });

      if (!confirmed) {
        return;
      }

      await invoke("delete_model", { modelName });

      // Refresh model status
      await loadModels();

      // Model selection clearing is handled by the component via settings
    } catch (error) {
      console.error("Failed to delete model:", error);
      if (showToasts) {
        toast.error(`Failed to delete model: ${error}`);
      }
    }
  }, [loadModels, models, showToasts]);

  // Setup event listeners BEFORE any other effects
  useEffect(() => {
    let unregisterProgress: (() => void) | undefined;
    let unregisterVerifying: (() => void) | undefined;
    let unregisterComplete: (() => void) | undefined;
    let unregisterCancelled: (() => void) | undefined;

    const setupListeners = async () => {
      // DEBUG: Add direct listener to verify events are reaching frontend
      if (typeof window !== 'undefined') {
        const { listen } = await import('@tauri-apps/api/event');

        // Direct debug listener bypassing EventCoordinator
        const debugUnlisten = await listen('download-progress', (event) => {
          console.log('[DEBUG] Direct download-progress event received:', event);
        });

        // Clean up debug listener on unmount
        const originalCleanup = () => {
          debugUnlisten();
        };

        // Store for cleanup
        (window as any).__debugUnlisten = originalCleanup;
      }
      // Progress updates
      unregisterProgress = await registerEvent<{ model: string; engine?: string; downloaded: number; total: number; progress: number }>(
        "download-progress",
        (payload) => {
          const { model, progress, downloaded, total, engine } = payload;

          console.log(`[useModelManagement] Download progress for ${model} (${engine ?? 'whisper'}): ${progress.toFixed(1)}% (${downloaded}/${total} bytes)`);

          // Keep updating progress until we receive the verifying event
          setDownloadProgress((prev) => ({
            ...prev,
            [model]: Math.min(progress, 100) // Ensure progress doesn't exceed 100%
          }));
        }
      );

      // Model verifying (after download, before verification)
      unregisterVerifying = await registerEvent<{ model: string; engine?: string }>("model-verifying", (event) => {
        const modelName = event.model;

        // First ensure the progress shows 100% before transitioning to verification
        setDownloadProgress((prev) => ({
          ...prev,
          [modelName]: 100
        }));

        // Add to verifying set
        setVerifyingModels((prev) => new Set(prev).add(modelName));

        // Remove from download progress after a brief delay to ensure UI updates
        setTimeout(() => {
          setDownloadProgress((prev) => {
            const newProgress = { ...prev };
            delete newProgress[modelName];
            return newProgress;
          });
        }, 500);
      });

      // Download complete
      unregisterComplete = await registerEvent<{ model: string; engine?: string }>("model-downloaded", async (event) => {
        const modelName = event.model;

        // Remove from active downloads
        activeDownloads.current.delete(modelName);

        // Remove from progress tracking and verifying
        setDownloadProgress((prev) => {
          const newProgress = { ...prev };
          delete newProgress[modelName];
          return newProgress;
        });
        
        // Remove from verifying set
        setVerifyingModels((prev) => {
          const newSet = new Set(prev);
          newSet.delete(modelName);
          return newSet;
        });

        // Refresh model list
        await loadModels();

        if (showToasts) {
          toast.success(`Model ${modelName} downloaded successfully`);
        }
      });

      // Download cancelled
      unregisterCancelled = await registerEvent<{ model: string; engine?: string }>("download-cancelled", (payload) => {
        const modelName = typeof payload === 'string' ? payload : payload.model;
        // Remove from active downloads
        activeDownloads.current.delete(modelName);

        // Remove from progress tracking
        setDownloadProgress((prev) => {
          const newProgress = { ...prev };
          delete newProgress[modelName];
          return newProgress;
        });

        if (showToasts) {
          toast.info(`Download cancelled for ${modelName}`);
        }
      });
    };

    setupListeners();

    // Cleanup
    return () => {
      // Clean up debug listener
      if ((window as any).__debugUnlisten) {
        (window as any).__debugUnlisten();
        delete (window as any).__debugUnlisten;
      }
      unregisterProgress?.();
      unregisterVerifying?.();
      unregisterComplete?.();
      unregisterCancelled?.();
    };
  }, [registerEvent, loadModels, showToasts]);

  // Load models on mount
  useEffect(() => {
    loadModels();
  }, [loadModels]);

  // Derive model order from sorted models
  const sortedModelsArray = sortModels(Object.entries(models), "accuracy");
  const modelOrder = sortedModelsArray.map(([name]) => name);

  return {
    // State
    models,
    modelOrder,
    downloadProgress,
    verifyingModels,
    isLoading,

    // Actions
    loadModels,
    downloadModel,
    cancelDownload,
    deleteModel,

    // Utils
    sortedModels: sortedModelsArray
  };
}
