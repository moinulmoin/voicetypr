import { PillShell, PillStatus } from "@/components/pill/PillShell";
import { usePillController } from "@/components/pill/usePillController";

export function RecordingPill() {
  const {
    audioLevel,
    elapsedSeconds,
    isActive,
    isVisible,
    pillPosition,
    pillState,
    pillStyle,
  } = usePillController();

  if (!isVisible) {
    return null;
  }

  return (
    <PillShell isActive={isActive} position={pillPosition} style={pillStyle}>
      <PillStatus
        audioLevel={audioLevel}
        elapsedSeconds={elapsedSeconds}
        state={pillState}
        style={pillStyle}
      />
    </PillShell>
  );
}
