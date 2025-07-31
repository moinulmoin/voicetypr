import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { LicenseStatus } from '@/types';

export function useLicenseStatus() {
  const [licenseStatus, setLicenseStatus] = useState<LicenseStatus | null>(null);
  const [isChecking, setIsChecking] = useState(false);

  const checkLicense = useCallback(async () => {
    setIsChecking(true);
    try {
      const status = await invoke<LicenseStatus>('check_license_status');
      setLicenseStatus(status);
      return status;
    } catch (error) {
      console.error('Failed to check license status:', error);
      return null;
    } finally {
      setIsChecking(false);
    }
  }, []);

  // Check on mount
  useEffect(() => {
    checkLicense();
  }, [checkLicense]);

  // Provide computed values
  const isValid = licenseStatus ? 
    ['active', 'trial'].includes(licenseStatus.status) : 
    false;

  return {
    licenseStatus,
    isValid,
    isChecking,
    checkLicense
  };
}