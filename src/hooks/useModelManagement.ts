import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { useEventCoordinator } from "./useEventCoordinator";
import { ModelInfo } from "../types";

interface UseModelManagementOptions {
  windowId?: "main" | "pill";
  showToasts?: boolean;
}

// Helper function to calculate balanced performance score
function calculateBalancedScore(model: ModelInfo): number {
  // Weighted average: 40% speed, 60% accuracy
  return ((model.speed_score * 0.4 + model.accuracy_score * 0.6) / 10) * 100;
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
        return b.speed_score - a.speed_score;
      case "accuracy":
        return a.accuracy_score - b.accuracy_score;
      case "size":
        return a.size - b.size;
      case "balanced":
      default:
        return calculateBalancedScore(b) - calculateBalancedScore(a);
    }
  });
}

export function useModelManagement(options: UseModelManagementOptions = {}) {
  const { windowId = "main", showToasts = true } = options;
  const { registerEvent } = useEventCoordinator(windowId);
  
  
  
  
  const [models, setModels] = useState<Record<string, ModelInfo>>({});
  const [modelOrder, setModelOrder] = useState<string[]>([]);
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({});
  const [selectedModel, setSelectedModel] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  // Load models from backend
  const loadModels = useCallback(async () => {
    try {
      setIsLoading(true);
      console.log("[useModelManagement.loadModels] Loading models from backend...");
      const modelStatusArray = await invoke<[string, ModelInfo][]>("get_model_status");
      console.log("[useModelManagement.loadModels] Received from backend:", modelStatusArray);
      
      // Log each model's status
      modelStatusArray.forEach(([name, info]) => {
        console.log(`[useModelManagement.loadModels] Model '${name}': downloaded=${info.downloaded}`);
      });
      
      // Convert array to object for compatibility
      const modelStatus = Object.fromEntries(modelStatusArray);
      const order = modelStatusArray.map(([name]) => name);
      
      console.log("[useModelManagement.loadModels] Setting models state...");
      setModels(modelStatus);
      setModelOrder(order);
      
      return modelStatusArray;
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
    try {
      console.log(`[useModelManagement.downloadModel] Starting download for model: ${modelName}`);
      
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
      });
    } catch (error) {
      console.error("[useModelManagement.downloadModel] Failed to start download:", error);
      if (showToasts) {
        toast.error(`Failed to start download: ${error}`);
      }
    }
  }, [showToasts]);

  // Cancel download
  const cancelDownload = useCallback(async (modelName: string) => {
    try {
      console.log(`Cancelling download for model: ${modelName}`);
      await invoke("cancel_download", { modelName });
      
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
    console.log("deleteModel called with:", modelName);
    try {
      const confirmed = await ask(`Are you sure you want to delete the ${modelName} model?`, {
        title: "Delete Model",
        kind: "warning"
      });

      if (!confirmed) {
        return;
      }

      console.log("Calling delete_model command...");
      await invoke("delete_model", { modelName });
      console.log("delete_model command completed");

      // Refresh model status
      await loadModels();
      
      // If deleted model was the current one, clear selection
      if (selectedModel === modelName) {
        setSelectedModel(null);
      }
    } catch (error) {
      console.error("Failed to delete model:", error);
      if (showToasts) {
        toast.error(`Failed to delete model: ${error}`);
      }
    }
  }, [selectedModel, loadModels, showToasts]);

  // Setup event listeners BEFORE any other effects
  useEffect(() => {
    let unregisterProgress: (() => void) | undefined;
    let unregisterComplete: (() => void) | undefined;
    let unregisterCancelled: (() => void) | undefined;

    const setupListeners = async () => {
      // Progress updates
      unregisterProgress = await registerEvent<{ model: string; downloaded: number; total: number; progress: number }>(
        "download-progress",
        (payload) => {
          const { model, progress } = payload;
          
          // If progress reaches 100%, remove from download progress
          // The model-downloaded event will handle setting it as downloaded
          if (progress >= 100) {
            setDownloadProgress((prev) => {
              const newProgress = { ...prev };
              delete newProgress[model];
              return newProgress;
            });
          } else {
            setDownloadProgress((prev) => ({
              ...prev,
              [model]: progress
            }));
          }
        }
      );

      // Download complete
      unregisterComplete = await registerEvent<{ model: string }>("model-downloaded", async (event) => {
        console.log("[useModelManagement] Model downloaded event received:", event);
        const modelName = event.model;
        console.log("[useModelManagement] Model name:", modelName);
        
        // Remove from progress tracking
        setDownloadProgress((prev) => {
          const newProgress = { ...prev };
          delete newProgress[modelName];
          return newProgress;
        });
        
        // Refresh model list
        await loadModels();
        
        if (showToasts) {
          toast.success(`Model ${modelName} downloaded successfully`);
        }
      });

      // Download cancelled
      unregisterCancelled = await registerEvent<string>("download-cancelled", (modelName) => {
        console.log("Download cancelled for model:", modelName);
        
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
      unregisterProgress?.();
      unregisterComplete?.();
      unregisterCancelled?.();
    };
  }, [registerEvent, loadModels, showToasts]);

  // Load models on mount
  useEffect(() => {
    loadModels();
  }, [loadModels]);


  return {
    // State
    models,
    modelOrder,
    downloadProgress,
    selectedModel,
    isLoading,
    
    // Actions
    setSelectedModel,
    loadModels,
    downloadModel,
    cancelDownload,
    deleteModel,
    
    // Utils
    sortedModels: sortModels(Object.entries(models), "accuracy")
  };
}