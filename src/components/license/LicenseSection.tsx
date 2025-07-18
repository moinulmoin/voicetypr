import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { useLicense } from '@/contexts/LicenseContext';
import { useState } from 'react';

export function LicenseSection() {
  const { status, isLoading, restoreLicense, activateLicense, deactivateLicense, openPurchasePage } = useLicense();
  const [licenseKey, setLicenseKey] = useState('');
  const [isActivating, setIsActivating] = useState(false);

  const handleActivate = async () => {
    if (!licenseKey.trim()) return;

    setIsActivating(true);
    await activateLicense(licenseKey.trim());
    setIsActivating(false);
    setLicenseKey('');
  };

  const formatLicenseStatus = () => {
    if (!status) return 'Loading...';

    switch (status.status) {
      case 'licensed':
        return `Licensed`;
      case 'trial':
        return status.trial_days_left !== undefined
          ? status.trial_days_left > 0
            ? `Trial - ${status.trial_days_left} days remaining`
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
    <div className="p-6 space-y-6">
      <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">License Management</h2>

      {/* Current Status */}
      <div className="flex items-center justify-between">
        <span className="text-sm text-gray-600 dark:text-gray-400">Status</span>
        <Badge variant={getStatusBadgeVariant()}>
          {isLoading ? 'Loading...' : formatLicenseStatus()}
        </Badge>
      </div>

      {/* Actions for unlicensed/expired users */}
      {status && (status.status === 'expired' || status.status === 'none' || status.status === 'trial') && (
        <>
          {/* Buy and Restore buttons side by side */}
          <div className="flex items-center justify-center gap-3 mt-6">
            <Button
              onClick={openPurchasePage}
            >
              Buy License

            </Button>

            <span className="text-sm text-gray-500 dark:text-gray-400">OR</span>

            <Button
              onClick={restoreLicense}
              variant="outline"
            >
              Restore license
            </Button>
          </div>


          {/* License key activation at the bottom */}
          <div className="space-y-3 mt-20">
            <div className="space-y-2">
              <label className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                Have a license key?
              </label>
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
                  className="flex-1"
                />
                <Button
                  onClick={handleActivate}
                  disabled={!licenseKey.trim() || isActivating}
                >
                  {isActivating ? 'Activating...' : 'Activate'}
                </Button>
              </div>
            </div>
          </div>
        </>
      )}

      {/* Licensed user info */}
      {status && status.status === 'licensed' && (
        <div className="space-y-4">
          <div className="bg-green-50 dark:bg-green-900/20 rounded-lg p-4 space-y-3">
            {/* <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Check className="w-5 h-5 text-green-600 dark:text-green-400" />
                <span className="font-medium text-green-900 dark:text-green-100">
                  VoiceTypr {status.license_type || 'Pro'}
                </span>
              </div>
              <Badge variant="default" className="bg-green-600 hover:bg-green-600">
                Active
              </Badge>
            </div> */}
            {status.license_key && (
              <div className="flex justify-between items-center">
                <p className="text-xs text-green-700 dark:text-green-300 font-medium">License Key</p>
                <p className="text-sm text-green-800 dark:text-green-200 font-mono">
                  ****-****-****-{status.license_key.slice(-4)}
                </p>
              </div>
            )}
          </div>

          <div className="space-y-3">
            <Button
              onClick={deactivateLicense}
              variant="outline"
              className="w-full"
            >
              Deactivate License
            </Button>
            <p className="text-xs text-gray-500 dark:text-gray-400 text-center">
              Deactivate to use this license on another device
            </p>
          </div>
        </div>
      )}

    </div>
  );
}