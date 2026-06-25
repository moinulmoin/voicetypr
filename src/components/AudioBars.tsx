import type { CSSProperties } from "react";

type BarState = "idle" | "listening" | "transcribing" | "formatting";

interface AudioBarsProps {
  state: BarState;
  audioLevel?: number;
}

// Sage green, matched to the brand accent and the reference waveform.
const SAGE = "oklch(0.78 0.1 155)";
// Brighter sage marks the AI-formatting phase so it reads as a distinct step.
const SAGE_BRIGHT = "oklch(0.86 0.12 155)";

const BAR_COUNT = 13;
const CENTER = (BAR_COUNT - 1) / 2;
// Parabolic taper: ~0.4 at the edges, 1.0 at the center — the waveform envelope.
const ENVELOPE = Array.from({ length: BAR_COUNT }, (_, index) => {
  const distance = Math.abs(index - CENTER) / CENTER;
  return 0.4 + (1 - distance * distance) * 0.6;
});

const BAR_WIDTH = 2;
const BAR_GAP = 1.5;
// Layout box each bar scales within. Bars sit small at rest and the wave grows
// toward the full box height as the voice gets louder, so the pill stays compact.
const BAR_HEIGHT = 16;
const REST_SCALE = 0.16;

type WaveStyle = CSSProperties & {
  "--wave-duration": string;
  "--wave-max": string;
  "--wave-min": string;
};

// Per-phase scaleY animation envelope. Each phase travels/breathes via the shared
// `pill-wave` keyframe; only listening reacts to the live audio level.
function getWaveStyle(state: BarState, envelope: number, level: number, index: number): WaveStyle {
  if (state === "listening") {
    // Traveling wave whose peak tracks the voice: tiny at rest, tall when loud.
    const peak = REST_SCALE + level * envelope * (1 - REST_SCALE);
    return {
      "--wave-duration": "650ms",
      "--wave-min": (peak * 0.45).toFixed(3),
      "--wave-max": peak.toFixed(3),
      animationDelay: `${index * -60}ms`,
    };
  }

  if (state === "formatting") {
    // AI-polish step: a tall wave sweeping left -> right in brighter sage.
    return {
      "--wave-duration": "1100ms",
      "--wave-min": "0.4",
      "--wave-max": (0.7 + envelope * 0.3).toFixed(2),
      animationDelay: `${index * 95}ms`,
    };
  }

  // transcribing: a shorter ripple sweeping left -> right.
  return {
    "--wave-duration": "900ms",
    "--wave-min": "0.25",
    "--wave-max": (0.45 + envelope * 0.3).toFixed(2),
    animationDelay: `${index * 70}ms`,
  };
}

export function AudioBars({ state, audioLevel = 0 }: AudioBarsProps) {
  const isIdle = state === "idle";
  const isFormatting = state === "formatting";
  const level = Math.max(0, Math.min(1, audioLevel));
  const color = isFormatting ? SAGE_BRIGHT : SAGE;

  return (
    <div
      data-testid="audio-bars"
      data-state={state}
      className="flex items-center justify-center"
      style={{ gap: BAR_GAP, height: BAR_HEIGHT }}
    >
      {ENVELOPE.map((envelope, index) => {
        const base: CSSProperties = {
          width: BAR_WIDTH,
          height: BAR_HEIGHT,
          backgroundColor: color,
          transformOrigin: "center",
        };

        if (isIdle) {
          // Quiet, still silhouette.
          return (
            <span
              key={index}
              className="rounded-full"
              style={{ ...base, transform: `scaleY(${(REST_SCALE + envelope * 0.12).toFixed(3)})`, opacity: 0.55 }}
            />
          );
        }

        return (
          <span
            key={index}
            className="rounded-full animate-pill-wave"
            style={{ ...base, ...getWaveStyle(state, envelope, level, index) }}
          />
        );
      })}
    </div>
  );
}
