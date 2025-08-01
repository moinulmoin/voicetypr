import { createContext, useContext, useEffect, useState, ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { LicenseStatus } from '@/types';
import { toast } from 'sonner';

interface LicenseContextValue {
  status: LicenseStatus | null;
  isLoading: boolean;
  checkStatus: () => Promise<void>;
  restoreLicense: () => Promise<void>;
  activateLicense: (key: string) => Promise<void>;
  deactivateLicense: () => Promise<void>;
  openPurchasePage: () => Promise<void>;
}

const LicenseContext = createContext<LicenseContextValue | undefined>(undefined);

export function LicenseProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<LicenseStatus | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const checkStatus = async () => {
    try {
      setIsLoading(true);
      console.log(`[${new Date().toISOString()}] Frontend: Checking license status...`);
      const licenseStatus = await invoke<LicenseStatus>('check_license_status');
      console.log(`[${new Date().toISOString()}] Frontend: License status received:`, licenseStatus);
      setStatus(licenseStatus);
    } catch (error) {
      console.error('Failed to check license status:', error);
      toast.error('Failed to check license status');
    } finally {
      setIsLoading(false);
    }
  };

  const restoreLicense = async () => {
    try {
      const licenseStatus = await invoke<LicenseStatus>('restore_license');
      setStatus(licenseStatus);
      toast.success('License restored successfully');
    } catch (error: any) {
      console.error('Failed to restore license:', error);
      if (error.includes('No license found')) {
        toast.error('No license found. Please enter your license key manually.');
      } else if (error.includes('already activated on another device')) {
        toast.error('This license is already activated on another device');
      } else if (error.includes('maximum number of devices')) {
        toast.error('This license has reached its device activation limit');
      } else if (error.includes('Invalid license key')) {
        toast.error('Invalid license key');
      } else {
        toast.error(error || 'Failed to restore license');
      }
    }
  };

  const activateLicense = async (key: string) => {
    try {
      const licenseStatus = await invoke<LicenseStatus>('activate_license', { licenseKey: key });
      setStatus(licenseStatus);
      toast.success('License activated successfully');
    } catch (error: any) {
      console.error('Failed to activate license:', error);
      if (error.includes('already activated on another device')) {
        toast.error('This license is already activated on another device');
      } else if (error.includes('maximum number of devices')) {
        toast.error('This license has reached its device activation limit');
      } else if (error.includes('Invalid license key')) {
        toast.error('Invalid license key');
      } else {
        toast.error(error || 'Failed to activate license');
      }
    }
  };

  const deactivateLicense = async () => {
    try {
      await invoke('deactivate_license');
      // Re-check status after deactivation
      await checkStatus();
      toast.success('License deactivated. You can now use it on another device.');
    } catch (error: any) {
      console.error('Failed to deactivate license:', error);
      toast.error(error || 'Failed to deactivate license');
    }
  };

  const openPurchasePage = async () => {
    try {
      await invoke('open_purchase_page');
    } catch (error) {
      console.error('Failed to open purchase page:', error);
      // Fallback to window.open
      window.open('https://voicetypr.com/#pricing', '_blank');
    }
  };

  // Check license status on mount
  useEffect(() => {
    console.log(`[${new Date().toISOString()}] Frontend: LicenseProvider mounted, checking status...`);
    checkStatus();
  }, []);

  const value: LicenseContextValue = {
    status,
    isLoading,
    checkStatus,
    restoreLicense,
    activateLicense,
    deactivateLicense,
    openPurchasePage,
  };

  return (
    <LicenseContext.Provider value={value}>
      {children}
    </LicenseContext.Provider>
  );
}

export function useLicense() {
  const context = useContext(LicenseContext);
  if (!context) {
    throw new Error('useLicense must be used within a LicenseProvider');
  }
  return context;
}