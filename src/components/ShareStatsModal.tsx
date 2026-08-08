import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { invoke } from "@tauri-apps/api/core";
import { Check, Copy, Download, Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { createLogger } from "@/lib/logger";

const log = createLogger("share-stats");

interface ShareStatsModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  stats: {
    totalTranscriptions: number;
    todayCount: number;
    totalWords: number;
    avgLength: number;
    timeSavedDisplay: string;
    currentStreak: number;
    longestStreak: number;
  };
}

export function ShareStatsModal({ open, onOpenChange, stats }: ShareStatsModalProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [copied, setCopied] = useState(false);
  const [imageDataUrl, setImageDataUrl] = useState<string>("");
  const [isLoading, setIsLoading] = useState(true);
  const [isCopying, setIsCopying] = useState(false);

  useEffect(() => {
    if (open) {
      setIsLoading(true);
      // Use requestAnimationFrame to ensure DOM is ready
      requestAnimationFrame(() => {
        if (canvasRef.current) {
          drawStatsCard();
          setIsLoading(false);
        }
      });
    } else {
      // Reset states when modal closes
      setIsLoading(true);
      setCopied(false);
    }
  }, [open, stats]);

  const drawStatsCard = () => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    canvas.width = 2400;
    canvas.height = 1600;
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";

    const background = ctx.createLinearGradient(0, 0, canvas.width, canvas.height);
    background.addColorStop(0, "#f8f8f3");
    background.addColorStop(1, "#e9efe6");
    ctx.fillStyle = background;
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    ctx.fillStyle = "rgba(101, 122, 98, 0.08)";
    ctx.beginPath();
    ctx.arc(2210, 100, 420, 0, Math.PI * 2);
    ctx.fill();
    ctx.beginPath();
    ctx.arc(80, 1540, 300, 0, Math.PI * 2);
    ctx.fill();

    const fontFamily = "-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
    ctx.textAlign = "left";

    ctx.fillStyle = "#657a62";
    ctx.beginPath();
    ctx.roundRect(150, 120, 58, 58, 16);
    ctx.fill();
    ctx.fillStyle = "#f8f8f3";
    [0, 1, 2].forEach((index) => {
      ctx.beginPath();
      ctx.arc(171 + index * 8, 149, 3, 0, Math.PI * 2);
      ctx.fill();
    });

    ctx.font = `600 36px ${fontFamily}`;
    ctx.fillStyle = "#52604f";
    ctx.fillText("VOICETYPR", 232, 161);

    ctx.font = `600 88px ${fontFamily}`;
    ctx.fillStyle = "#20251f";
    ctx.fillText("Your voice, in numbers", 150, 342);

    ctx.font = `36px ${fontFamily}`;
    ctx.fillStyle = "#6b7468";
    ctx.fillText("A snapshot of your dictation momentum", 150, 412);

    const cards = [
      {
        label: "TRANSCRIPTIONS",
        value: stats.totalTranscriptions.toLocaleString(),
        detail: `+${stats.todayCount.toLocaleString()} today`,
      },
      {
        label: "WORDS CAPTURED",
        value: stats.totalWords.toLocaleString(),
        detail: `${stats.avgLength.toLocaleString()} words per take`,
      },
      {
        label: "TIME SAVED",
        value: stats.timeSavedDisplay,
        detail: "vs. typing at 40 wpm",
      },
    ];

    const cardWidth = 660;
    const cardHeight = 430;
    const cardGap = 60;
    const cardY = 520;

    cards.forEach((card, index) => {
      const x = 150 + index * (cardWidth + cardGap);
      ctx.fillStyle = "rgba(255, 255, 255, 0.88)";
      ctx.strokeStyle = "rgba(82, 96, 79, 0.14)";
      ctx.lineWidth = 3;
      ctx.beginPath();
      ctx.roundRect(x, cardY, cardWidth, cardHeight, 36);
      ctx.fill();
      ctx.stroke();

      ctx.font = `600 28px ${fontFamily}`;
      ctx.fillStyle = "#657a62";
      ctx.fillText(card.label, x + 52, cardY + 88);

      ctx.font = `600 92px ${fontFamily}`;
      ctx.fillStyle = "#20251f";
      ctx.fillText(card.value, x + 52, cardY + 230);

      ctx.font = `32px ${fontFamily}`;
      ctx.fillStyle = "#6b7468";
      ctx.fillText(card.detail, x + 52, cardY + 330);
    });

    ctx.fillStyle = "#52674f";
    ctx.beginPath();
    ctx.roundRect(150, 1050, 2100, 330, 44);
    ctx.fill();

    ctx.font = `600 60px ${fontFamily}`;
    ctx.fillStyle = "#f7f8f4";
    ctx.fillText(
      stats.currentStreak > 1
        ? `${stats.currentStreak}-day dictation streak`
        : "Built one thought at a time",
      220,
      1170,
    );

    ctx.font = `34px ${fontFamily}`;
    ctx.fillStyle = "rgba(247, 248, 244, 0.74)";
    const longestStreak =
      stats.longestStreak > 0 ? `${stats.longestStreak} day best streak` : "Start your streak today";
    ctx.fillText(
      `${stats.totalTranscriptions.toLocaleString()} transcriptions  ·  ${longestStreak}`,
      220,
      1252,
    );

    ctx.textAlign = "right";
    ctx.font = `500 34px ${fontFamily}`;
    ctx.fillStyle = "rgba(247, 248, 244, 0.74)";
    ctx.fillText("voicetypr.com", 2180, 1328);

    setImageDataUrl(canvas.toDataURL("image/png"));
  };

  const copyImageToClipboard = async () => {
    if (!canvasRef.current || !imageDataUrl || isCopying) return;

    setIsCopying(true);
    
    try {
      // Use Tauri's clipboard API for system-level copy
      await invoke("copy_image_to_clipboard", {
        imageDataUrl: imageDataUrl
      });

      setCopied(true);
      toast.success("Stats image copied to clipboard!");
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      log.error("Failed to copy image to clipboard:", err);
      toast.error("Failed to copy image. Try the download button instead.");
    } finally {
      setIsCopying(false);
    }
  };

  const downloadImage = async () => {
    if (!imageDataUrl) return;

    try {
      const fileName = `voicetypr-stats-${Date.now()}.png`;

      // Use Tauri's save dialog to let user choose location
      const { save } = await import('@tauri-apps/plugin-dialog');
      const filePath = await save({
        defaultPath: fileName,
        filters: [{
          name: 'Image',
          extensions: ['png']
        }]
      });

      if (filePath) {
        // Use the Rust backend to save the file (best practice)
        await invoke("save_image_to_file", {
          imageDataUrl: imageDataUrl,
          filePath: filePath
        });
      }
    } catch (err) {
      log.error("Failed to download image:", err);
      // Fallback to browser download
      const link = document.createElement("a");
      link.download = `voicetypr-stats-${Date.now()}.png`;
      link.href = imageDataUrl;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle>Share your stats</DialogTitle>
          <DialogDescription>
            Copy or save a private snapshot of your dictation progress.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {/* Canvas Preview */}
          <div className="relative min-h-[300px] overflow-hidden rounded-2xl border border-border bg-muted/30">
            {isLoading && (
              <div className="absolute inset-0 z-10 flex items-center justify-center bg-background/70 backdrop-blur-sm">
                <div className="flex flex-col items-center gap-2">
                  <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
                  <span className="text-sm text-muted-foreground">Generating stats image...</span>
                </div>
              </div>
            )}
            <canvas
              ref={canvasRef}
              className="h-auto w-full"
              style={{ maxHeight: "420px", objectFit: "contain" }}
            />
          </div>

          <div className="flex justify-end gap-2">
            <Button
              onClick={copyImageToClipboard}
              disabled={isCopying || !imageDataUrl}
              className={cn(
                "min-w-32 gap-2",
                copied && "bg-sage text-sage-foreground hover:bg-sage/90",
              )}
            >
              {isCopying ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Copying...
                </>
              ) : copied ? (
                <>
                  <Check className="h-4 w-4" />
                  Copied!
                </>
              ) : (
                <>
                  <Copy className="h-4 w-4" />
                  Copy Image
                </>
              )}
            </Button>
            <Button
              onClick={downloadImage}
              variant="outline"
              className="gap-2"
            >
              <Download className="h-4 w-4" />
              Download
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}