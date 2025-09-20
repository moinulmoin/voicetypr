import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { invoke } from "@tauri-apps/api/core";
import { Check, Copy, Download, Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";

interface ShareStatsModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  stats: {
    totalTranscriptions: number;
    todayCount: number;
    totalWords: number;
    avgLength: number;
    timeSavedDisplay: string;
    productivityScore: number;
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

    // Set canvas size - high resolution for social media sharing
    canvas.width = 2400;
    canvas.height = 1600;

    // Optimize for text quality
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = 'high';

    // Create clean gradient background
    const gradient = ctx.createLinearGradient(0, 0, canvas.width, canvas.height);
    gradient.addColorStop(0, "#0f0f0f");
    gradient.addColorStop(1, "#1a1a1a");

    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // Title at the top - scaled up
    ctx.font = "bold 96px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
    ctx.fillStyle = "#ffffff";
    ctx.textAlign = "center";
    ctx.fillText("My VoiceTypr Stats", canvas.width / 2, 200);

    // Best streak above the grid (if there's a longest streak)
    if (stats.longestStreak > 0) {
      ctx.font = "bold 72px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
      ctx.fillStyle = "#ffffff";
      ctx.textAlign = "center";
      ctx.fillText(`🔥 Best Streak: ${stats.longestStreak} Days`, canvas.width / 2, 360);
    }

    // 2x2 Grid of cards - scaled up
    const cardWidth = 760;
    const cardHeight = 360;
    const cardGap = 80;
    const gridStartX = (canvas.width - (cardWidth * 2 + cardGap)) / 2;
    const gridStartY = stats.longestStreak > 0 ? 480 : 360;

    // Card data - simplified
    const cards = [
      {
        label: "Transcriptions",
        value: stats.totalTranscriptions.toString(),
        subtitle: "total"
      },
      {
        label: "Words Captured",
        value: stats.totalWords.toLocaleString(),
        subtitle: `${stats.avgLength} avg`
      },
      {
        label: "Time Saved",
        value: stats.timeSavedDisplay,
        subtitle: "from typing"
      },
      {
        label: "Productivity",
        value: `${stats.productivityScore}%`,
        subtitle: "this week"
      }
    ];

    // Draw cards in 2x2 grid
    cards.forEach((card, index) => {
      const row = Math.floor(index / 2);
      const col = index % 2;
      const x = gridStartX + col * (cardWidth + cardGap);
      const y = gridStartY + row * (cardHeight + cardGap);

      // Simple card background
      ctx.fillStyle = "rgba(255, 255, 255, 0.05)";
      ctx.strokeStyle = "rgba(255, 255, 255, 0.1)";
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.roundRect(x, y, cardWidth, cardHeight, 32);
      ctx.fill();
      ctx.stroke();

      // Card label - scaled up
      ctx.font = "48px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
      ctx.fillStyle = "rgba(255, 255, 255, 0.5)";
      ctx.textAlign = "center";
      ctx.fillText(card.label, x + cardWidth/2, y + 90);

      // Card value - MUCH BIGGER
      ctx.font = "bold 112px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
      ctx.fillStyle = "#ffffff";
      ctx.fillText(card.value, x + cardWidth/2, y + 220);

      // Card subtitle - scaled up
      ctx.font = "40px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
      ctx.fillStyle = "rgba(255, 255, 255, 0.4)";
      ctx.fillText(card.subtitle, x + cardWidth/2, y + 290);
    });

    // // Current Streak at the bottom - scaled up
    // if (stats.currentStreak > 0) {
    //   ctx.font = "bold 80px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
    //   ctx.fillStyle = "#ffffff";
    //   ctx.textAlign = "center";
    //   const streakY = gridStartY + 2 * (cardHeight + cardGap) + 120;
    //   ctx.fillText(`🔥 ${stats.currentStreak} Day Streak`, canvas.width / 2, streakY);
    // }

    // Website at the bottom - scaled up
    ctx.font = "56px -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
    ctx.fillStyle = "rgba(255, 255, 255, 0.6)";
    ctx.textAlign = "center";
    ctx.fillText("voicetypr.com", canvas.width / 2, canvas.height - 80);

    // Convert canvas to data URL
    const dataUrl = canvas.toDataURL("image/png");
    setImageDataUrl(dataUrl);
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
      console.error("Failed to copy image to clipboard:", err);
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
      console.error("Failed to download image:", err);
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
      <DialogContent className="max-w-4xl">
        <DialogHeader>
          <DialogTitle>Share Your Stats</DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          {/* Canvas Preview */}
          <div className="relative rounded-lg overflow-hidden bg-black/5 border border-border/50 min-h-[300px]">
            {isLoading && (
              <div className="absolute inset-0 flex items-center justify-center bg-background/50 backdrop-blur-sm z-10">
                <div className="flex flex-col items-center gap-2">
                  <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
                  <span className="text-sm text-muted-foreground">Generating stats image...</span>
                </div>
              </div>
            )}
            <canvas
              ref={canvasRef}
              className="w-full h-auto"
              style={{ maxHeight: "400px", objectFit: "contain" }}
            />
          </div>

          {/* Actions - centered */}
          <div className="flex justify-center gap-2">
            <Button
              onClick={copyImageToClipboard}
              disabled={isCopying || !imageDataUrl}
              className={cn(
                "gap-2 min-w-[120px]",
                copied && "bg-green-600 hover:bg-green-600"
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