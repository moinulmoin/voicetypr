import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { NotebookText, Mail, RefreshCw, CheckCircle, Twitter, RotateCcw } from "lucide-react";
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { toast } from 'sonner';
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { AppSettings } from '@/types';

export function AboutSection() {
  const [checking, setChecking] = useState(false);
  const [resetting, setResetting] = useState(false);

  const handleResetOnboarding = async () => {
    setResetting(true);
    try {
      // Get current settings and set onboarding_completed to false
      const settings = await invoke<AppSettings>('get_settings');
      await invoke('save_settings', {
        settings: {
          ...settings,
          onboarding_completed: false,
        },
      });
      
      toast.success("Onboarding reset! Please restart the app.");
      
      // Reload the window to trigger onboarding
      setTimeout(() => {
        window.location.reload();
      }, 1000);
    } catch (error) {
      console.error('Failed to reset onboarding:', error);
      toast.error("Failed to reset onboarding");
    } finally {
      setResetting(false);
    }
  };

  const handleCheckForUpdates = async () => {
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

  const openExternalLink = (url: string) => {
    window.open(url, "_blank", "noopener,noreferrer");
  };

  return (
    <div className="p-6">
      <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-6">About VoiceTypr</h2>
      
      <div className="space-y-6">
        {/* App Info Section */}
        <div className="space-y-4">
          {/* Version */}
          <div className="flex items-center gap-3">
            <p className="text-sm text-gray-600 dark:text-gray-400">Version</p>
            <p className="text-base font-medium">0.1.0</p>
          </div>
          
          {/* License Status */}
          <div className="flex items-center gap-3">
            <p className="text-sm text-gray-600 dark:text-gray-400">License</p>
            <Badge variant="default" className="bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-100">
              <CheckCircle className="w-3 h-3 mr-1" />
              Active
            </Badge>
          </div>
        </div>

        {/* Links Section */}
        <div className="flex items-center gap-6 mt-8">
            <button
              onClick={() => openExternalLink("https://voicetypr.com/changelog")}
              className="flex items-center gap-2 text-sm text-gray-900 dark:text-gray-100 hover:text-gray-600 dark:hover:text-gray-400 hover:underline underline-offset-4"
            >
              <NotebookText className="w-4 h-4" />
              Changelog
            </button>
            <button
              onClick={() => openExternalLink("mailto:support@voicetypr.com")}
              className="flex items-center gap-2 text-sm text-gray-900 dark:text-gray-100 hover:text-gray-600 dark:hover:text-gray-400 hover:underline underline-offset-4"
            >
              <Mail className="w-4 h-4" />
              support@voicetypr.com
            </button>
            <button
              onClick={() => openExternalLink("https://twitter.com/voicetypr")}
              className="flex items-center gap-2 text-sm text-gray-900 dark:text-gray-100 hover:text-gray-600 dark:hover:text-gray-400 hover:underline underline-offset-4"
            >
              <Twitter className="w-4 h-4" />
              @voicetypr
            </button>
        </div>

        {/* Action Buttons */}
        <div className="mt-12 flex justify-center gap-3">
          <Button 
            size="sm" 
            variant="outline" 
            onClick={handleCheckForUpdates}
            className="h-8"
            disabled={checking}
          >
            <RefreshCw className={`w-3 h-3 mr-1 ${checking ? 'animate-spin' : ''}`} />
            {checking ? "Checking..." : "Check for Updates"}
          </Button>
          
          <Button 
            size="sm" 
            variant="outline" 
            onClick={handleResetOnboarding}
            className="h-8"
            disabled={resetting}
          >
            <RotateCcw className={`w-3 h-3 mr-1 ${resetting ? 'animate-spin' : ''}`} />
            {resetting ? "Resetting..." : "Reset Onboarding"}
          </Button>
        </div>
      </div>
    </div>
  );
}