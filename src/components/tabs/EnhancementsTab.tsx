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
            description: "Please update your API key in the Enhancements section",
            action: {
              label: "Update API Key",
              onClick: () => {
                // Already on enhancements tab, show guidance
                toast.info('API Key Required', {
                  description: 'Please enter your API key for the AI service you want to use.',
                  duration: 5000
                });
              }
            }
          });
        });

        registerEvent("ai-enhancement-error", (event) => {
          console.warn("AI enhancement error:", event.payload);
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