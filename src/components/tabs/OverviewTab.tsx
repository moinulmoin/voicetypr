import { Button } from "@/components/ui/button";
import { useCanAutoInsert, useReadiness } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { cn } from "@/lib/utils";
import { useTranscriptionHistory } from "@/hooks/useTranscriptionHistory";
import { useActiveTrigger } from "@/hooks/useActiveTrigger";
import { isMacOS } from "@/lib/platform";
import { getModelDisplayName } from "@/lib/model-display";
import { invoke } from "@tauri-apps/api/core";
import {
  Clock3,
  FileText,
  Loader2,
  Share2,
  TrendingUp,
} from "lucide-react";
import { lazy, Suspense, useEffect, useMemo, useState, type ReactNode } from "react";
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
  const { label: triggerLabel } = useActiveTrigger(settings?.hotkey);
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

    // Per-day counts for the last 7 days (weekly rhythm sparkline).
    const weekDays = Array.from({ length: 7 }, (_, index) => {
      const dayStart = new Date(startOfToday);
      dayStart.setDate(dayStart.getDate() - (6 - index));
      const dayEnd = new Date(dayStart);
      dayEnd.setDate(dayEnd.getDate() + 1);
      const count = history.filter((item) => {
        const t = new Date(item.timestamp);
        return t >= dayStart && t < dayEnd;
      }).length;
      return {
        key: dayStart.getTime(),
        label: dayStart.toLocaleDateString(undefined, { weekday: "short" }).slice(0, 3),
        count,
      };
    });
    const weekMax = Math.max(1, ...weekDays.map((day) => day.count));

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
      timeSavedHours,
      timeSavedRemMinutes: timeSavedMinutes % 60,
      timeSavedMinutes,
      productivityScore,
      totalTranscriptions: totalCount,
      currentStreak,
      longestStreak,
      weekDays,
      weekMax,
    };
  }, [history, totalCount]);

  const todayLabel = useMemo(
    () => new Date().toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" }),
    [],
  );

  const hotkeyHint =
    triggerLabel === "Not set"
      ? "Set a recording trigger in Settings, then speak in any app — the text lands at your cursor."
      : `Press ${triggerLabel} in any app — speak, release, and the text lands at your cursor.`;

  return (
    <div className="h-full min-h-0 overflow-auto">
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-3.5 px-6 py-7 md:px-8">
        {/* ===== Head ===== */}
        <div className="mb-1 flex flex-wrap items-start gap-4">
          <div>
            <h1 className="text-2xl font-semibold tracking-tight text-foreground">Overview</h1>
            <p className="mt-0.5 text-sm text-muted-foreground">{todayLabel}</p>
          </div>
          <div className="ml-auto flex flex-wrap items-center gap-2.5">
            <StatusChip>
              <span
                className={cn(
                  "size-1.5 rounded-full",
                  canRecord ? "bg-sage" : "bg-amber-500",
                )}
              />
              {selectedSourceLabel}
            </StatusChip>
            <StatusChip>{canAutoInsert ? "Auto-insert on" : "Manual paste"}</StatusChip>
            <Button size="sm" onClick={() => setShareModalOpen(true)} className="gap-2">
              <Share2 className="h-4 w-4" />
              Share stats
            </Button>
          </div>
        </div>

        {/* ===== Ready hero ===== */}
        <div className="relative overflow-hidden rounded-3xl border border-border bg-gradient-to-br from-sage-bg/70 via-card to-card p-7 shadow-sm md:p-8">
          <div className="flex items-center gap-6">
            <div className="min-w-0">
              <h2 className="text-[2rem] font-semibold leading-[1.1] tracking-tight text-foreground md:text-[2.25rem]">
                {stats.currentStreak > 1
                  ? `${stats.currentStreak}-day dictation streak`
                  : "Ready for the next recording"}
              </h2>
              <p className="mt-2.5 max-w-md text-sm leading-relaxed text-muted-foreground">
                {canRecord ? hotkeyHint : setupMessage}
              </p>
            </div>
            <Waveform active={canRecord} className="ml-auto hidden shrink-0 sm:flex" />
          </div>
        </div>

        {/* ===== Stat cards ===== */}
        <div className="grid gap-3.5 sm:grid-cols-3">
          <StatCard
            icon={FileText}
            kicker="Transcriptions"
            value={stats.totalTranscriptions.toLocaleString()}
            foot={
              <>
                <b className="font-semibold text-sage">+{stats.todayCount} today</b> · all time
              </>
            }
          />
          <StatCard
            icon={TrendingUp}
            kicker="Words captured"
            value={stats.totalWords.toLocaleString()}
            foot={
              <>
                avg <b className="font-semibold text-foreground">{stats.avgLength} words</b> per take
              </>
            }
          />
          <StatCard
            icon={Clock3}
            kicker="Time saved"
            value={
              stats.timeSavedHours > 0 ? (
                <>
                  {stats.timeSavedHours}
                  <small className="text-[0.5em] text-muted-foreground">h </small>
                  {stats.timeSavedRemMinutes}
                  <small className="text-[0.5em] text-muted-foreground">m</small>
                </>
              ) : (
                <>
                  {stats.timeSavedMinutes}
                  <small className="text-[0.5em] text-muted-foreground">m</small>
                </>
              )
            }
            foot="vs. typing at 40 wpm"
          />
        </div>

        {/* ===== Glance row ===== */}
        <div className="grid gap-3.5 lg:grid-cols-[1.7fr_1fr]">
          <div className="rounded-2xl border border-border bg-card p-6 shadow-sm">
            <p className="text-sm font-semibold text-foreground">Weekly rhythm</p>
            <div className="mt-4 flex h-24 items-end gap-2">
              {stats.weekDays.map((day) => {
                const isHot = day.count > 0 && day.count === stats.weekMax;
                const heightPct = Math.max(6, Math.round((day.count / stats.weekMax) * 100));
                return (
                  <div
                    key={day.key}
                    className={cn(
                      "flex-1 rounded-md transition-colors",
                      isHot ? "bg-sage" : "bg-sage/25",
                    )}
                    style={{ height: `${heightPct}%` }}
                    title={`${day.count} on ${day.label}`}
                  />
                );
              })}
            </div>
            <div className="mt-2 flex gap-2">
              {stats.weekDays.map((day) => (
                <span
                  key={day.key}
                  className="flex-1 text-center text-[10px] font-medium uppercase tracking-wide text-muted-foreground"
                >
                  {day.label}
                </span>
              ))}
            </div>
          </div>

          <div className="rounded-2xl border border-border bg-card p-6 shadow-sm">
            <p className="text-sm font-semibold text-foreground">At a glance</p>
            <div className="mt-4 grid gap-2">
              <GlanceItem label="Today" value={`${stats.todayCount} transcriptions`} />
              <GlanceItem label="Last 7 days" value={`${stats.weekCount}`} />
              <GlanceItem
                label="Best streak"
                value={stats.longestStreak > 0 ? `${stats.longestStreak} days` : "—"}
              />
            </div>
          </div>
        </div>

        {/* ===== How it works ===== */}
        <div className="grid grid-cols-1 overflow-hidden rounded-2xl border border-border bg-card shadow-sm sm:grid-cols-3">
          <LoopStep
            n={1}
            title="Trigger"
            body={triggerLabel === "Not set" ? "Set a hotkey in Settings to start." : `Press ${triggerLabel} — or tap to toggle.`}
          />
          <LoopStep
            n={2}
            title="Speak"
            body="Talk naturally. Transcription runs on this device."
            divider
          />
          <LoopStep
            n={3}
            title="Release"
            body={canAutoInsert ? "Text lands at your cursor. Keep moving." : "Copy the transcript, or enable auto-insert."}
            divider
          />
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
              timeSavedDisplay:
                stats.timeSavedHours > 0
                  ? `${stats.timeSavedHours}h ${stats.timeSavedRemMinutes}m`
                  : `${stats.timeSavedMinutes}m`,
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

function StatusChip({ children }: { children: ReactNode }) {
  return (
    <span className="inline-flex items-center gap-2 whitespace-nowrap rounded-full border border-border bg-card px-3 py-1.5 text-xs font-medium text-muted-foreground">
      {children}
    </span>
  );
}

function StatCard({
  icon: Icon,
  kicker,
  value,
  foot,
}: {
  icon: typeof FileText;
  kicker: string;
  value: ReactNode;
  foot: ReactNode;
}) {
  return (
    <div className="rounded-2xl border border-border bg-card p-6 shadow-sm">
      <p className="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
        <Icon className="h-3.5 w-3.5 text-sage" />
        {kicker}
      </p>
      <p className="mt-3 text-[2.6rem] font-semibold leading-none tracking-tight text-foreground tabular-nums">
        {value}
      </p>
      <p className="mt-2.5 text-xs text-muted-foreground">{foot}</p>
    </div>
  );
}

function GlanceItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between rounded-xl bg-muted px-3.5 py-2.5">
      <span className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
        {label}
      </span>
      <span className="text-sm font-semibold text-foreground">{value}</span>
    </div>
  );
}

function LoopStep({
  n,
  title,
  body,
  divider,
}: {
  n: number;
  title: string;
  body: string;
  divider?: boolean;
}) {
  return (
    <div className={cn("flex items-start gap-3.5 p-5", divider && "border-t border-border sm:border-l sm:border-t-0")}>
      <span className="grid size-7 shrink-0 place-items-center rounded-full border border-sage/20 bg-sage-bg text-sm font-semibold text-sage">
        {n}
      </span>
      <div>
        <b className="block text-sm font-semibold text-foreground">{title}</b>
        <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">{body}</p>
      </div>
    </div>
  );
}

const WAVE_BARS = [14, 22, 34, 26, 44, 30, 52, 38, 48, 28, 40, 20, 30, 16];

function Waveform({ active, className }: { active: boolean; className?: string }) {
  return (
    <div className={cn("h-[54px] items-center gap-[3px]", className)} aria-hidden>
      {WAVE_BARS.map((h, i) => (
        <span
          key={i}
          className={cn(
            "w-[3.5px] rounded-full",
            active ? "bg-sage/70 animate-pill-wave" : "bg-sage/30",
          )}
          style={{
            height: `${h}px`,
            ...(active
              ? {
                  animationDelay: `${i * 70}ms`,
                  ["--wave-min" as string]: "0.4",
                  ["--wave-max" as string]: "1",
                }
              : {}),
          }}
        />
      ))}
    </div>
  );
}
