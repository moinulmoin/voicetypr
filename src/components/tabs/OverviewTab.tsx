import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { useCanAutoInsert, useReadiness } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { cn } from "@/lib/utils";
import { useTranscriptionHistory } from "@/hooks/useTranscriptionHistory";
import { isMacOS } from "@/lib/platform";
import { getModelDisplayName } from "@/lib/model-display";
import { invoke } from "@tauri-apps/api/core";
import {
  BarChart3,
  CheckCircle2,
  Clock3,
  FileText,
  Flame,
  Loader2,
  Mic,
  Share2,
  Sparkles,
  TrendingUp,
  Zap,
} from "lucide-react";
import { lazy, Suspense, useEffect, useMemo, useState } from "react";
import { createLogger } from "@/lib/logger";

const log = createLogger("overview-tab");

const ShareStatsModal = lazy(() =>
  import("@/components/ShareStatsModal").then((module) => ({
    default: module.ShareStatsModal,
  })),
);

interface SavedConnection {
  id: string;
  host: string;
  port: number;
  name: string | null;
}

export function OverviewTab() {
  const readiness = useReadiness();
  const canRecord = readiness.canRecord;
  const canAutoInsert = useCanAutoInsert();
  const { settings } = useSettings();
  const hotkey = settings?.hotkey || "Cmd+Shift+Space";
  const [shareModalOpen, setShareModalOpen] = useState(false);
  const [activeRemoteLabel, setActiveRemoteLabel] = useState<string | null>(null);
  const selectedSourceLabel = readiness.remoteSelected
    ? activeRemoteLabel ?? "Remote Voicetypr"
    : getModelDisplayName(settings?.current_model) ?? "No source selected";
  const setupMessage =
    readiness.licenseStatus === "expired" || readiness.licenseStatus === "none"
      ? "Activate a license to keep recording with Voicetypr."
      : readiness.hasModels === false || readiness.selectedModelAvailable === false
        ? "Choose a ready local model, cloud provider, or remote Voicetypr source before recording."
        : isMacOS && readiness.hasMicrophonePermission === false
          ? "Allow microphone access in macOS Settings before recording."
          : "Finish setup before recording will work cleanly.";

  useEffect(() => {
    if (!readiness.remoteSelected) {
      return;
    }

    let cancelled = false;

    const loadActiveRemoteLabel = async () => {
      try {
        const [activeServerId, servers] = await Promise.all([
          invoke<string | null>("get_active_remote_server"),
          invoke<SavedConnection[]>("list_remote_servers"),
        ]);
        if (cancelled) return;

        const activeServer = servers.find((server) => server.id === activeServerId);
        setActiveRemoteLabel(
          activeServer?.name || (activeServer ? `${activeServer.host}:${activeServer.port}` : "Remote Voicetypr"),
        );
      } catch (error) {
        log.error("[OverviewTab] Failed to load active remote Voicetypr:", error);
        if (!cancelled) {
          setActiveRemoteLabel("Remote Voicetypr");
        }
      }
    };

    void loadActiveRemoteLabel();

    return () => {
      cancelled = true;
    };
  }, [readiness.remoteSelected]);
  const { history, totalCount } = useTranscriptionHistory({
    limit: 500,
    includeTotalCount: true,
  });

  const stats = useMemo(() => {
    const now = new Date();
    const startOfToday = new Date(now);
    startOfToday.setHours(0, 0, 0, 0);

    const startOfWeek = new Date(now);
    startOfWeek.setDate(startOfWeek.getDate() - 7);

    const startOfMonth = new Date(now);
    startOfMonth.setDate(startOfMonth.getDate() - 30);

    const todayCount = history.filter(
      (item) => new Date(item.timestamp) >= startOfToday,
    ).length;
    const weekCount = history.filter(
      (item) => new Date(item.timestamp) >= startOfWeek,
    ).length;
    const monthCount = history.filter(
      (item) => new Date(item.timestamp) >= startOfMonth,
    ).length;

    const totalWords = history.reduce(
      (acc, item) => acc + item.text.split(/\s+/).filter(Boolean).length,
      0,
    );
    const avgLength = history.length > 0 ? Math.round(totalWords / history.length) : 0;

    const avgTypingSpeed = 40;
    const timeSavedMinutes = Math.round(totalWords / avgTypingSpeed);
    const timeSavedHours = Math.floor(timeSavedMinutes / 60);
    const timeSavedDisplay =
      timeSavedHours > 0
        ? `${timeSavedHours}h ${timeSavedMinutes % 60}m`
        : `${timeSavedMinutes}m`;

    let currentStreak = 0;
    let longestStreak = 0;

    if (history.length > 0) {
      const activeDays = new Set<number>();
      history.forEach((item) => {
        const date = new Date(item.timestamp);
        date.setHours(0, 0, 0, 0);
        activeDays.add(date.getTime());
      });

      const sortedDays = Array.from(activeDays).sort((a, b) => b - a);

      if (sortedDays.length > 0) {
        const today = new Date();
        today.setHours(0, 0, 0, 0);
        const yesterday = new Date(today);
        yesterday.setDate(yesterday.getDate() - 1);

        const mostRecentDay = sortedDays[0];
        if (
          mostRecentDay === today.getTime() ||
          mostRecentDay === yesterday.getTime()
        ) {
          currentStreak = 1;
          for (let index = 1; index < sortedDays.length; index += 1) {
            const expectedDate = new Date(sortedDays[index - 1]);
            expectedDate.setDate(expectedDate.getDate() - 1);
            if (sortedDays[index] === expectedDate.getTime()) {
              currentStreak += 1;
            } else {
              break;
            }
          }
        }

        let tempStreak = 1;
        longestStreak = 1;
        for (let index = 1; index < sortedDays.length; index += 1) {
          const expectedDate = new Date(sortedDays[index - 1]);
          expectedDate.setDate(expectedDate.getDate() - 1);
          if (sortedDays[index] === expectedDate.getTime()) {
            tempStreak += 1;
            longestStreak = Math.max(longestStreak, tempStreak);
          } else {
            tempStreak = 1;
          }
        }
      }
    }

    const productivityScore = Math.min(100, Math.round((weekCount / 7) * 20));

    return {
      todayCount,
      weekCount,
      monthCount,
      totalWords,
      avgLength,
      timeSavedDisplay,
      productivityScore,
      totalTranscriptions: totalCount,
      currentStreak,
      longestStreak,
    };
  }, [history, totalCount]);

  const kpis = [
    {
      label: "Transcriptions",
      value: stats.totalTranscriptions.toLocaleString(),
      caption: `${stats.todayCount} today`,
      icon: FileText,
    },
    {
      label: "Words captured",
      value: stats.totalWords.toLocaleString(),
      caption: `${stats.avgLength} average words`,
      icon: BarChart3,
    },
    {
      label: "Time saved",
      value: stats.timeSavedDisplay,
      caption: "vs. manual typing",
      icon: Clock3,
    },
    {
      label: "Weekly rhythm",
      value: `${stats.productivityScore}%`,
      caption: `${stats.weekCount} in the last 7 days`,
      icon: Zap,
    },
  ] as const;

  return (
    <div className="h-full min-h-0 overflow-auto">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-6">
        <Card className="overflow-hidden border-border/70 bg-card/95 shadow-sm">
          <CardHeader className="gap-4">
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div className="space-y-3">
                <div className="space-y-2">
                  <CardTitle className="text-3xl tracking-[-0.04em] sm:text-4xl">
                    {stats.currentStreak > 0
                      ? `${stats.currentStreak}-day dictation streak`
                      : "Ready for the next recording"}
                  </CardTitle>
                  <CardDescription className="max-w-2xl text-sm leading-6">
                    {canRecord
                      ? "Voice notes in, clean text out. Check readiness, recent output, and what to do next."
                      : setupMessage}
                  </CardDescription>
                </div>
              </div>
              <Button size="sm" onClick={() => setShareModalOpen(true)} className="gap-2 self-start">
                <Share2 className="h-4 w-4" />
                Share stats
              </Button>
            </div>
          </CardHeader>
          <CardContent className="grid gap-4 lg:grid-cols-[1.2fr_0.8fr]">
            <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
              {kpis.map((item) => {
                const Icon = item.icon;
                return (
                  <Card key={item.label} size="sm" className="border-border/70 bg-background/75 shadow-sm">
                    <CardHeader className="gap-3">
                      <div className="flex size-10 items-center justify-center rounded-xl bg-muted text-primary">
                        <Icon className="h-5 w-5" />
                      </div>
                      <div>
                        <CardDescription className="text-[11px] uppercase tracking-[0.16em]">
                          {item.label}
                        </CardDescription>
                        <CardTitle className="mt-2 text-2xl tracking-[-0.03em]">
                          {item.value}
                        </CardTitle>
                      </div>
                    </CardHeader>
                    <CardContent>
                      <p className="text-xs text-muted-foreground">{item.caption}</p>
                    </CardContent>
                  </Card>
                );
              })}
            </div>

            <Card size="sm" className="border-border/70 bg-background/75 shadow-sm">
              <CardHeader>
                <CardTitle className="flex items-center gap-2 text-lg">
                  <TrendingUp className="h-5 w-5 text-primary" />
                  At a glance
                </CardTitle>
                <CardDescription>
                  A simple read on current usage without pretending to be more precise than the actual data.
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-3 text-sm">
                <MetricRow label="Today" value={`${stats.todayCount} transcriptions`} />
                <MetricRow label="Last 7 days" value={`${stats.weekCount} transcriptions`} />
                <MetricRow label="Last 30 days" value={`${stats.monthCount} transcriptions`} />
                <MetricRow
                  label="Best streak"
                  value={
                    stats.longestStreak > 0 ? `${stats.longestStreak} days` : "No streak yet"
                  }
                />
              </CardContent>
            </Card>
          </CardContent>
        </Card>

        <div className="grid gap-6 xl:grid-cols-[1.15fr_0.85fr]">
          <Card className="border-border/70 bg-card/90 shadow-sm">
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-xl">
                <Sparkles className="h-5 w-5 text-primary" />
                How it works
              </CardTitle>
              <CardDescription>
                Keep the loop obvious: trigger, speak, release, keep moving.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <WorkflowStep
                title="Start recording"
                body={`Press ${hotkey} anywhere.`}
                ready={canRecord}
              />
              <WorkflowStep
                title="Speak naturally"
                body="Voicetypr transcribes first, then applies deterministic cleanup and optional AI formatting."
                ready={canRecord}
              />
              <WorkflowStep
                title="Keep the transcript flowing"
                body={
                  canAutoInsert
                    ? "Transcribed text can auto-insert at your cursor as soon as processing finishes."
                    : "Grant Accessibility to enable auto-insert across apps. Manual copy still works without it."
                }
                ready={canAutoInsert}
              />
            </CardContent>
          </Card>

          <Card className="border-border/70 bg-card/90 shadow-sm">
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-xl">
                <Flame className="h-5 w-5 text-primary" />
                Ready right now
              </CardTitle>
              <CardDescription>
                The app should tell you plainly whether the next recording will work.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <StatusRow
                label="Recording"
                value={canRecord ? "Ready to record" : "Needs setup"}
                tone={canRecord ? "ready" : "warning"}
              />
              <StatusRow
                label="Auto-insert"
                value={canAutoInsert ? "Enabled" : "Accessibility permission missing"}
                tone={canAutoInsert ? "ready" : "warning"}
              />
              <StatusRow
                label="Transcription source"
                value={selectedSourceLabel}
                tone={readiness.remoteSelected || settings?.current_model ? "neutral" : "warning"}
              />
            </CardContent>
          </Card>
        </div>

        <Suspense
          fallback={
            <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 backdrop-blur-sm">
              <div className="flex items-center gap-2 rounded-2xl border border-border bg-background px-4 py-3 shadow-lg">
                <Loader2 className="h-5 w-5 animate-spin" />
                <span className="text-sm">Loading share modal…</span>
              </div>
            </div>
          }
        >
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
              longestStreak: stats.longestStreak,
            }}
          />
        </Suspense>
      </div>
    </div>
  );
}

function MetricRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-xl bg-muted/45 px-3 py-2">
      <span className="text-xs uppercase tracking-[0.16em] text-muted-foreground">
        {label}
      </span>
      <span className="text-sm font-medium text-foreground">{value}</span>
    </div>
  );
}

function WorkflowStep({
  title,
  body,
  ready,
}: {
  title: string;
  body: string;
  ready: boolean;
}) {
  return (
    <div className="flex gap-3 rounded-2xl border border-border/60 bg-background/75 p-4 shadow-xs">
      <div
        className={cn(
          "mt-0.5 flex size-8 items-center justify-center rounded-lg",
          ready ? "bg-primary text-primary-foreground" : "bg-muted text-muted-foreground",
        )}
      >
        {ready ? <CheckCircle2 className="h-4 w-4" /> : <Mic className="h-4 w-4" />}
      </div>
      <div className="space-y-1">
        <p className="text-sm font-medium">{title}</p>
        <p className="text-sm leading-6 text-muted-foreground">{body}</p>
      </div>
    </div>
  );
}

function StatusRow({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: "ready" | "warning" | "neutral";
}) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-xl bg-muted/45 px-3 py-2">
      <span className="text-sm text-muted-foreground">{label}</span>
      <Badge
        variant="secondary"
        className={cn(
          tone === "ready" && "text-primary",
          tone === "warning" && "text-amber-700 dark:text-amber-400",
        )}
      >
        {value}
      </Badge>
    </div>
  );
}
