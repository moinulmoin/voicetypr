import { NetworkSharingCard } from "../sections/NetworkSharingCard";
import { SettingsPage, SettingsHeader } from "@/components/settings/settings-ui";

export function NetworkSharingTab() {
  return (
    <SettingsPage>
      <SettingsHeader
        title="Network sharing"
        description="Share this device's transcription engine with other Voicetypr apps on your network — or route your dictation to one."
      />
      <NetworkSharingCard />
    </SettingsPage>
  );
}
