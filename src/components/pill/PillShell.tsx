import { AudioBars } from "@/components/AudioBars";
import type { PillState } from "@/components/pill/usePillController";
import type { PropsWithChildren } from "react";

interface PillShellProps extends PropsWithChildren {
  isActive: boolean;
}

interface PillStatusProps {
  audioLevel: number;
  state: PillState;
}

export function PillShell({ children, isActive }: PillShellProps) {
  return (
    <div className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center">
      <div
        className={`flex select-none items-center justify-center rounded-full border border-white/10 bg-[#14171c] text-neutral-100 transition-[padding] duration-150 ease-out ${isActive ? "px-2.5 py-1" : "px-2 py-1"}`}
      >
        {children}
      </div>
    </div>
  );
}

export function PillStatus({ audioLevel, state }: PillStatusProps) {
  return <AudioBars audioLevel={audioLevel} state={state} />;
}
