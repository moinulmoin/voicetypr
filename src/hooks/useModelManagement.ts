import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { ModelInfo, isCloudModel } from "../types";
import { useEventCoordinator } from "./useEventCoordinator";
import { getModelDisplayName } from "@/lib/model-display";

interface UseModelManagementOptions {
  windowId?: "main" | "pill" | "onboarding";
  showToasts?: boolean;
}

const CLOUD_SPEED_FALLBACK = 8;
const CLOUD_ACCURACY_FALLBACK = 9;

const isDownloadCancellation = (error: unknown) =>
  String(error).toLowerCase().includes("cancelled") ||
  String(error).toLowerCase().includes("canceled");

const createDownloadRequestId = () => {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }

  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
};

const getSpeedScore = (model: ModelInfo) =>
  model.speed_score ?? (isCloudModel(model) ? CLOUD_SPEED_FALLBACK : 0);

const getAccuracyScore = (model: ModelInfo) =>
  model.accuracy_score ?? (isCloudModel(model) ? CLOUD_ACCURACY_FALLBACK : 0);

const getSizeValue = (model: ModelInfo) =>
  isCloudModel(model)
    ? Number.POSITIVE_INFINITY
    : (model.size ?? Number.POSITIVE_INFINITY);

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
  sortBy: "balanced" | "speed" | "accuracy" | "size" = "balanced",
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
  const cancelledDownloads = useRef(new Set<string>());
  const activeDownloadRequests = useRef(new Map<string, string>());
  const cancelledDownloadRequests = useRef(new Set<string>());
  const modelsRef = useRef<Record<string, ModelInfo>>({});

  const [models, setModels] = useState<Record<string, ModelInfo>>({});
  const [downloadProgress, setDownloadProgress] = useState<
    Record<string, number>
  >({});
  const [verifyingModels, setVerifyingModels] = useState<Set<string>>(
    new Set(),
  );

  // Removed selectedModel state - now using settings.current_model directly
  const [isLoading, setIsLoading] = useState(false);

  // Load models from backend
  const loadModels = useCallback(async () => {
    try {
      setIsLoading(true);
      console.log("[useModelManagement] Calling get_model_status...");
      const response = await invoke<{ models: ModelInfo[] }>(
        "get_model_status",
      );
      console.log("[useModelManagement] Response:", response);

      if (!response || !Array.isArray(response.models)) {
        throw new Error("Invalid response format from get_model_status");
      }

      const modelStatus = Object.fromEntries(
        response.models.map((model) => [model.name, model]),
      );
      setModels(modelStatus);
      modelsRef.current = modelStatus;

      return response.models;
    } catch (error) {
      console.error(
        "[useModelManagement.loadModels] Failed to load models:",
        error,
      );
      if (showToasts) {
        toast.error("Failed to load models");
      }
      return [];
    } finally {
      setIsLoading(false);
    }
  }, [showToasts]);

  // Download model
  const downloadModel = useCallback(
    async (modelName: string) => {
      const target = models[modelName];
      if (target && isCloudModel(target)) {
        if (showToasts) {
          toast.info(
            `${target.display_name} is a cloud model and does not require downloading.`,
          );
        }
        return;
      }

      // Check if already downloading
      if (activeDownloads.current.has(modelName)) {
        if (showToasts) {
          toast.info(`${getModelDisplayName(modelName, models)} is already downloading`);
        }
        return;
      }

      try {
        const requestId = createDownloadRequestId();

        // Mark as downloading
        activeDownloads.current.add(modelName);
        activeDownloadRequests.current.set(modelName, requestId);

        // Set initial progress to show download started
        setDownloadProgress((prev) => ({
          ...prev,
          [modelName]: 0,
        }));

        // Don't await - let it run async so progress events can update UI
        invoke("download_model", { modelName, requestId }).catch((error) => {
          if (cancelledDownloadRequests.current.has(requestId) || isDownloadCancellation(error)) {
            const isCurrentRequest = activeDownloadRequests.current.get(modelName) === requestId;
            if (isCurrentRequest) {
              activeDownloads.current.delete(modelName);
              activeDownloadRequests.current.delete(modelName);
            }
            cancelledDownloadRequests.current.add(requestId);
            if (isCurrentRequest) {
              setDownloadProgress((prev) => {
                const newProgress = { ...prev };
                delete newProgress[modelName];
                return newProgress;
              });
            }
            return;
          }

          if (activeDownloadRequests.current.get(modelName) !== requestId) {
            return;
          }

          console.error(
            "[useModelManagement.downloadModel] Failed to download model:",
            error,
          );
          if (showToasts) {
            toast.error(`Failed to download ${getModelDisplayName(modelName, models)}: ${error}`);
          }
          // Remove from progress on error
          setDownloadProgress((prev) => {
            const newProgress = { ...prev };
            delete newProgress[modelName];
            return newProgress;
          });
          // Remove from active downloads
          if (activeDownloadRequests.current.get(modelName) === requestId) {
            activeDownloads.current.delete(modelName);
            activeDownloadRequests.current.delete(modelName);
          }
        });
      } catch (error) {
        console.error(
          "[useModelManagement.downloadModel] Failed to start download:",
          error,
        );
        if (showToasts) {
          toast.error(`Failed to start ${getModelDisplayName(modelName, models)} download: ${error}`);
        }
        // Remove from active downloads
        activeDownloads.current.delete(modelName);
        activeDownloadRequests.current.delete(modelName);
      }
    },
    [models, showToasts],
  );

  // Cancel download
  const cancelDownload = useCallback(
    async (modelName: string) => {
      const requestId = activeDownloadRequests.current.get(modelName);

      try {
        cancelledDownloads.current.add(modelName);
        if (requestId) {
          cancelledDownloadRequests.current.add(requestId);
        }

        await invoke("cancel_download", { modelName });

        // Immediately remove from active downloads to allow retry
        activeDownloads.current.delete(modelName);
        activeDownloadRequests.current.delete(modelName);

        // Remove from progress tracking
        setDownloadProgress((prev) => {
          const newProgress = { ...prev };
          delete newProgress[modelName];
          return newProgress;
        });
      } catch (error) {
        cancelledDownloads.current.delete(modelName);
        if (requestId) {
          cancelledDownloadRequests.current.delete(requestId);
        }
        console.error("Failed to cancel download:", error);
        if (showToasts) {
          toast.error(`Failed to cancel ${getModelDisplayName(modelName, modelsRef.current)} download: ${error}`);
        }
      }
    },
    [showToasts],
  );

  // Delete model
  const deleteModel = useCallback(
    async (modelName: string) => {
      const target = models[modelName];
      if (target && isCloudModel(target)) {
        if (showToasts) {
          toast.info(
            `${target.display_name} is a cloud model and cannot be deleted locally.`,
          );
        }
        return;
      }

      try {
        const confirmed = await ask(
          `Are you sure you want to delete the ${getModelDisplayName(modelName, models)} model?`,
          {
            title: "Delete Model",
            kind: "warning",
          },
        );

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
          toast.error(`Failed to delete ${getModelDisplayName(modelName, models)}: ${error}`);
        }
      }
    },
    [loadModels, models, showToasts],
  );

  // Setup event listeners BEFORE any other effects
  useEffect(() => {
    let unregisterProgress: (() => void) | undefined;
    let unregisterVerifying: (() => void) | undefined;
    let unregisterComplete: (() => void) | undefined;
    let unregisterCancelled: (() => void) | undefined;

    const setupListeners = async () => {
      // Progress updates
      unregisterProgress = await registerEvent<{
        model: string;
        engine?: string;
        downloaded: number;
        total: number;
        progress: number;
        requestId?: string;
      }>("download-progress", (payload) => {
        const { model, progress, downloaded, total, engine, requestId } = payload;
        if (requestId && cancelledDownloadRequests.current.has(requestId)) {
          return;
        }

        const activeRequestId = activeDownloadRequests.current.get(model);
        if (requestId && activeRequestId && activeRequestId !== requestId) {
          return;
        }

        console.log(
          `[useModelManagement] Download progress for ${model} (${engine ?? "whisper"}): ${progress.toFixed(1)}% (${downloaded}/${total} bytes)`,
        );

        // Keep updating progress until we receive the verifying event
        setDownloadProgress((prev) => ({
          ...prev,
          [model]: Math.min(progress, 100), // Ensure progress doesn't exceed 100%
        }));
      });

      // Model verifying (after download, before verification)
      unregisterVerifying = await registerEvent<{
        model: string;
        engine?: string;
        requestId?: string;
      }>("model-verifying", (event) => {
        const modelName = event.model;
        const requestId = event.requestId;
        if (requestId && cancelledDownloadRequests.current.has(requestId)) {
          return;
        }

        const activeRequestId = activeDownloadRequests.current.get(modelName);
        if (requestId && activeRequestId && activeRequestId !== requestId) {
          return;
        }

        // First ensure the progress shows 100% before transitioning to verification
        setDownloadProgress((prev) => ({
          ...prev,
          [modelName]: 100,
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
      unregisterComplete = await registerEvent<{
        model: string;
        engine?: string;
        requestId?: string;
      }>("model-downloaded", async (event) => {
        const modelName = event.model;
        const requestId = event.requestId;
        const isCancelledCompletion = requestId
          ? cancelledDownloadRequests.current.has(requestId)
          : cancelledDownloads.current.has(modelName);

        if (isCancelledCompletion) {
          if (requestId) {
            cancelledDownloadRequests.current.delete(requestId);
          }
          cancelledDownloads.current.delete(modelName);
          if (!requestId || activeDownloadRequests.current.get(modelName) === requestId) {
            activeDownloads.current.delete(modelName);
            activeDownloadRequests.current.delete(modelName);
          }
          setDownloadProgress((prev) => {
            const newProgress = { ...prev };
            delete newProgress[modelName];
            return newProgress;
          });
          return;
        }

        const activeRequestId = activeDownloadRequests.current.get(modelName);
        if (requestId && activeRequestId && activeRequestId !== requestId) {
          return;
        }

        // Remove from active downloads
        activeDownloads.current.delete(modelName);
        activeDownloadRequests.current.delete(modelName);
        setModels((prev) => {
          const existing = prev[modelName];
          if (!existing) {
            return prev;
          }

          return {
            ...prev,
            [modelName]: {
              ...existing,
              downloaded: true,
              requires_setup: false,
            },
          };
        });

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
          toast.success(`${getModelDisplayName(modelName, modelsRef.current)} downloaded successfully`);
        }
      });

      // Download cancelled
      unregisterCancelled = await registerEvent<{
        model: string;
        engine?: string;
        requestId?: string;
      }>("download-cancelled", (payload) => {
        const modelName = typeof payload === "string" ? payload : payload.model;
        const requestId = typeof payload === "string" ? undefined : payload.requestId;
        const isCurrentRequest =
          !requestId || activeDownloadRequests.current.get(modelName) === requestId;
        // Remove from active downloads
        if (isCurrentRequest) {
          activeDownloads.current.delete(modelName);
          activeDownloadRequests.current.delete(modelName);
        }
        cancelledDownloads.current.add(modelName);
        if (requestId) {
          cancelledDownloadRequests.current.add(requestId);
        }

        // Remove from progress tracking
        if (isCurrentRequest) {
          setDownloadProgress((prev) => {
            const newProgress = { ...prev };
            delete newProgress[modelName];
            return newProgress;
          });
        }

        if (showToasts) {
          toast.info(`Download cancelled for ${getModelDisplayName(modelName, modelsRef.current)}`);
        }
      });
    };

    setupListeners();

    // Cleanup
    return () => {
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
    sortedModels: sortedModelsArray,
  };
}
