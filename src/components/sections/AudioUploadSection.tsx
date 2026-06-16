import { useState, useEffect } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Upload,
  FileAudio,
  FileText,
  Loader2,
  Copy,
  Check,
  AlertCircle,
  HelpCircle,
  Download,
} from "lucide-react";
import { toast } from "sonner";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useSettings } from "@/contexts/SettingsContext";
import { useModelAvailability } from "@/hooks/useModelAvailability";
import { listen } from "@tauri-apps/api/event";
import { cn } from "@/lib/utils";
import { getModelDisplayName } from "@/lib/model-display";
import { isCloudEngine } from "@/lib/cloudProviders";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useUploadStore } from "@/state/upload";

// local result type not needed; handled by store

export function AudioUploadSection() {
  const [copied, setCopied] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [activeRemoteServer, setActiveRemoteServer] = useState<string | null>(null);
  const { settings } = useSettings();
  const { selectedModelAvailable, remoteAvailable } = useModelAvailability();
  const {
    selectedFile,
    status,
    resultText,
    error: storeError,
    speakerSegments,
    diarizationError,
    select,
    clearSelection,
    start,
    reset
  } = useUploadStore();
  const isProcessing = status === 'processing';
  const effectiveFileName = selectedFile?.name || null;
  const hasEffectiveSelection = !!selectedFile;
  const formatTimestamp = (milliseconds: number) => {
    const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000));
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${minutes}:${seconds.toString().padStart(2, "0")}`;
  };


  const resolveHistoryModelName = async (remoteServerIdOverride?: string | null) => {
    const effectiveRemoteId = remoteServerIdOverride ?? activeRemoteServer;
    if (!effectiveRemoteId) {
      return isCloudEngine(settings?.current_model_engine ?? 'whisper') ? getModelDisplayName(settings?.current_model) || '' : (getModelDisplayName(settings?.current_model) || '');
    }

    try {
      const servers = await invoke<Array<{
        id: string;
        name?: string;
        host: string;
        port: number;
      }>>('list_remote_servers');
      if (!Array.isArray(servers)) {
        return `Remote: ${effectiveRemoteId}`;
      }

      const activeServer = servers.find((server) => server.id === effectiveRemoteId);

      if (!activeServer) {
        return `Remote: ${effectiveRemoteId}`;
      }

      const displayName = activeServer.name || `${activeServer.host}:${activeServer.port}`;
      return `Remote: ${displayName}`;
    } catch (error) {
      console.error('Failed to resolve active remote server name:', error);
      return `Remote: ${effectiveRemoteId}`;
    }
  };

  useEffect(() => {
    const loadActiveRemoteServer = async () => {
      try {
        const activeId = await invoke<string | null>('get_active_remote_server');
        setActiveRemoteServer(activeId);
      } catch (error) {
        console.error('Failed to get active remote server:', error);
        setActiveRemoteServer(null);
      }
    };

    loadActiveRemoteServer();

    const unlistenModelChanged = listen('model-changed', loadActiveRemoteServer);
    const unlistenSharingChanged = listen('sharing-status-changed', loadActiveRemoteServer);

    return () => {
      unlistenModelChanged.then((fn) => fn());
      unlistenSharingChanged.then((fn) => fn());
    };
  }, []);

  const activeSourceLabel = activeRemoteServer
    ? "Remote VoiceTypr"
    : isCloudEngine(settings?.current_model_engine ?? 'whisper')
      ? getModelDisplayName(settings?.current_model) || "No source selected"
      : getModelDisplayName(settings?.current_model) || "No source selected";

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

    const latestActiveRemoteServer = await invoke<string | null>('get_active_remote_server')
      .catch(() => activeRemoteServer);
    const latestAvailability = await invoke<{ remote_available?: boolean } | null>('get_recognition_availability_snapshot')
      .catch(() => null);
    const effectiveRemoteSelected = !!latestActiveRemoteServer;
    const effectiveRemoteAvailable = latestAvailability?.remote_available ?? remoteAvailable;

    if (effectiveRemoteSelected && !effectiveRemoteAvailable) {
      toast.error('Selected remote unavailable. Reconnect or choose another source.');
      return;
    }

    if (!settings?.current_model && !effectiveRemoteAvailable) {
      toast.error('Select a speech model in Models before transcribing.');
      return;
    }

    if (!effectiveRemoteSelected && selectedModelAvailable === false) {
      const engine = settings?.current_model_engine || 'whisper';
      toast.error(
        isCloudEngine(engine)
          ? 'Connect your cloud provider before transcribing audio.'
          : 'Download the selected model before transcribing audio.'
      );
      return;
    }

    if (isProcessing) {
      toast.info("A transcription is already in progress");
      return;
    }

    const result = await start(
      settings?.current_model || '',
      effectiveRemoteSelected ? null : (settings?.current_model_engine || 'whisper'),
      await resolveHistoryModelName(latestActiveRemoteServer)
    );

    try {
      if (result?.outcome === 'error') {
        toast.error(result.message);
      } else if (result?.outcome === 'success') {
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

  const handleSaveAs = async () => {
    if (!resultText) return;
    const base = (selectedFile?.name || "transcript").replace(/\.[^/.]+$/, "");
    try {
      const path = await save({
        defaultPath: `${base}.txt`,
        filters: [
          { name: "Text", extensions: ["txt"] },
          { name: "Markdown", extensions: ["md"] },
        ],
      });
      if (!path) return; // cancelled
      const content = path.toLowerCase().endsWith(".md")
        ? `# ${base}\n\n${resultText}`
        : resultText;
      await invoke("save_transcript_file", { path, content });
      toast.success("Transcript saved");
    } catch (e) {
      toast.error(`Save failed: ${e}`);
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
    <div className="h-full min-h-0 flex flex-col">
      {/* Header */}
      <div className="shrink-0 px-6 py-4 border-b border-border/40">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h1 className="text-2xl font-semibold">Upload files</h1>
              <Dialog>
                <DialogTrigger asChild>
                  <Button type="button" variant="secondary" size="icon" aria-label="Upload guide" className="rounded-full">
                    <HelpCircle className="h-4.5 w-4.5" />
                  </Button>
                </DialogTrigger>
                <DialogContent className="sm:max-w-lg">
                  <DialogHeader>
                    <DialogTitle>Upload guide</DialogTitle>
                    <DialogDescription>
                      Upload uses your currently selected transcription source. Change it in Transcription if you want a different model or remote device first.
                    </DialogDescription>
                  </DialogHeader>
                  <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                    <p><strong className="text-foreground">Supported files</strong>: WAV, MP3, M4A, FLAC, OGG, MP4, and WebM.</p>
                    <p><strong className="text-foreground">Video files</strong>: audio is extracted first, then transcribed.</p>
                    <p><strong className="text-foreground">Long files</strong>: expect longer processing times and higher memory use.</p>
                  </div>
                </DialogContent>
              </Dialog>
            </div>
            <p className="mt-1 text-sm text-muted-foreground">
              Transcribe audio or video files using the same source you use for live recording.
            </p>
          </div>
          <Badge variant="secondary" className="max-w-[280px] truncate">
            Source: {activeSourceLabel}
          </Badge>
        </div>
      </div>

      <ScrollArea className="flex-1 min-h-0">
        <div className="p-6 space-y-6">
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

                      {speakerSegments.length > 0 && (
                        <div className="rounded-lg border border-border/50 bg-card/60 p-4">
                          <div className="mb-3 flex items-center justify-between gap-3">
                            <h3 className="text-sm font-medium">Speaker timeline</h3>
                            <Badge variant="secondary">
                              {new Set(speakerSegments.map((segment) => segment.speaker_id)).size} speakers
                            </Badge>
                          </div>
                          <ScrollArea className="max-h-48">
                            <div className="space-y-2 pr-2">
                              {speakerSegments.map((segment, index) => (
                                <div
                                  key={`${segment.speaker_id}-${segment.start_ms}-${segment.end_ms}-${index}`}
                                  className="flex items-center justify-between gap-3 rounded-md bg-accent/30 px-3 py-2 text-xs"
                                >
                                  <span className="font-medium text-foreground">
                                    {segment.speaker_id}
                                  </span>
                                  <span className="font-mono text-muted-foreground">
                                    {formatTimestamp(segment.start_ms)}–{formatTimestamp(segment.end_ms)}
                                  </span>
                                </div>
                              ))}
                            </div>
                          </ScrollArea>
                        </div>
                      )}

                      {diarizationError && (
                        <div className="rounded-lg border border-amber-200/50 bg-amber-500/10 p-3 text-xs text-amber-700">
                          Speaker timeline unavailable: {diarizationError}
                        </div>
                      )}

                      <div className="flex items-center justify-between gap-3">
                        <div className="flex items-center gap-2">
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
                          <Button onClick={handleSaveAs} variant="outline">
                            <Download className="h-4 w-4 mr-2" /> Save
                          </Button>
                        </div>

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
                <div className="p-1.5 rounded-md bg-muted">
                  <AlertCircle className="h-4 w-4 text-muted-foreground" />
                </div>
                <div className="space-y-2 flex-1">
                  <h3 className="font-medium text-sm">Important Information</h3>
                  <div className="text-sm text-muted-foreground space-y-1">
                    <p>• <strong>Supported Formats:</strong> WAV, MP3, M4A, FLAC, OGG, MP4, WebM</p>
                    <p>• <strong>Conversion:</strong> Non-WAV files will be converted to 16 kHz mono WAV before transcription; this may take time</p>
                    <p>• <strong>Video:</strong> Video files are supported; audio is extracted first</p>
                    <p>• <strong>Processing:</strong> Uses your selected local, cloud, or remote transcription source</p>
                    <p>• <strong>Duration:</strong> Longer media may take longer and use more memory</p>
                    <p className="font-medium text-foreground/80 mt-2">
                      Long media (4-5+ hours) may take several minutes and use significant memory.
                    </p>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </ScrollArea>
    </div>
  );
}
