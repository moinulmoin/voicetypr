import type { CSSProperties } from "react";

type DotState = "idle" | "listening" | "transcribing" | "formatting";

interface AudioDotsProps {
  state: DotState;
  audioLevel?: number;
}

const DOTS = [
  { delayMs: -80, sensitivity: 0.76 },
  { delayMs: 0, sensitivity: 1 },
  { delayMs: -160, sensitivity: 0.76 },
] as const;

const IDLE_DOT_SIZE = 5;
const ACTIVE_DOT_SIZE = 7;
const DOT_GAP = 5;
const IDLE_CONTAINER_HEIGHT = 10;
const ACTIVE_CONTAINER_HEIGHT = 14;

type WaveStyle = CSSProperties & {
  "--wave-duration": string;
  "--wave-max": string;
  "--wave-min": string;
};

function clampLevel(level: number) {
  return Math.max(0, Math.min(1, level));
}

function getWaveStyle(level: number, sensitivity: number, delayMs: number): WaveStyle {
  const normalized = clampLevel(level);
  const maxScale = 1 + normalized * sensitivity * 1.45;
  const minScale = Math.max(0.72, 1 - normalized * sensitivity * 0.18);
  const durationMs = Math.round(820 - normalized * 420);

  return {
    "--wave-duration": `${durationMs}ms`,
    "--wave-max": maxScale.toFixed(2),
    "--wave-min": minScale.toFixed(2),
    animationDelay: `${delayMs}ms`,
  };
}

export function AudioDots({ state, audioLevel = 0 }: AudioDotsProps) {
  const isListening = state === "listening";
  const isActive = state !== "idle";
  const isPulsing = state === "transcribing" || state === "formatting";
  const dotSize = isActive ? ACTIVE_DOT_SIZE : IDLE_DOT_SIZE;
  const containerHeight = isActive ? ACTIVE_CONTAINER_HEIGHT : IDLE_CONTAINER_HEIGHT;

  return (
    <div
      className="flex items-center justify-center transition-[height] duration-200 ease-out"
      style={{ gap: DOT_GAP, height: containerHeight }}
    >
      {DOTS.map((dot, index) => (
        <span
          key={index}
          className={`origin-center rounded-full bg-current opacity-90 transition-[height,width,opacity,transform] duration-200 ease-out ${
            isListening ? "animate-pill-wave" : ""
          } ${isPulsing ? "animate-pill-soft-pulse" : ""}`}
          style={{
            height: dotSize,
            width: dotSize,
            ...(isListening
              ? getWaveStyle(audioLevel, dot.sensitivity, dot.delayMs)
              : undefined),
          }}
        />
      ))}
    </div>
  );
}
