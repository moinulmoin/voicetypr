import { ShareStatsModal } from "@/components/ShareStatsModal";
import { Button } from "@/components/ui/button";
import { useCanAutoInsert, useCanRecord } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { formatHotkey } from "@/lib/hotkey-utils";
import { cn } from "@/lib/utils";
import { TranscriptionHistory } from "@/types";
import { BarChart3, Clock, FileText, Flame, Share2, TrendingUp, Zap, Info } from "lucide-react";
import { useMemo, useState } from "react";

interface OverviewTabProps {
  history: TranscriptionHistory[];
}

export function OverviewTab({ history }: OverviewTabProps) {
  const canRecord = useCanRecord();
  const canAutoInsert = useCanAutoInsert();
  const { settings } = useSettings();
  const hotkey = settings?.hotkey || "Cmd+Shift+Space";
  const [selectedPeriod, setSelectedPeriod] = useState<"today" | "week" | "month" | "all">("all");
  const [shareModalOpen, setShareModalOpen] = useState(false);

  // Calculate stats
  const stats = useMemo(() => {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const thisWeek = new Date();
    thisWeek.setDate(thisWeek.getDate() - 7);
    const thisMonth = new Date();
    thisMonth.setDate(thisMonth.getDate() - 30);

    const todayCount = history.filter((item) => new Date(item.timestamp) >= today).length;

    const weekCount = history.filter((item) => new Date(item.timestamp) >= thisWeek).length;

    const monthCount = history.filter((item) => new Date(item.timestamp) >= thisMonth).length;

    const totalWords = history.reduce((acc, item) => acc + item.text.split(" ").length, 0);

    const avgLength = history.length > 0 ? Math.round(totalWords / history.length) : 0;

    // Calculate time saved (assuming 40 WPM typing speed)
    const avgTypingSpeed = 40; // words per minute
    const timeSavedMinutes = Math.round(totalWords / avgTypingSpeed);
    const timeSavedHours = Math.floor(timeSavedMinutes / 60);
    const timeSavedDisplay =
      timeSavedHours > 0 ? `${timeSavedHours}h ${timeSavedMinutes % 60}m` : `${timeSavedMinutes}m`;

    // Calculate current streak and longest streak
    let currentStreak = 0;
    let longestStreak = 0;

    if (history.length > 0) {
      // Create a set of unique days with activity (normalized to midnight)
      const activeDays = new Set<number>();
      history.forEach((item) => {
        const date = new Date(item.timestamp);
        date.setHours(0, 0, 0, 0);
        activeDays.add(date.getTime());
      });

      // Convert to sorted array of dates
      const sortedDays = Array.from(activeDays).sort((a, b) => b - a);

      if (sortedDays.length > 0) {
        // Check current streak (must include today or yesterday)
        const today = new Date();
        today.setHours(0, 0, 0, 0);
        const yesterday = new Date(today);
        yesterday.setDate(yesterday.getDate() - 1);

        const mostRecentDay = sortedDays[0];
        const isToday = mostRecentDay === today.getTime();
        const isYesterday = mostRecentDay === yesterday.getTime();

        // Only count current streak if last activity was today or yesterday
        if (isToday || isYesterday) {
          currentStreak = 1;

          // Count consecutive days backwards
          for (let i = 1; i < sortedDays.length; i++) {
            const expectedDate = new Date(sortedDays[i - 1]);
            expectedDate.setDate(expectedDate.getDate() - 1);

            if (sortedDays[i] === expectedDate.getTime()) {
              currentStreak++;
            } else {
              break; // Gap found, streak is broken
            }
          }
        }

        // Calculate longest streak ever
        let tempStreak = 1;
        longestStreak = 1;

        for (let i = 1; i < sortedDays.length; i++) {
          const expectedDate = new Date(sortedDays[i - 1]);
          expectedDate.setDate(expectedDate.getDate() - 1);

          if (sortedDays[i] === expectedDate.getTime()) {
            tempStreak++;
            longestStreak = Math.max(longestStreak, tempStreak);
          } else {
            tempStreak = 1; // Reset temp streak
          }
        }
      }
    }

    // Productivity score (0-100 based on usage)
    const productivityScore = Math.min(100, Math.round((weekCount / 7) * 20));

    return {
      todayCount,
      weekCount,
      monthCount,
      totalWords,
      avgLength,
      timeSavedDisplay,
      productivityScore,
      totalTranscriptions: history.length,
      currentStreak,
      longestStreak
    };
  }, [history]);

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="px-6 py-4 border-b border-border/40">
        <div className="flex items-center justify-between">
          <div>
            <div className="flex items-center gap-3">
              {stats.currentStreak > 0 ? (
                <div className="flex items-center gap-2">
                  <Flame
                    className={cn(
                      "h-6 w-6",
                      stats.currentStreak >= 7
                        ? "text-orange-500"
                        : stats.currentStreak >= 3
                        ? "text-orange-400"
                        : "text-orange-300"
                    )}
                  />
                  <h1 className="text-2xl font-semibold">{stats.currentStreak} day streak</h1>
                </div>
              ) : (
                <h1 className="text-2xl font-semibold">Start your streak today</h1>
              )}
            </div>
            <p className="text-sm text-muted-foreground mt-1">
              {new Date().toLocaleDateString("en-US", {
                weekday: "long",
                month: "long",
                day: "numeric"
              })}
              {stats.longestStreak > stats.currentStreak && (
                <span className="ml-2 text-xs">• Best streak: {stats.longestStreak} days</span>
              )}
            </p>
          </div>
          <div className="flex items-center gap-3">
            {
              <div className="flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm font-medium">
                {stats.todayCount > 0 && <TrendingUp className="h-3.5 w-3.5 text-green-500" />}
                {stats.todayCount} today
              </div>
            }
            {/* <span className="text-sm text-muted-foreground">
              {canRecord ? '✓ Ready' : '⚠️ Setup required'}
            </span> */}
            <Button size="sm" onClick={() => setShareModalOpen(true)} className="gap-2">
              <Share2 className="h-3.5 w-3.5" />
              Share
            </Button>
          </div>
        </div>
      </div>

      <div className="flex-1 overflow-hidden">
        <div className="h-full p-6">
          {/* Quick Stats */}
          <div className="grid grid-cols-4 gap-4 mb-6">
            <div
              className={cn(
                "p-4 rounded-lg bg-card border border-border/50 hover:border-border transition-all cursor-pointer",
                selectedPeriod === "all" && "bg-primary/5"
              )}
              onClick={() => setSelectedPeriod("all")}
              title="Click to filter all time"
            >
              <FileText className="h-5 w-5 text-blue-500 mb-3" />
              <p className="text-xs text-muted-foreground font-medium">Transcriptions</p>
              <p className="text-2xl font-bold mt-1">{stats.totalTranscriptions}</p>
              <p className="text-xs text-muted-foreground mt-1">{stats.todayCount} today</p>
            </div>

            <div
              className={cn(
                "p-4 rounded-lg bg-card border border-border/50 hover:border-border transition-all cursor-pointer",
                selectedPeriod === "month" && "bg-primary/5"
              )}
              onClick={() => setSelectedPeriod("month")}
              title="Click to filter last 30 days"
            >
              <BarChart3 className="h-5 w-5 text-purple-500 mb-3" />
              <p className="text-xs text-muted-foreground font-medium">Words Captured</p>
              <p className="text-2xl font-bold mt-1">{stats.totalWords.toLocaleString()}</p>
              <p className="text-xs text-muted-foreground mt-1">{stats.avgLength} avg</p>
            </div>

            <div
              className={cn(
                "p-4 rounded-lg bg-card border border-border/50 hover:border-border transition-all cursor-pointer",
                selectedPeriod === "today" && "bg-primary/5"
              )}
              onClick={() => setSelectedPeriod("today")}
              title="Based on 40 WPM typing speed"
            >
              <Clock className="h-5 w-5 text-green-500 mb-3" />
              <p className="text-xs text-muted-foreground font-medium">Time Saved</p>
              <p className="text-2xl font-bold mt-1">{stats.timeSavedDisplay}</p>
              <p className="text-xs text-muted-foreground mt-1">voice typing</p>
            </div>

            <div
              className={cn(
                "p-4 rounded-lg bg-card border border-border/50 hover:border-border transition-all cursor-pointer",
                selectedPeriod === "week" && "bg-primary/5"
              )}
              onClick={() => setSelectedPeriod("week")}
              title="Click to filter last 7 days"
            >
              <Zap className="h-5 w-5 text-yellow-500 mb-3" />
              <p className="text-xs text-muted-foreground font-medium">Productivity</p>
              <p className="text-2xl font-bold mt-1">{stats.productivityScore}%</p>
              <p className="text-xs text-muted-foreground mt-1">{stats.weekCount} this week</p>
            </div>
          </div>

          {/* Quick Tips Section */}
          {canRecord && (
            <div className="rounded-lg border border-border/50 bg-card overflow-hidden">
              <div className="p-4">
                <div className="flex items-start gap-3">
                  <div className="p-1.5 rounded-md bg-amber-500/10">
                    <Info className="h-4 w-4 text-amber-500" />
                  </div>
                  <div className="space-y-2 flex-1">
                    <h3 className="font-medium text-sm">Quick Tips</h3>
                    <ol className="text-sm text-muted-foreground space-y-1.5 list-decimal list-inside">
                      <li>Hold {formatHotkey(hotkey)} while speaking</li>
                      <li>Release the key to stop and transcribe</li>
                      <li>Text appears at your cursor automatically</li>
                      <li>Works in any text field across all apps</li>
                      <li>Double tap ESC to cancel recording</li>
                    </ol>
                    <p className="text-xs text-muted-foreground mt-3">
                      {canAutoInsert 
                        ? "Voice typing saves an average of 3x time compared to manual typing."
                        : "Note: Accessibility permission needed for automatic text insertion."}
                    </p>
                  </div>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Share Stats Modal */}
      <ShareStatsModal
        open={shareModalOpen}
        onOpenChange={setShareModalOpen}
        stats={{
          totalTranscriptions: stats.totalTranscriptions,
          todayCount: stats.todayCount,
          totalWords: stats.totalWords,
          avgLength: stats.avgLength,
          timeSavedDisplay: stats.timeSavedDisplay,
          productivityScore: stats.productivityScore,
          currentStreak: stats.currentStreak,
          longestStreak: stats.longestStreak
        }}
      />
    </div>
  );
}
