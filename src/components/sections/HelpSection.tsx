import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { 
  ChevronDown,
  Mail,
  Mic,
  Keyboard,
  Type,
  Download,
  Copy,
  FileText
} from "lucide-react";
import XIcon from "@/components/icons/XIcon";
import { useState, useEffect } from 'react';
import { toast } from 'sonner';
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { platform, version as osVersion } from '@tauri-apps/plugin-os';
import { useSettings } from '@/contexts/SettingsContext';
import { useCanRecord, useCanAutoInsert } from '@/contexts/ReadinessContext';

interface QuickFix {
  id: string;
  title: string;
  icon: any;
  issue: string;
  solution: string;
  checkStatus?: () => boolean;
}

export function HelpSection() {
  const [appVersion, setAppVersion] = useState<string>('');
  const [platformName, setPlatformName] = useState<string>('');
  const [openItems, setOpenItems] = useState<string[]>([]);
  const [diagnostics, setDiagnostics] = useState<string>('');
  const { settings } = useSettings();
  const canRecord = useCanRecord();
  const canAutoInsert = useCanAutoInsert();

  useEffect(() => {
    const fetchSystemInfo = async () => {
      try {
        const [appVer, os, osVer] = await Promise.all([
          getVersion(),
          platform(),
          osVersion()
        ]);
        setAppVersion(appVer);
        setPlatformName(`${os} ${osVer}`);
        
        // Prepare diagnostics info
        const diag = `
App Version: ${appVer}
OS: ${os} ${osVer}
Model: ${settings?.current_model || 'None selected'}
Microphone Permission: ${canRecord ? 'Granted' : 'Not granted'}
Accessibility Permission: ${canAutoInsert ? 'Granted' : 'Not granted'}
        `.trim();
        setDiagnostics(diag);
      } catch (error) {
        console.error('Failed to get system info:', error);
      }
    };

    fetchSystemInfo();
  }, [settings, canRecord, canAutoInsert]);

  const quickFixes: QuickFix[] = [
    {
      id: 'recording',
      title: 'Recording not working',
      icon: Mic,
      issue: 'Voice recording doesn\'t start when using hotkey',
      solution: 'Go to Advanced section and check if Microphone permission is granted. Also check Settings to ensure a recording device is selected.',
      checkStatus: () => canRecord
    },
    {
      id: 'hotkey',
      title: 'Hotkey not responding',
      icon: Keyboard,
      issue: 'Global hotkey doesn\'t trigger recording',
      solution: 'Go to Advanced section and grant Accessibility permission. This is required for global hotkeys to work.',
      checkStatus: () => canAutoInsert
    },
    {
      id: 'insertion',
      title: 'Text not inserting',
      icon: Type,
      issue: 'Transcribed text doesn\'t appear at cursor',
      solution: 'Make sure your cursor is in an active text field. Accessibility permission must be granted in Advanced section.',
      checkStatus: () => canAutoInsert
    },
    {
      id: 'download',
      title: 'Model download stuck',
      icon: Download,
      issue: 'Whisper model download not progressing',
      solution: 'Go to Models section, cancel the current download and try again. Check your internet connection.'
    }
  ];

  const toggleItem = (itemId: string) => {
    setOpenItems(prev => 
      prev.includes(itemId) 
        ? prev.filter(id => id !== itemId)
        : [...prev, itemId]
    );
  };

  const handleEmailSupport = () => {
    const subject = "VoiceTypr Support Request";
    const body = `
${diagnostics}

Issue Description:
[Please describe your issue here]

Steps to reproduce:
1. 
2. 
3. 

Expected behavior:


Actual behavior:

`;
    
    const mailtoUrl = `mailto:support@voicetypr.com?subject=${encodeURIComponent(subject)}&body=${encodeURIComponent(body)}`;
    window.open(mailtoUrl, '_blank');
  };


  const handleTwitterSupport = () => {
    const twitterUrl = 'https://twitter.com/messages/compose?recipient_id=voicetypr';
    window.open(twitterUrl, '_blank');
  };

  const handleCopySystemInfo = async () => {
    try {
      await navigator.clipboard.writeText(diagnostics);
      toast.success('System info copied to clipboard');
    } catch (error) {
      console.error('Failed to copy system info:', error);
      toast.error('Failed to copy system info');
    }
  };

  const handleOpenLogs = async () => {
    try {
      await invoke('open_logs_folder');
      toast.info('Please attach the latest log file to your support message');
    } catch (error) {
      console.error('Failed to open logs folder:', error);
      toast.error('Failed to open logs folder');
    }
  };

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="px-6 py-4 border-b border-border/40">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold">Help & Support</h1>
            <p className="text-sm text-muted-foreground mt-1">
              Quick fixes and support resources
            </p>
          </div>
        </div>
      </div>

      <ScrollArea className="flex-1">
        <div className="p-6 space-y-6">
          {/* Quick Fixes Section */}
          <div className="space-y-4">
            <h2 className="text-base font-semibold">Quick Fixes</h2>
            
            <div className="space-y-2">
              {quickFixes.map(fix => {
                const Icon = fix.icon;
                const isOpen = openItems.includes(fix.id);
                
                return (
                  <Collapsible
                    key={fix.id}
                    open={isOpen}
                    onOpenChange={() => toggleItem(fix.id)}
                  >
                    <div className="rounded-lg border border-border/50 bg-card overflow-hidden">
                      <CollapsibleTrigger className="w-full px-4 py-3 flex items-center justify-between hover:bg-accent/50 transition-colors">
                        <div className="flex items-center gap-3">
                          <Icon className="h-4 w-4 text-muted-foreground" />
                          <span className="text-sm font-medium">{fix.title}</span>
                        </div>
                        <ChevronDown className={`h-4 w-4 text-muted-foreground transition-transform ${isOpen ? 'rotate-180' : ''}`} />
                      </CollapsibleTrigger>
                      
                      <CollapsibleContent>
                        <div className="px-4 pb-4 pt-2 space-y-3 border-t border-border/50">
                          <div className="space-y-2">
                            <div className="space-y-1">
                              <p className="text-xs font-medium text-muted-foreground">Issue</p>
                              <p className="text-sm">{fix.issue}</p>
                            </div>
                            
                            <div className="space-y-1 mt-3">
                              <p className="text-xs font-medium text-muted-foreground">Solution</p>
                              <p className="text-sm">{fix.solution}</p>
                            </div>
                          </div>
                        </div>
                      </CollapsibleContent>
                    </div>
                  </Collapsible>
                );
              })}
            </div>
          </div>

          {/* Contact Support Section */}
          <div className="space-y-4">
            <h2 className="text-base font-semibold">Get Support</h2>
            
            <div className="space-y-3">
              <button
                onClick={handleTwitterSupport}
                className="w-full rounded-lg border border-border/50 bg-card hover:bg-accent/50 transition-colors p-4 flex items-center justify-between group"
              >
                <div className="flex items-center gap-3">
                  <div className="p-2 rounded-lg bg-primary/10 group-hover:bg-primary/20 transition-colors">
                    <XIcon className="h-4 w-4" />
                  </div>
                  <div className="text-left">
                    <p className="text-sm font-medium">DM us on X</p>
                    <p className="text-xs text-muted-foreground">
                      Get quick support via direct message
                    </p>
                  </div>
                </div>
                <ChevronDown className="h-4 w-4 text-muted-foreground -rotate-90" />
              </button>

              <button
                onClick={handleEmailSupport}
                className="w-full rounded-lg border border-border/50 bg-card hover:bg-accent/50 transition-colors p-4 flex items-center justify-between group"
              >
                <div className="flex items-center gap-3">
                  <div className="p-2 rounded-lg bg-primary/10 group-hover:bg-primary/20 transition-colors">
                    <Mail className="h-4 w-4" />
                  </div>
                  <div className="text-left">
                    <p className="text-sm font-medium">Email Support</p>
                    <p className="text-xs text-muted-foreground">
                      Send us an email with diagnostic info
                    </p>
                  </div>
                </div>
                <ChevronDown className="h-4 w-4 text-muted-foreground -rotate-90" />
              </button>
            </div>
          </div>

          {/* Diagnostics Section */}
          <div className="space-y-4">
            <h2 className="text-base font-semibold">Diagnostics</h2>
            
            <div className="space-y-3">
              <button
                onClick={handleCopySystemInfo}
                className="w-full rounded-lg border border-border/50 bg-card hover:bg-accent/50 transition-colors p-4 flex items-center justify-between group"
              >
                <div className="flex items-center gap-3">
                  <div className="p-2 rounded-lg bg-primary/10 group-hover:bg-primary/20 transition-colors">
                    <Copy className="h-4 w-4" />
                  </div>
                  <div className="text-left">
                    <p className="text-sm font-medium">Copy System Info</p>
                    <p className="text-xs text-muted-foreground">
                      Copy basic system information to clipboard
                    </p>
                  </div>
                </div>
                <ChevronDown className="h-4 w-4 text-muted-foreground -rotate-90" />
              </button>

              <button
                onClick={handleOpenLogs}
                className="w-full rounded-lg border border-border/50 bg-card hover:bg-accent/50 transition-colors p-4 flex items-center justify-between group"
              >
                <div className="flex items-center gap-3">
                  <div className="p-2 rounded-lg bg-primary/10 group-hover:bg-primary/20 transition-colors">
                    <FileText className="h-4 w-4" />
                  </div>
                  <div className="text-left">
                    <p className="text-sm font-medium">Open Logs Folder</p>
                    <p className="text-xs text-muted-foreground">
                      Open logs folder to attach to support messages
                    </p>
                  </div>
                </div>
                <ChevronDown className="h-4 w-4 text-muted-foreground -rotate-90" />
              </button>
            </div>
          </div>

          {/* System Info Footer */}
          <div className="pt-4">
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>VoiceTypr v{appVersion}</span>
              <span>{platformName}</span>
            </div>
          </div>
        </div>
      </ScrollArea>
    </div>
  );
}