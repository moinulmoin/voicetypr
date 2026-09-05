import { PillActivityIndicator } from "@/components/PillActivityIndicator";
import type { PillState } from "@/components/pill/usePillController";
import type { PillIndicatorPosition, PillIndicatorStyle } from "@/types";
import type { PropsWithChildren } from "react";

interface PillShellProps extends PropsWithChildren {
  isActive: boolean;
  style: PillIndicatorStyle;
  position: PillIndicatorPosition;
}

interface PillStatusProps {
  audioLevel: number;
  state: PillState;
  elapsedSeconds: number;
  style: PillIndicatorStyle;
}

const PILL_LABELS: Record<PillState, string | null> = {
  idle: null,
  listening: "Listening",
  transcribing: "Transcribing...",
  formatting: "Polishing...",
};

function formatElapsed(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const remainder = seconds % 60;
  return `${minutes}:${remainder.toString().padStart(2, "0")}`;
}

export function PillShell({ children, isActive, position, style }: PillShellProps) {
  const horizontalAlignment = position.endsWith("-left")
    ? "justify-start"
    : position.endsWith("-right")
      ? "justify-end"
      : "justify-center";
  const verticalAlignment = position.startsWith("top-") ? "items-start" : "items-end";

  return (
    <div
      className={`pointer-events-none fixed inset-0 z-50 flex ${horizontalAlignment} ${verticalAlignment}`}
      data-pill-position={position}
    >
      <div
        className={`flex select-none items-center justify-center rounded-full border border-white/10 bg-[#14171c] text-neutral-100 shadow-[0_8px_24px_rgba(0,0,0,0.28)] transition-[padding] duration-150 ease-out ${
          isActive ? "px-3 py-1.5" : "px-2.5 py-1.5"
        }`}
        data-pill-style={style}
      >
        {children}
      </div>
    </div>
  );
}

export function PillStatus({ audioLevel, elapsedSeconds, state, style }: PillStatusProps) {
  const label = PILL_LABELS[state];
  const showLabel = style === "full" ? label : null;
  const showTimer = style === "full" && state === "listening";

  return (
    <div
      aria-live="polite"
      className="flex items-center justify-center gap-2"
      role={showLabel ? "status" : undefined}
    >
      <PillActivityIndicator audioLevel={audioLevel} state={state} />
      {showLabel ? (
        <span className="whitespace-nowrap text-[11px] font-medium leading-none text-neutral-300">
          {label}
        </span>
      ) : null}
      {showTimer ? (
        <span className="min-w-8 whitespace-nowrap text-right font-mono text-[10px] leading-none tabular-nums text-neutral-400">
          {formatElapsed(elapsedSeconds)}
        </span>
      ) : null}
    </div>
  );
}
