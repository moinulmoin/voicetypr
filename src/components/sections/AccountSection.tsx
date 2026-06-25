import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  SettingsCard,
  SettingsHeader,
  SettingsPage,
  SettingRow,
} from "@/components/settings/settings-ui";
import { useLicense } from "@/contexts/LicenseContext";
import { open } from '@tauri-apps/plugin-shell';
import { ask } from '@tauri-apps/plugin-dialog';
import {
  Check,
  Clock,
  Crown,
  HelpCircle,
  Shield
} from "lucide-react";
import { useState } from 'react';
import { toast } from 'sonner';
import { createLogger } from "@/lib/logger";

const log = createLogger("account");

export function AccountSection() {
  const { status, isLoading, checkStatus, activateLicense, deactivateLicense, openPurchasePage } = useLicense();
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
      log.error('Failed to open external link:', error);
      toast.error('Failed to open link');
    }
  };

  const formatLicenseStatus = () => {
    if (!status) return 'Unknown';

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

  const isUnlicensed =
    !isLoading && (!status || status.status === 'expired' || status.status === 'none' || status.status === 'trial');

  return (
    <SettingsPage>
      <SettingsHeader
        title={
          <span className="flex items-center gap-2">
            Licensing
            <Dialog>
              <DialogTrigger asChild>
                <Button type="button" variant="ghost" size="icon-sm" aria-label="Licensing guide" className="size-7 rounded-full text-muted-foreground">
                  <HelpCircle className="h-4 w-4" />
                </Button>
              </DialogTrigger>
              <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                  <DialogTitle>Licensing guide</DialogTitle>
                  <DialogDescription>
                    Manage your trial and activate or remove a Pro license.
                  </DialogDescription>
                </DialogHeader>
                <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                  <p><strong className="text-foreground">Trial</strong> shows the remaining trial state when no Pro license is active.</p>
                  <p><strong className="text-foreground">License activation</strong> validates the key and stores only the app needs to confirm status.</p>
                  <p><strong className="text-foreground">Purchase</strong> opens the checkout flow when you need to upgrade from trial or free.</p>
                </div>
              </DialogContent>
            </Dialog>
          </span>
        }
        description="Your trial and license activation."
        actions={
          status && status.status === 'licensed' ? (
            <div className="flex items-center gap-2 rounded-lg bg-green-500/10 px-3 py-1.5">
              <Crown className="h-4 w-4 text-green-600 dark:text-green-400" />
              <span className="text-sm font-medium text-green-600 dark:text-green-400">
                Pro Licensed
              </span>
            </div>
          ) : undefined
        }
      />

      <SettingsCard
        icon={Shield}
        title="License status"
        description="Your current trial or Pro license state."
      >
        <SettingRow
          title="Status"
          control={
            <Badge variant={getStatusBadgeVariant()} className="font-medium">
              {isLoading ? 'Loading...' : formatLicenseStatus()}
            </Badge>
          }
        />

        {!isLoading && !status && (
          <SettingRow
            title="Couldn’t load license status"
            description="We weren’t able to read your license. Try again."
            control={
              <Button onClick={checkStatus} variant="outline" size="sm">
                Retry
              </Button>
            }
          />
        )}

        {/* Licensed user info */}
        {status && status.status === 'licensed' && (
          <div className="mt-4 space-y-4">
            <div className="space-y-3 rounded-lg border border-green-500/20 bg-green-500/10 p-4">
              <div className="flex items-start gap-3">
                <div className="rounded-md bg-green-500/10 p-1.5">
                  <Check className="h-4 w-4 text-green-600 dark:text-green-400" />
                </div>
                <div className="flex-1 space-y-1">
                  <p className="text-sm font-medium text-green-900 dark:text-green-100">
                    Voicetypr Pro Active
                  </p>
                  {status.license_key && (
                    <p className="font-mono text-xs text-green-700 dark:text-green-300">
                      License: ****-****-****-{status.license_key.slice(-4)}
                    </p>
                  )}
                  <p className="mt-2 text-xs text-muted-foreground">
                    All pro features unlocked
                  </p>
                </div>
              </div>
            </div>

            <div className="flex gap-2">
              <Button
                onClick={() => openExternalLink("https://polar.sh/ideaplexa/portal")}
                variant="outline"
                size="sm"
                className="flex-1"
              >
                Manage License
              </Button>
              <Button
                onClick={handleDeactivate}
                variant="outline"
                size="sm"
                className="flex-1"
              >
                Deactivate License
              </Button>
            </div>
          </div>
        )}

        {/* Trial/Expired notice for unlicensed users */}
        {status && (status.status === 'trial' || status.status === 'expired') && (
          <div className="mt-4 rounded-lg border border-amber-500/20 bg-amber-500/10 p-4">
            <div className="flex items-start gap-3">
              <div className="rounded-md bg-amber-500/10 p-1.5">
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
      </SettingsCard>

      {/* Activate / purchase for unlicensed, expired, or trial users */}
      {isUnlicensed && (
        <SettingsCard
          icon={Crown}
          title="Activate license"
          description="Upgrade to Pro or enter an existing license key."
        >
          <div className="mt-4 space-y-4">
            <div className="flex gap-2">
              <Button
                onClick={openPurchasePage}
                className="flex-1"
                size="sm"
              >
                <Crown className="mr-1.5 h-3.5 w-3.5" />
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

            <div className="space-y-2 border-t border-border/50 pt-4">
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
        </SettingsCard>
      )}
    </SettingsPage>
  );
}
