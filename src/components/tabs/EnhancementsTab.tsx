import { useEffect } from "react";
import { toast } from "sonner";
import { EnhancementsSection } from "../sections/EnhancementsSection";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";
import { createLogger } from "@/lib/logger";

const log = createLogger("enhancements-tab");

export function EnhancementsTab() {
  const { registerEvent } = useEventCoordinator("main");

  // Initialize enhancements tab
  useEffect(() => {
    const init = async () => {
      try {
        // Listen for AI enhancement errors
        registerEvent("ai-enhancement-auth-error", (payload) => {
          log.error("AI authentication error:", payload);
          toast.error(payload as string, {
            description: "Please update your API key in the Formatting section",
          });
        });

        registerEvent("ai-enhancement-error", (payload) => {
          log.warn("AI formatting error:", payload);
          toast.warning(payload as string);
        });
      } catch (error) {
        log.error("Failed to initialize enhancements tab:", error);
      }
    };

    init();
  }, [registerEvent]);

  return <EnhancementsSection view="ai" />;
}
