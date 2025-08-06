import React, { createContext, useContext } from 'react';
import { isMacOS, isWindows } from '@/lib/platform';

interface PlatformContextType {
  isMac: boolean;
  isWindows: boolean;
}

const PlatformContext = createContext<PlatformContextType>({
  isMac: false,
  isWindows: false,
});

export function PlatformProvider({ children }: { children: React.ReactNode }) {


  const value: PlatformContextType = {
    isMac: isMacOS,
    isWindows: isWindows,
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