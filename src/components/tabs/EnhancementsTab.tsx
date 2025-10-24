import { useEffect } from "react";
import { toast } from "sonner";
import { EnhancementsSection } from "../sections/EnhancementsSection";
import { useEventCoordinator } from "@/hooks/useEventCoordinator";

export function EnhancementsTab() {
  const { registerEvent } = useEventCoordinator("main");

  // Initialize enhancements tab
  useEffect(() => {
    const init = async () => {
      try {
        // Listen for AI enhancement errors
        registerEvent("ai-enhancement-auth-error", (event) => {
          console.error("AI authentication error:", event.payload);
          toast.error(event.payload as string, {
            description: "Please update your API key in the Formatting section",
          });
        });

        registerEvent("ai-enhancement-error", (event) => {
          console.warn("AI formatting error:", event.payload);
          toast.warning(event.payload as string);
        });
      } catch (error) {
        console.error("Failed to initialize enhancements tab:", error);
      }
    };

    init();
  }, [registerEvent]);

  return <EnhancementsSection />;
}
