import { PillShell, PillStatus } from "@/components/pill/PillShell";
import { usePillController } from "@/components/pill/usePillController";

export function RecordingPill() {
  const { audioLevel, isActive, isVisible, pillState } = usePillController();

  if (!isVisible) {
    return null;
  }

  return (
    <PillShell isActive={isActive}>
      <PillStatus audioLevel={audioLevel} state={pillState} />
    </PillShell>
  );
}
