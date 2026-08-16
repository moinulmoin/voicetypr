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
import { save } from "@tauri-apps/plugin-dialog";
import { Check, Copy, Download, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
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

export function ShareStatsModal({
  open,
  onOpenChange,
  stats,
}: ShareStatsModalProps) {
  const [canvas, setCanvas] = useState<HTMLCanvasElement | null>(null);
  const [copied, setCopied] = useState(false);
  const [imageDataUrl, setImageDataUrl] = useState("");
  const [isLoading, setIsLoading] = useState(true);
  const [isCopying, setIsCopying] = useState(false);

  useEffect(() => {
    if (!open) {
      setIsLoading(true);
      setCopied(false);
      return;
    }

    setImageDataUrl("");
    setIsLoading(true);
    if (!canvas) return;

    const context = canvas.getContext("2d");
    if (!context) {
      log.error("Could not create the share card canvas");
      setIsLoading(false);
      return;
    }

    canvas.width = 2400;
    canvas.height = 1260;
    context.imageSmoothingEnabled = true;
    context.imageSmoothingQuality = "high";

    const fontFamily =
      "-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
    const forest = "#17352e";
    const paper = "#f5f2e9";
    const mint = "#98e7bd";
    const mutedMint = "#b8d9c7";

    context.fillStyle = forest;
    context.fillRect(0, 0, canvas.width, canvas.height);

    const glow = context.createRadialGradient(1880, 460, 40, 1880, 460, 880);
    glow.addColorStop(0, "rgba(93, 189, 145, 0.22)");
    glow.addColorStop(0.55, "rgba(74, 139, 110, 0.08)");
    glow.addColorStop(1, "rgba(23, 53, 46, 0)");
    context.fillStyle = glow;
    context.fillRect(0, 0, canvas.width, canvas.height);

    context.save();
    context.translate(2150, 90);
    context.rotate(Math.PI / 10);
    context.strokeStyle = "rgba(245, 242, 233, 0.06)";
    context.lineWidth = 70;
    context.beginPath();
    context.arc(0, 0, 430, 0, Math.PI * 2);
    context.stroke();
    context.restore();

    context.textAlign = "left";
    context.fillStyle = mint;
    context.beginPath();
    context.roundRect(120, 92, 58, 58, 18);
    context.fill();
    context.fillStyle = forest;
    [0, 1, 2].forEach((index) => {
      context.beginPath();
      context.arc(141 + index * 8, 121, 3.2, 0, Math.PI * 2);
      context.fill();
    });

    context.font = `650 34px ${fontFamily}`;
    context.fillStyle = paper;
    context.fillText("VOICETYPR", 202, 132);
    context.textAlign = "right";
    context.font = `600 24px ${fontFamily}`;
    context.fillStyle = "rgba(245, 242, 233, 0.52)";
    context.fillText("MY VOICE / IN NUMBERS", 2280, 130);

    context.textAlign = "left";
    context.font = `500 34px ${fontFamily}`;
    context.fillStyle = mutedMint;
    context.fillText("I turned my voice into", 120, 302);
    context.font = `720 196px ${fontFamily}`;
    context.fillStyle = paper;
    context.fillText(stats.totalWords.toLocaleString(), 110, 490);
    context.font = `720 80px ${fontFamily}`;
    context.fillStyle = mint;
    context.fillText("WORDS OUT LOUD.", 120, 592);

    const waveformHeights = [
      112, 190, 286, 164, 350, 494, 246, 408, 552, 318, 462, 220, 356, 176, 274,
      128,
    ];
    waveformHeights.forEach((height, index) => {
      const x = 1540 + index * 43;
      const y = 430 - height / 2;
      context.fillStyle = "rgba(152, 231, 189, 0.32)";
      context.beginPath();
      context.roundRect(x, y, 20, height, 10);
      context.fill();
    });

    const timeSaved =
      stats.timeSavedDisplay === "0m"
        ? "Just getting started"
        : stats.timeSavedDisplay;
    context.font = `400 31px ${fontFamily}`;
    context.fillStyle = "rgba(245, 242, 233, 0.64)";
    context.fillText(
      `${stats.totalTranscriptions.toLocaleString()} thoughts captured · ${timeSaved} not spent typing`,
      120,
      676,
    );

    context.strokeStyle = "rgba(245, 242, 233, 0.14)";
    context.lineWidth = 2;
    context.beginPath();
    context.moveTo(120, 785);
    context.lineTo(2280, 785);
    context.stroke();

    const streakValue =
      stats.currentStreak > 0
        ? `${stats.currentStreak} ${stats.currentStreak === 1 ? "day" : "days"}`
        : "Start today";
    const statItems = [
      {
        label: "TRANSCRIPTIONS",
        value: stats.totalTranscriptions.toLocaleString(),
        detail: `+${stats.todayCount.toLocaleString()} today`,
      },
      {
        label: "AVERAGE TAKE",
        value: `${stats.avgLength.toLocaleString()} words`,
        detail: "from thought to text",
      },
      {
        label: "CURRENT STREAK",
        value: streakValue,
        detail:
          stats.longestStreak > 0
            ? `${stats.longestStreak} day personal best`
            : "your rhythm starts here",
      },
    ];

    statItems.forEach((item, index) => {
      const x = 120 + index * 720;
      context.font = `650 22px ${fontFamily}`;
      context.fillStyle = "rgba(152, 231, 189, 0.7)";
      context.fillText(item.label, x, 876);
      context.font = `650 50px ${fontFamily}`;
      context.fillStyle = paper;
      context.fillText(item.value, x, 950);
      context.font = `400 25px ${fontFamily}`;
      context.fillStyle = "rgba(245, 242, 233, 0.48)";
      context.fillText(item.detail, x, 1004);
    });

    context.fillStyle = mint;
    context.beginPath();
    context.roundRect(120, 1102, 330, 48, 24);
    context.fill();
    context.font = `650 22px ${fontFamily}`;
    context.fillStyle = forest;
    context.fillText("SPOKEN, NOT TYPED", 143, 1135);
    context.textAlign = "right";
    context.font = `500 24px ${fontFamily}`;
    context.fillStyle = "rgba(245, 242, 233, 0.48)";
    context.fillText("voicetypr.com", 2280, 1135);

    try {
      setImageDataUrl(canvas.toDataURL("image/png"));
    } catch (error) {
      log.error("Could not encode the share card", error);
    } finally {
      setIsLoading(false);
    }
  }, [
    canvas,
    open,
    stats.avgLength,
    stats.currentStreak,
    stats.longestStreak,
    stats.timeSavedDisplay,
    stats.todayCount,
    stats.totalTranscriptions,
    stats.totalWords,
  ]);

  const copyImageToClipboard = async () => {
    if (!imageDataUrl || isCopying) return;

    setIsCopying(true);
    try {
      await invoke("copy_image_to_clipboard", { imageDataUrl });
      setCopied(true);
      toast.success("Stats image copied to clipboard");
      window.setTimeout(() => setCopied(false), 2000);
    } catch (error) {
      log.error("Failed to copy stats image:", error);
      toast.error("Could not copy the image. Try downloading it instead.");
    } finally {
      setIsCopying(false);
    }
  };

  const downloadImage = async () => {
    if (!imageDataUrl) return;

    try {
      const fileName = `voicetypr-stats-${Date.now()}.png`;
      const filePath = await save({
        defaultPath: fileName,
        filters: [{ name: "Image", extensions: ["png"] }],
      });
      if (filePath) {
        await invoke("save_image_to_file", { imageDataUrl, filePath });
      }
    } catch (error) {
      log.error("Failed to save stats image:", error);
      const link = document.createElement("a");
      link.download = `voicetypr-stats-${Date.now()}.png`;
      link.href = imageDataUrl;
      document.body.appendChild(link);
      link.click();
      link.remove();
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="overflow-hidden p-0"
        style={{ width: "calc(100% - 4rem)", maxWidth: "38rem" }}
      >
        <DialogHeader className="border-b border-border/70 px-5 py-4 text-left">
          <DialogTitle className="text-lg">Share your momentum</DialogTitle>
          <DialogDescription>
            Export a private progress card. Transcript text is never included.
          </DialogDescription>
        </DialogHeader>

        <div className="flex min-w-0 flex-col gap-3 p-4">
          <div
            className="relative w-full min-w-0 overflow-hidden rounded-xl bg-[#17352e] ring-1 ring-black/10"
            style={{ aspectRatio: "40 / 21" }}
          >
            {isLoading ? (
              <div className="absolute inset-0 z-10 flex items-center justify-center bg-[#17352e]/85 backdrop-blur-sm">
                <div className="flex flex-col items-center gap-2">
                  <Loader2 className="size-8 animate-spin text-[#98e7bd]" />
                  <span className="text-sm text-white/70">
                    Creating your share card…
                  </span>
                </div>
              </div>
            ) : null}
            {imageDataUrl ? (
              <img
                src={imageDataUrl}
                alt="Voicetypr voice statistics share card"
                className="block h-auto w-full max-w-full"
              />
            ) : null}
            <canvas
              ref={setCanvas}
              width={2400}
              height={1260}
              className={cn(
                "block h-auto w-full max-w-full",
                imageDataUrl && "hidden",
              )}
            />
          </div>

          <div className="flex justify-end gap-2">
            <Button
              onClick={copyImageToClipboard}
              disabled={isCopying || !imageDataUrl}
              className={cn(
                "min-w-32",
                copied && "bg-sage text-sage-foreground hover:bg-sage/90",
              )}
            >
              {isCopying ? (
                <Loader2 className="animate-spin" />
              ) : copied ? (
                <Check />
              ) : (
                <Copy />
              )}
              {isCopying ? "Copying…" : copied ? "Copied" : "Copy image"}
            </Button>
            <Button onClick={downloadImage} variant="outline">
              <Download />
              Download
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
