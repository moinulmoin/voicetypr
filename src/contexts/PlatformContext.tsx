import React, { createContext, useContext, useEffect, useState } from 'react';
import { getPlatform, type Platform } from '@/lib/platform';

interface PlatformContextType {
  platform: Platform | null;
  isMac: boolean;
  isWindows: boolean;
  isLinux: boolean;
  isLoading: boolean;
}

const PlatformContext = createContext<PlatformContextType>({
  platform: null,
  isMac: false,
  isWindows: false,
  isLinux: false,
  isLoading: true,
});

export function PlatformProvider({ children }: { children: React.ReactNode }) {
  const [platform, setPlatform] = useState<Platform | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    getPlatform().then((detectedPlatform) => {
      setPlatform(detectedPlatform);
      setIsLoading(false);
    });
  }, []);

  const value: PlatformContextType = {
    platform,
    isMac: platform === 'darwin',
    isWindows: platform === 'windows',
    isLinux: platform === 'linux',
    isLoading,
  };

  return (
    <PlatformContext.Provider value={value}>
      {children}
    </PlatformContext.Provider>
  );
}

export function usePlatform() {
  const context = useContext(PlatformContext);
  if (!context) {
    throw new Error('usePlatform must be used within a PlatformProvider');
  }
  return context;
}