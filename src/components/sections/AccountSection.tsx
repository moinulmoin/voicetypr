import { XformerlyTwitter } from "@/assets/icon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useLicense } from "@/contexts/LicenseContext";
import { getVersion } from '@tauri-apps/api/app';
import { open } from '@tauri-apps/plugin-shell';
import { 
  Check, 
  Globe, 
  Info, 
  KeyRound, 
  Mail, 
  RefreshCw
} from "lucide-react";
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { updateService } from '@/services/updateService';

export function AccountSection() {
  const { status, isLoading, restoreLicense, activateLicense, deactivateLicense, openPurchasePage } = useLicense();
  const [licenseKey, setLicenseKey] = useState('');
  const [isActivating, setIsActivating] = useState(false);
  const [appVersion, setAppVersion] = useState<string>('Loading...');
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => setAppVersion('Unknown'));
  }, []);

  const handleActivate = async () => {
    if (!licenseKey.trim()) return;

    setIsActivating(true);
    await activateLicense(licenseKey.trim());
    setIsActivating(false);
    setLicenseKey('');
  };

  const handleCheckForUpdates = async () => {
    setChecking(true);
    try {
      await updateService.checkForUpdatesManually();
    } finally {
      setChecking(false);
    }
  };

  const openExternalLink = async (url: string) => {
    try {
      await open(url);
    } catch (error) {
      console.error('Failed to open external link:', error);
      toast.error('Failed to open link');
    }
  };

  const formatLicenseStatus = () => {
    if (!status) return 'Loading...';

    switch (status.status) {
      case 'licensed':
        return `Licensed`;
      case 'trial':
        return status.trial_days_left !== undefined
          ? status.trial_days_left > 0
            ? `Trial - ${status.trial_days_left} day${status.trial_days_left > 1 ? 's' : ''}`
            : 'Trial expires today'
          : 'Trial (3-day limit)';
      case 'expired':
        return 'Trial Expired';
      case 'none':
        return 'No License';
      default:
        return 'Unknown';
    }
  };

  const getStatusBadgeVariant = () => {
    if (!status) return 'secondary';

    switch (status.status) {
      case 'licensed':
        return 'default';
      case 'trial':
        return 'secondary';
      case 'expired':
        return 'destructive';
      case 'none':
        return 'outline';
      default:
        return 'secondary';
    }
  };

  return (
    <div className="h-full flex flex-col p-6">
      <div className="flex-shrink-0 mb-4 space-y-3">
        <h2 className="text-lg font-semibold">Account</h2>
        <p className="text-sm text-muted-foreground">
          Manage your license and app information
        </p>
      </div>

      <ScrollArea className="flex-1 min-h-0">
        <div className="space-y-6">
          {/* License Status Section */}
          <div className="rounded-lg border bg-card p-4 space-y-4">
            <div className="flex items-center gap-2 mb-2">
              <KeyRound className="h-4 w-4 text-muted-foreground" />
              <h3 className="text-sm font-medium">License Status</h3>
            </div>

            <div className="flex items-center justify-between">
              <span className="text-sm text-muted-foreground">Status</span>
              <Badge variant={getStatusBadgeVariant()}>
                {isLoading ? 'Loading...' : formatLicenseStatus()}
              </Badge>
            </div>

            {/* Licensed user info */}
            {status && status.status === 'licensed' && (
              <div className="space-y-4">
                <div className="bg-green-50 dark:bg-green-900/20 rounded-lg p-3 space-y-2">
                  <div className="flex items-center gap-2">
                    <Check className="h-4 w-4 text-green-600 dark:text-green-400" />
                    <span className="text-sm font-medium text-green-900 dark:text-green-100">
                      VoiceTypr Pro Active
                    </span>
                  </div>
                  {status.license_key && (
                    <p className="text-xs text-green-700 dark:text-green-300 font-mono">
                      License: ****-****-****-{status.license_key.slice(-4)}
                    </p>
                  )}
                </div>

                <Button
                  onClick={deactivateLicense}
                  variant="outline"
                  size="sm"
                  className="w-full"
                >
                  Deactivate License
                </Button>
                <p className="text-xs text-muted-foreground text-center">
                  Deactivate to use this license on another device
                </p>
              </div>
            )}

            {/* Actions for unlicensed/expired users */}
            {status && (status.status === 'expired' || status.status === 'none' || status.status === 'trial') && (
              <div className="space-y-4">
                <div className="flex gap-2">
                  <Button
                    onClick={openPurchasePage}
                    className="flex-1"
                    size="sm"
                  >
                    Buy License
                  </Button>
                  <Button
                    onClick={restoreLicense}
                    variant="outline"
                    size="sm"
                    className="flex-1"
                  >
                    Restore
                  </Button>
                </div>

                <div className="space-y-2">
                  <p className="text-xs font-medium text-muted-foreground">Have a license key?</p>
                  <div className="flex gap-2">
                    <Input
                      placeholder="Enter license key"
                      value={licenseKey}
                      onChange={(e) => setLicenseKey(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') {
                          handleActivate();
                        }
                      }}
                      className="flex-1 h-8 text-sm"
                    />
                    <Button
                      onClick={handleActivate}
                      disabled={!licenseKey.trim() || isActivating}
                      size="sm"
                    >
                      {isActivating ? 'Activating...' : 'Activate'}
                    </Button>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    Note: You may be prompted for your password to securely store the license
                  </p>
                </div>
              </div>
            )}
          </div>

          {/* App Info Section */}
          <div className="rounded-lg border bg-card p-4 space-y-4">
            <div className="flex items-center gap-2 mb-2">
              <Info className="h-4 w-4 text-muted-foreground" />
              <h3 className="text-sm font-medium">App Info</h3>
            </div>
            
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">Version</span>
                <span className="text-sm font-medium">{appVersion}</span>
              </div>

              <Button
                onClick={handleCheckForUpdates}
                disabled={checking}
                variant="outline"
                size="sm"
                className="w-full"
              >
                {checking ? (
                  <>
                    <RefreshCw className="mr-2 h-4 w-4 animate-spin" />
                    Checking for Updates...
                  </>
                ) : (
                  <>
                    <RefreshCw className="mr-2 h-4 w-4" />
                    Check for Updates
                  </>
                )}
              </Button>

              <div className="pt-3 flex items-center justify-between text-sm">
                <button
                  onClick={() => openExternalLink("https://voicetypr.com")}
                  className="flex items-center gap-1.5 text-muted-foreground hover:text-foreground transition-colors"
                >
                  <Globe className="h-4 w-4" />
                  <span>Website</span>
                </button>
                
                <button
                  onClick={() => openExternalLink("mailto:support@voicetypr.com")}
                  className="flex items-center gap-1.5 text-muted-foreground hover:text-foreground transition-colors"
                >
                  <Mail className="h-4 w-4" />
                  <span>Support</span>
                </button>
                
                <button
                  onClick={() => openExternalLink("https://twitter.com/voicetypr")}
                  className="flex items-center gap-1.5 text-muted-foreground hover:text-foreground transition-colors"
                >
                  <XformerlyTwitter className="h-4 w-4" />
                  <span>@voicetypr</span>
                </button>
              </div>
            </div>
          </div>
        </div>
      </ScrollArea>
    </div>
  );
}