import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { Button } from '@/components/ui/button';
import { toast } from 'sonner';
import { useState } from 'react';

export function UpdateSection() {
  const [checking, setChecking] = useState(false);

  const handleCheckUpdates = async () => {
    setChecking(true);
    try {
      const update = await check();
      if (update?.available) {
        // Tauri's dialog will handle the rest
        await update.downloadAndInstall();
        await relaunch();
      } else {
        toast.success("You're on the latest version!");
      }
    } catch (error) {
      console.error('Update check failed:', error);
      toast.error("Failed to check for updates");
    } finally {
      setChecking(false);
    }
  };

  return (
    <div className="space-y-2">
      <h3 className="text-sm font-medium">Updates</h3>
      <p className="text-xs text-muted-foreground">
        Check for new versions of VoiceTypr
      </p>
      <Button 
        onClick={handleCheckUpdates} 
        variant="outline" 
        size="sm"
        disabled={checking}
      >
        {checking ? "Checking..." : "Check for Updates"}
      </Button>
    </div>
  );
}