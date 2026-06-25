import { AgentCliSection } from "../sections/AgentCliSection";
import { SettingsPage, SettingsHeader } from "@/components/settings/settings-ui";

export function AgentCliTab() {
  return (
    <SettingsPage>
      <SettingsHeader
        title="Agent & CLI"
        description="Drive Voicetypr from scripts and agents — a command-line interface plus a local HTTP API for programmatic transcription."
      />
      <AgentCliSection />
    </SettingsPage>
  );
}
