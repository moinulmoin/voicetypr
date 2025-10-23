import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import {
  Upload,
  FileAudio,
  FileText,
  Loader2,
  Copy,
  Check,
  AlertCircle
} from "lucide-react";
import { toast } from "sonner";
// invoke handled inside zustand store
import { open } from "@tauri-apps/plugin-dialog";
import { useSettings } from "@/contexts/SettingsContext";
import { useModelAvailability } from "@/hooks/useModelAvailability";
import { listen } from "@tauri-apps/api/event";
import { cn } from "@/lib/utils";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useUploadStore } from "@/state/upload";

// local result type not needed; handled by store

export function AudioUploadSection() {
  const [copied, setCopied] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const { settings } = useSettings();
  const { selectedModelAvailable } = useModelAvailability();
  const {
    selectedFile,
    status,
    resultText,
    error: storeError,
    select,
    clearSelection,
    start,
    reset
  } = useUploadStore();
  const isProcessing = status === 'processing';
  const effectiveFileName = selectedFile?.name || null;
  const hasEffectiveSelection = !!selectedFile;

  const handleFileSelect = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: "Audio/Video Files",
            extensions: ["wav", "mp3", "m4a", "flac", "ogg", "mp4", "webm"]
          }
        ]
      });

      if (selected && typeof selected === 'string') {
        select(selected);
      }
    } catch (error) {
      console.error("Failed to select file:", error);
      toast.error("Failed to select file");
    }
  };

  const handleTranscribe = async () => {
    if (!selectedFile) {
      toast.error("Please select an audio file first");
      return;
    }

    if (!settings?.current_model) {
      toast.error("Select a speech model in Models before transcribing.");
      return;
    }

    if (selectedModelAvailable === false) {
      const engine = settings.current_model_engine || 'whisper';
      toast.error(
        engine === 'soniox'
          ? 'Connect your cloud provider before transcribing audio.'
          : 'Download the selected model before transcribing audio.'
      );
      return;
    }

    if (isProcessing) {
      toast.info("A transcription is already in progress");
      return;
    }

    await start(settings.current_model, settings.current_model_engine || 'whisper');

    try {
      if (storeError) {
        toast.error(storeError);
      } else if (status === 'done') {
        toast.success("Transcription completed and saved to history!");
      }
    } catch (error) {
      console.error("[Upload] Transcription handling failed:", error);
      toast.error(`Transcription failed: ${error}`);
    } finally {
      // keep store state; UI renders from it
    }
  };

  const handleCopy = async () => {
    if (resultText) {
      try {
        await navigator.clipboard.writeText(resultText);
        setCopied(true);
        toast.success("Copied to clipboard");
        setTimeout(() => setCopied(false), 2000);
      } catch (error) {
        console.error("Failed to copy:", error);
        toast.error("Failed to copy to clipboard");
      }
    }
  };

  const handleReset = () => {
    reset();
    setCopied(false);
  };

  // Handle file drop
  const handleFileDrop = (filePath: string) => {
    // Validate file extension
    const supportedExtensions = ['wav', 'mp3', 'm4a', 'flac', 'ogg', 'mp4', 'webm'];
    const fileExtension = filePath.split('.').pop()?.toLowerCase();

    if (!fileExtension || !supportedExtensions.includes(fileExtension)) {
      toast.error("Unsupported file format. Please drop an audio or video file.");
      return;
    }

    select(filePath);
  };

  // Setup drag and drop listeners
  useEffect(() => {
    // Listen for file drop events
    const unlisten = listen('tauri://drag-drop', (event) => {
      setIsDragging(false);

      const payload = event.payload as { paths: string[]; position: { x: number; y: number } };
      if (payload.paths && payload.paths.length > 0) {
        // Only take the first file if multiple are dropped
        handleFileDrop(payload.paths[0]);
      }
    });

    // Listen for drag over events
    const unlistenHover = listen('tauri://drag-hover', () => {
      setIsDragging(true);
    });

    // Listen for drag leave events
    const unlistenLeave = listen('tauri://drag-leave', () => {
      setIsDragging(false);
    });

    return () => {
      unlisten.then(fn => fn());
      unlistenHover.then(fn => fn());
      unlistenLeave.then(fn => fn());
    };
  }, []);

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="px-6 py-4 border-b border-border/40">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold">Upload files</h1>
            <p className="text-sm text-muted-foreground mt-1">
              Transcribe audio or video files locally
            </p>
          </div>
        </div>
      </div>

      <div className="flex-1 p-6">
        <div className="space-y-6">
          {/* Upload Card */}
          <div className={cn(
            "rounded-lg border-2 bg-card overflow-hidden transition-all",
            isDragging
              ? "border-primary bg-primary/5 scale-[1.02]"
              : "border-border/50"
          )}>
            <div className="p-6">
              <div className="space-y-4">
                {/* File Selection / Drop Zone */}
                {status !== 'done' && (
                    <div className="space-y-4">
                      {hasEffectiveSelection ? (
                        <div className="flex items-center justify-between p-3 rounded-lg bg-accent/50">
                          <div className="flex items-center gap-3">
                            <FileAudio className="h-4 w-4 text-muted-foreground" />
                            <span className="text-sm font-medium">
                              {effectiveFileName}
                            </span>
                          </div>
                          {!isProcessing && selectedFile && (
                            <Button
                              size="sm"
                              variant="ghost"
                              onClick={() => { clearSelection(); setCopied(false); }}
                            >
                              Change
                            </Button>
                          )}
                        </div>
                      ) : (
                        <div className={cn(
                          "relative rounded-lg border-2 border-dashed p-6 text-center transition-all",
                          isDragging
                            ? "border-primary bg-primary/5"
                            : "border-border/50 hover:border-border"
                        )}>
                          {isDragging ? (
                            <div className="space-y-1">
                              <Upload className="h-7 w-7 mx-auto text-primary animate-bounce" />
                              <p className="text-sm font-medium text-primary">
                                Drop your audio or video file here
                              </p>
                              <p className="text-xs text-muted-foreground">
                                WAV, MP3, M4A, FLAC, OGG, MP4, WebM
                              </p>
                            </div>
                          ) : (
                            <div className="space-y-3">
                              <div className="space-y-1">
                                <Upload className="h-7 w-7 mx-auto text-muted-foreground" />
                                <p className="text-sm font-medium">
                                  Drag & drop your audio or video file here
                                </p>
                                <p className="text-xs text-muted-foreground">
                                  or click to browse
                                </p>
                              </div>
                              <Button
                                onClick={handleFileSelect}
                                variant="outline"
                                className="mx-auto"
                                disabled={isProcessing}
                              >
                                <Upload className="h-4 w-4 mr-2" />
                                Select File
                              </Button>
                            </div>
                          )}
                        </div>
                      )}

                      {hasEffectiveSelection && (
                        <Button
                          onClick={handleTranscribe}
                          className="w-full"
                          disabled={isProcessing}
                        >
                          {isProcessing ? (
                            <>
                              <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                              Processing...
                            </>
                          ) : (
                            <>
                              <FileText className="h-4 w-4 mr-2" />
                              Transcribe
                            </>
                          )}
                        </Button>
                      )}
                  </div>
                )}

                {/* Transcription Result */}
                {status === 'done' && resultText && selectedFile && (
                    <div className="space-y-4">
                      <div className="p-4 rounded-lg bg-accent/30 space-y-3">
                        <div className="flex items-start gap-4">
                          <div className="flex-1">
                            <ScrollArea className="h-64">
                              <p className="text-sm leading-relaxed pr-2">
                                {resultText}
                              </p>
                            </ScrollArea>
                          </div>
                        </div>
                        <div className="flex items-center justify-between text-xs text-muted-foreground">
                          <span>{selectedFile.name}</span>
                          <span>{resultText.split(' ').length} words</span>
                        </div>
                      </div>

                      <div className="flex items-center justify-between gap-3">
                        <Button onClick={handleCopy} variant="outline">
                          {copied ? (
                            <>
                              <Check className="h-4 w-4 mr-2 text-green-500" /> Copied
                            </>
                          ) : (
                            <>
                              <Copy className="h-4 w-4 mr-2" /> Copy
                            </>
                          )}
                        </Button>

                        <Button onClick={handleReset} variant="outline">
                          Transcribe Another File
                        </Button>
                      </div>
                    </div>
                )}

                {status === 'error' && storeError && (
                    <div className="p-4 rounded-lg bg-amber-500/10 border border-amber-200/50 flex items-start gap-2">
                      <AlertCircle className="h-4 w-4 text-amber-600 mt-0.5" />
                      <div>
                        <p className="text-sm text-amber-700">{storeError}</p>
                      </div>
                    </div>
                )}
              </div>
            </div>
          </div>

          {/* Info Card */}
          <div className="rounded-lg border border-border/50 bg-card overflow-hidden">
            <div className="p-4">
              <div className="flex items-start gap-3">
                <div className="p-1.5 rounded-md bg-amber-500/10">
                  <AlertCircle className="h-4 w-4 text-amber-500" />
                </div>
                <div className="space-y-2 flex-1">
                  <h3 className="font-medium text-sm">Important Information</h3>
                  <div className="text-sm text-muted-foreground space-y-1">
                    <p>• <strong>Supported Formats:</strong> WAV, MP3, M4A, FLAC, OGG, MP4, WebM</p>
                    <p>• <strong>Conversion:</strong> Non-WAV files will be converted to 16 kHz mono WAV before transcription; this may take time</p>
                    <p>• <strong>Video:</strong> Video files are supported; audio is extracted first</p>
                    <p>• <strong>Processing:</strong> Happens locally on your device</p>
                    <p>• <strong>Duration:</strong> Longer media may take longer and use more memory</p>
                    <p className="text-amber-600 font-medium mt-2">
                      ⚠️ Long media (4-5+ hours) may take several minutes and use significant memory
                    </p>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
