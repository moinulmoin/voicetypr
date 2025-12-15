import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { open } from '@tauri-apps/plugin-shell';
import { getVersion } from '@tauri-apps/api/app';
import { 
  ExternalLink,
  Globe,
  Info,
  RefreshCw
} from "lucide-react";
import XIcon from "@/components/icons/XIcon";
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { updateService } from '@/services/updateService';

export function AboutSection() {
  const [appVersion, setAppVersion] = useState<string>('');
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);

  useEffect(() => {
    const fetchVersion = async () => {
      try {
        const version = await getVersion();
        setAppVersion(version);
      } catch (error) {
        console.error('Failed to get app version:', error);
        setAppVersion('Unknown');
      }
    };

    fetchVersion();
  }, []);

  const handleCheckUpdate = async () => {
    setIsCheckingUpdate(true);
    await updateService.checkForUpdatesManually();
    setIsCheckingUpdate(false);
  };

  const openExternalLink = async (url: string) => {
    try {
      await open(url);
    } catch (error) {
      console.error('Failed to open external link:', error);
      toast.error('Failed to open link');
    }
  };

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="px-6 py-4 border-b border-border/40">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold">About</h1>
            <p className="text-sm text-muted-foreground mt-1">
              App information and resources
            </p>
          </div>
        </div>
      </div>

      <ScrollArea className="flex-1">
        <div className="p-6 space-y-6">
          {/* App Information Section */}
          <div className="space-y-4">
            <h2 className="text-base font-semibold">App Information</h2>
            
            <div className="rounded-lg border border-border/50 bg-card p-4 space-y-4">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <Info className="h-4 w-4 text-muted-foreground" />
                  <span className="text-sm text-muted-foreground">Version</span>
                </div>
                <Badge variant="secondary" className="font-mono">
                  v{appVersion || 'Loading...'}
                </Badge>
              </div>

              <div className="flex justify-center">
                <Button
                  onClick={handleCheckUpdate}
                  disabled={isCheckingUpdate}
                  variant="ghost"
                  size="sm"
                >
                  <RefreshCw className={`h-3.5 w-3.5 mr-1.5 ${isCheckingUpdate ? 'animate-spin' : ''}`} />
                  {isCheckingUpdate ? 'Checking...' : 'Check for Updates'}
                </Button>
              </div>
            </div>
          </div>

          {/* Resources Section */}
          <div className="space-y-4">
            <h2 className="text-base font-semibold">Resources</h2>
            
            <div className="flex gap-3">
              <button
                onClick={() => openExternalLink("https://voicetypr.com")}
                className="flex-1 rounded-lg border border-border/50 bg-card p-4 flex items-center justify-between hover:bg-accent/50 transition-colors group"
              >
                <div className="flex items-center gap-3">
                  <div className="p-1.5 rounded-md bg-primary/10">
                    <Globe className="h-4 w-4 text-primary" />
                  </div>
                  <div className="text-left">
                    <p className="text-sm font-medium">Website</p>
                    <p className="text-xs text-muted-foreground">Official site</p>
                  </div>
                </div>
                <ExternalLink className="h-4 w-4 text-muted-foreground group-hover:text-foreground transition-colors" />
              </button>

              <button
                onClick={() => openExternalLink("https://x.com/voicetypr")}
                className="flex-1 rounded-lg border border-border/50 bg-card p-4 flex items-center justify-between hover:bg-accent/50 transition-colors group"
              >
                <div className="flex items-center gap-3">
                  <div className="p-1.5 rounded-md bg-accent">
                    <XIcon className="h-4 w-4 text-foreground" />
                  </div>
                  <div className="text-left">
                    <p className="text-sm font-medium">X</p>
                    <p className="text-xs text-muted-foreground">Follow for updates</p>
                  </div>
                </div>
                <ExternalLink className="h-4 w-4 text-muted-foreground group-hover:text-foreground transition-colors" />
              </button>
            </div>
          </div>
        </div>
      </ScrollArea>
    </div>
  );
}