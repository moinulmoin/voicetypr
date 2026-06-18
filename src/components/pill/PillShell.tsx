import { AudioDots } from "@/components/AudioDots";
import type { PillState } from "@/components/pill/usePillController";
import type { PropsWithChildren } from "react";

interface PillShellProps extends PropsWithChildren {
  isActive: boolean;
  state: PillState;
}

interface PillStatusProps {
  audioLevel: number;
  state: PillState;
}

const STATE_TREATMENT: Record<PillState, string> = {
  idle: "ring-white/10 shadow-black/25",
  listening:
    "scale-[1.04] ring-sky-300/25 shadow-[0_0_28px_rgba(56,189,248,0.14),0_14px_38px_rgba(0,0,0,0.35)]",
  transcribing:
    "animate-pill-soft-pulse ring-white/10 shadow-black/35",
  formatting:
    "animate-pill-soft-pulse ring-white/10 shadow-black/35",
};

export function PillShell({ children, isActive, state }: PillShellProps) {
  return (
    <div className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center">
      <div
        className={`flex select-none items-center justify-center rounded-full border border-white/10 bg-neutral-900/85 text-neutral-100 ring-1 backdrop-blur-xl transition-[padding,transform,opacity] duration-200 ease-out ${isActive ? "px-3.5 py-[7px]" : "px-2.5 py-[5px]"} ${STATE_TREATMENT[state]}`}
      >
        {children}
      </div>
    </div>
  );
}

export function PillStatus({ audioLevel, state }: PillStatusProps) {
  return <AudioDots audioLevel={audioLevel} state={state} />;
}
