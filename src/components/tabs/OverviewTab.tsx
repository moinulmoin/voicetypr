import { Button } from "@/components/ui/button";
import { ShareStatsModal } from "@/components/ShareStatsModal";
import { languages } from "@/components/LanguageSelection";
import { useCanAutoInsert, useReadiness } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { cn } from "@/lib/utils";
import { useTranscriptionHistory } from "@/hooks/useTranscriptionHistory";
import { useActiveTrigger } from "@/hooks/useActiveTrigger";
import { getModelDisplayName } from "@/lib/model-display";
import { invoke } from "@tauri-apps/api/core";
import { Share2, Wrench } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { ScreenId } from "@/components/navigation";
import { createLogger } from "@/lib/logger";

const log = createLogger("overview-tab");

interface SavedConnection {
  id: string;
  host: string;
  port: number;
  name: string | null;
}

export function OverviewTab({
  onNavigate,
}: {
  onNavigate?: (section: ScreenId) => void;
}) {
  const readiness = useReadiness();
  const canRecord = readiness.canRecord;
  const canAutoInsert = useCanAutoInsert();
  const { settings } = useSettings();
  const { label: triggerLabel } = useActiveTrigger(settings?.hotkey);
  const [shareModalOpen, setShareModalOpen] = useState(false);
  const [activeRemoteLabel, setActiveRemoteLabel] = useState<string | null>(
    null,
  );
  const selectedSourceLabel = readiness.remoteSelected
    ? (activeRemoteLabel ?? "Remote Voicetypr")
    : (getModelDisplayName(settings?.current_model) ?? "No source selected");

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

        const activeServer = servers.find(
          (server) => server.id === activeServerId,
        );
        setActiveRemoteLabel(
          activeServer?.name ||
            (activeServer
              ? `${activeServer.host}:${activeServer.port}`
              : "Remote Voicetypr"),
        );
      } catch (error) {
        log.error(
          "[OverviewTab] Failed to load active remote Voicetypr:",
          error,
        );
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

  const { history, totalCount, isLoading, loadError, refreshHistory } =
    useTranscriptionHistory({
      limit: 500,
      includeTotalCount: true,
    });

  const stats = useMemo(() => {
    const now = new Date();
    const startOfToday = new Date(now);
    startOfToday.setHours(0, 0, 0, 0);

    const startOfWeek = new Date(now);
    startOfWeek.setDate(startOfWeek.getDate() - 7);
    const todayCount = history.filter(
      (item) => new Date(item.timestamp) >= startOfToday,
    ).length;
    const weekCount = history.filter(
      (item) => new Date(item.timestamp) >= startOfWeek,
    ).length;


    const totalWords = history.reduce(
      (acc, item) => acc + item.text.split(/\s+/).filter(Boolean).length,
      0,
    );
    const avgLength =
      history.length > 0 ? Math.round(totalWords / history.length) : 0;

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
        label: dayStart
          .toLocaleDateString(undefined, { weekday: "short" })
          .slice(0, 3),
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

    return {
      todayCount,
      weekCount,
      totalWords,
      avgLength,
      timeSavedHours,
      timeSavedRemMinutes: timeSavedMinutes % 60,
      timeSavedMinutes,
      totalTranscriptions: totalCount,
      currentStreak,
      longestStreak,
      weekDays,
      weekMax,
    };
  }, [history, totalCount]);


  const spokenLanguage =
    languages.find(
      (language) => language.value === (settings?.speech_language ?? "en"),
    )?.label ?? settings?.speech_language ?? "English";

  return (
    <div className="h-full min-h-0 overflow-auto">
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-5 pb-4 pl-2 pr-4">
        <header className="flex items-start justify-between gap-4">
          <div>
            <h1 className="text-[1.75rem] font-semibold tracking-[-0.025em] text-foreground">
              Overview
            </h1>
            <p className="mt-1 text-sm text-muted-foreground">
              Your active dictation setup and usage at a glance.
            </p>
          </div>
          <Button
            type="button"
            variant="outline"
            className="shrink-0"
            onClick={() => setShareModalOpen(true)}
          >
            <Share2 />
            Share stats
          </Button>
        </header>

        <section className="rounded-xl bg-card p-5">
          <div className="flex items-start justify-between gap-4">
            <div>
              <h2 className="text-[15px] font-semibold text-foreground">
                Current setup
              </h2>
              <p className="mt-1 text-[13px] text-muted-foreground">
                Defaults used for your next desktop recording.
              </p>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              {!canRecord && onNavigate ? (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => onNavigate("advanced")}
                >
                  <Wrench />
                  Quick help
                </Button>
              ) : null}
              <span
                className={cn(
                  "rounded-full px-2.5 py-1 text-xs font-medium",
                  canRecord
                    ? "bg-sage-bg text-sage"
                    : "bg-amber-500/10 text-amber-700 dark:text-amber-300",
                )}
              >
                {canRecord ? "Ready" : "Needs attention"}
              </span>
            </div>
          </div>
          <dl className="mt-5 grid gap-x-8 gap-y-5 sm:grid-cols-2">
            <SetupValue label="Source" value={selectedSourceLabel} />
            <SetupValue
              label="Recording shortcut"
              value={triggerLabel === "Not set" ? "Not configured" : triggerLabel}
            />
            <SetupValue label="Spoken language" value={spokenLanguage} />
            <SetupValue
              label="After recording"
              value={canAutoInsert ? "Insert at the cursor" : "Keep in History"}
            />
          </dl>
        </section>

        <section
          aria-labelledby="weekly-rhythm-title"
          className="rounded-xl bg-card p-5"
        >
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h2
                id="weekly-rhythm-title"
                className="text-[15px] font-semibold text-foreground"
              >
                Last 7 days
              </h2>
              <p className="mt-1 text-[13px] text-muted-foreground">
                Completed transcripts by day.
              </p>
            </div>
            {stats.weekMax > 0 ? (
              <span className="rounded-full bg-sage-bg px-2.5 py-1 text-xs font-medium text-sage">
                Busiest{" "}
                {
                  stats.weekDays.find((day) => day.count === stats.weekMax)
                    ?.label
                }{" "}
                · {stats.weekMax}
              </span>
            ) : null}
          </div>
          {isLoading && history.length === 0 ? (
            <div aria-hidden className="mt-5 flex h-40 items-end gap-2">
              {[40, 70, 55].map((height) => (
                <div
                  key={height}
                  className="min-w-0 flex-1 bg-muted animate-pulse h-full rounded-md"
                  style={{ height: `${height}%` }}
                />
              ))}
            </div>
          ) : loadError && history.length === 0 ? (
            <p className="mt-5 text-[13px] text-muted-foreground">
              Couldn&apos;t load history{" "}
              <button
                type="button"
                onClick={() => {
                  void refreshHistory();
                }}
                className="text-[11px] text-sage hover:underline"
              >
                Retry
              </button>
            </p>
          ) : stats.weekCount === 0 ? (
            <p className="mt-5 text-[13px] text-muted-foreground">
              No transcripts this week — your daily activity will chart here.
            </p>
          ) : (
            <div className="mt-5 flex h-40 items-end gap-2">
              {stats.weekDays.map((day) => {
                const isPeak = day.count > 0 && day.count === stats.weekMax;
                const heightPct = Math.max(
                  6,
                  Math.round((day.count / stats.weekMax) * 100),
                );
                return (
                  <div
                    key={day.key}
                    className="flex h-full min-w-0 flex-1 flex-col justify-end gap-2"
                  >
                    <div
                      className={cn(
                        "w-full rounded-md transition-[height]",
                        isPeak ? "bg-sage" : "bg-sage/20",
                      )}
                      style={{ height: `${heightPct}%` }}
                      title={`${day.count} on ${day.label}`}
                    />
                    <span className="text-center text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                      {day.label}
                    </span>
                  </div>
                );
              })}
            </div>
          )}
          <p className="mt-4 text-[13px] tabular-nums text-muted-foreground">
            {stats.todayCount.toLocaleString()} today ·{" "}
            {stats.weekCount.toLocaleString()} last 7 days
            {stats.avgLength > 0 ? ` · ${stats.avgLength}/transcript` : ""}
          </p>
          <dl className="mt-5 grid grid-cols-2 gap-4 border-t border-border/70 pt-4 sm:grid-cols-4">
            <AllTimeValue
              label="Words spoken"
              value={stats.totalWords.toLocaleString()}
            />
            <AllTimeValue
              label="Transcripts"
              value={stats.totalTranscriptions.toLocaleString()}
            />
            <AllTimeValue
              label="Time saved"
              value={
                stats.timeSavedHours > 0
                  ? `${stats.timeSavedHours}h ${stats.timeSavedRemMinutes}m`
                  : `${stats.timeSavedMinutes}m`
              }
            />
            <AllTimeValue
              label="Day streak"
              value={stats.currentStreak.toLocaleString()}
            />
          </dl>
        </section>

        <ShareStatsModal
          open={shareModalOpen}
          onOpenChange={setShareModalOpen}
          stats={{
            totalTranscriptions: stats.totalTranscriptions,
            totalWords: stats.totalWords,
            timeSavedDisplay:
              stats.timeSavedHours > 0
                ? `${stats.timeSavedHours}h ${stats.timeSavedRemMinutes}m`
                : `${stats.timeSavedMinutes}m`,
          }}
        />
      </div>
    </div>
  );
}

function SetupValue({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 border-t border-border/70 pt-4 first:border-t-0 first:pt-0 sm:border-t-0 sm:pt-0">
      <dt className="text-xs font-medium text-muted-foreground">{label}</dt>
      <dd className="mt-1 truncate text-sm font-semibold text-foreground">
        {value}
      </dd>
    </div>
  );
}

function AllTimeValue({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0">
      <dt className="text-xs font-medium text-muted-foreground">{label}</dt>
      <dd className="mt-1 truncate text-lg font-semibold tabular-nums text-foreground">
        {value}
      </dd>
    </div>
  );
}
