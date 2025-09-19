import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useLicense } from "@/contexts/LicenseContext";
import { open } from '@tauri-apps/plugin-shell';
import { ask } from '@tauri-apps/plugin-dialog';
import {
  Check,
  Clock,
  Crown,
  Shield
} from "lucide-react";
import { useState } from 'react';
import { toast } from 'sonner';

export function AccountSection() {
  const { status, isLoading, activateLicense, deactivateLicense, openPurchasePage } = useLicense();
  const [licenseKey, setLicenseKey] = useState('');
  const [isActivating, setIsActivating] = useState(false);

  const handleActivate = async () => {
    if (!licenseKey.trim()) return;

    setIsActivating(true);
    await activateLicense(licenseKey.trim());
    setIsActivating(false);
    setLicenseKey('');
  };

  const handleDeactivate = async () => {
    const confirmed = await ask(
      'Deactivating your license will make the app unusable.',
      {
        title: 'Deactivate License',
        kind: 'warning',
        okLabel: 'Confirm',
        cancelLabel: 'Cancel'
      }
    );

    if (confirmed) {
      await deactivateLicense();
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
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="px-6 py-4 border-b border-border/40">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold">License</h1>
            <p className="text-sm text-muted-foreground mt-1">
              Manage your VoiceTypr license
            </p>
          </div>
          {status && status.status === 'licensed' && (
            <div className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-green-500/10">
              <Crown className="h-4 w-4 text-green-600 dark:text-green-400" />
              <span className="text-sm font-medium text-green-600 dark:text-green-400">
                Pro Licensed
              </span>
            </div>
          )}
        </div>
      </div>

      <ScrollArea className="flex-1">
        <div className="p-6 space-y-6">
          {/* License Status Section */}
          <div className="space-y-4">
            <div className="rounded-lg border border-border/50 bg-card p-4 space-y-4">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <Shield className="h-4 w-4 text-muted-foreground" />
                  <span className="text-sm text-muted-foreground">Status</span>
                </div>
                <Badge variant={getStatusBadgeVariant()} className="font-medium">
                  {isLoading ? 'Loading...' : formatLicenseStatus()}
                </Badge>
              </div>

              {/* Licensed user info */}
              {status && status.status === 'licensed' && (
                <div className="space-y-4">
                  <div className="bg-green-500/10 border border-green-500/20 rounded-lg p-4 space-y-3">
                    <div className="flex items-start gap-3">
                      <div className="p-1.5 rounded-md bg-green-500/10">
                        <Check className="h-4 w-4 text-green-600 dark:text-green-400" />
                      </div>
                      <div className="flex-1 space-y-1">
                        <p className="text-sm font-medium text-green-900 dark:text-green-100">
                          VoiceTypr Pro Active
                        </p>
                        {status.license_key && (
                          <p className="text-xs text-green-700 dark:text-green-300 font-mono">
                            License: ****-****-****-{status.license_key.slice(-4)}
                          </p>
                        )}
                        <p className="text-xs text-muted-foreground mt-2">
                          All pro features unlocked
                        </p>
                      </div>
                    </div>
                  </div>

                  <Button
                    onClick={handleDeactivate}
                    variant="outline"
                    size="sm"
                    className="w-full"
                  >
                    Deactivate License
                  </Button>
                </div>
              )}

              {/* Actions for unlicensed/expired users */}
              {status && (status.status === 'expired' || status.status === 'none' || status.status === 'trial') && (
                <div className="space-y-4">
                  {/* Trial/Expired Notice */}
                  {(status.status === 'trial' || status.status === 'expired') && (
                    <div className="bg-amber-500/10 border border-amber-500/20 rounded-lg p-4">
                      <div className="flex items-start gap-3">
                        <div className="p-1.5 rounded-md bg-amber-500/10">
                          <Clock className="h-4 w-4 text-amber-500" />
                        </div>
                        <div className="flex-1 space-y-1">
                          <p className="text-sm font-medium text-amber-900 dark:text-amber-400">
                            {status.status === 'trial' ? 'Trial Active' : 'Trial Expired'}
                          </p>
                          <p className="text-xs text-amber-800 dark:text-amber-500">
                            {status.status === 'trial' && status.trial_days_left !== undefined
                              ? status.trial_days_left > 0
                                ? `${status.trial_days_left} day${status.trial_days_left !== 1 ? 's' : ''} remaining in your trial`
                                : 'Trial expires today'
                              : 'Upgrade to Pro to continue'}
                          </p>
                        </div>
                      </div>
                    </div>
                  )}

                  <div className="flex gap-2">
                    <Button
                      onClick={openPurchasePage}
                      className="flex-1"
                      size="sm"
                    >
                      <Crown className="h-3.5 w-3.5 mr-1.5" />
                      Buy License
                    </Button>
                    <Button
                      onClick={() => openExternalLink("https://polar.sh/ideaplexa/portal")}
                      variant="outline"
                      size="sm"
                      className="flex-1"
                    >
                      Manage License
                    </Button>
                  </div>

                  <div className="space-y-2 pt-2 border-t border-border/50">
                    <p className="text-sm font-medium">Have a license key?</p>
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
                        className="flex-1 text-sm"
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
                      You may be prompted for your password to securely store the license
                    </p>
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      </ScrollArea>
    </div>
  );
}