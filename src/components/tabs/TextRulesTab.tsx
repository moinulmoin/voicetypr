import { EnhancementsSection } from "../sections/EnhancementsSection";

/**
 * Pre-AI Formatting — the always-on, deterministic text rules (corrections,
 * Words & Names, shortcuts, voice commands) that run before any AI polish.
 * Reuses EnhancementsSection scoped to its "rules" zone.
 */
export function TextRulesTab() {
  return <EnhancementsSection view="rules" />;
}
