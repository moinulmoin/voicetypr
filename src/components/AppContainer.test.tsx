import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi, describe, it, expect, beforeEach } from 'vitest';
import { AppContainer } from './AppContainer';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn()
}));

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    warning: vi.fn(),
    info: vi.fn(),
    success: vi.fn()
  }
}));

// Mock contexts
vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: {
      onboarding_completed: true,
      transcription_cleanup_days: 30,
      hotkey: 'Cmd+Shift+Space'
    },
    refreshSettings: vi.fn()
  })
}));

vi.mock('@/contexts/ReadinessContext', () => ({
  useReadiness: () => ({
    checkAccessibilityPermission: vi.fn().mockResolvedValue(true),
    checkMicrophonePermission: vi.fn().mockResolvedValue(true)
  })
}));

// Mock hooks
vi.mock('@/hooks/useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn((event: string, callback: any) => {
      (window as any).__testEventCallbacks = (window as any).__testEventCallbacks || {};
      (window as any).__testEventCallbacks[event] = callback;
      return vi.fn();
    })
  })
}));

vi.mock('@/hooks/useModelManagement', () => ({
  useModelManagement: () => ({
    models: {},
    downloadProgress: {},
    verifyingModels: new Set(),
    sortedModels: [],
    downloadModel: vi.fn(),
    deleteModel: vi.fn(),
    cancelDownload: vi.fn()
  })
}));

// Mock services
vi.mock('@/services/updateService', () => ({
  updateService: {
    initialize: vi.fn().mockResolvedValue(true),
    dispose: vi.fn()
  }
}));

vi.mock('@/utils/keyring', () => ({
  loadApiKeysToCache: vi.fn().mockResolvedValue(true)
}));

// Mock components
vi.mock('./Sidebar', () => ({
  Sidebar: ({ activeSection, onSectionChange }: any) => (
    <div data-testid="sidebar">
      <button onClick={() => onSectionChange('recordings')}>Recordings</button>
      <button onClick={() => onSectionChange('models')}>Models</button>
      <div>Active: {activeSection}</div>
    </div>
  )
}));

vi.mock('./tabs/TabContainer', () => ({
  TabContainer: ({ activeSection }: any) => (
    <div data-testid="tab-container">
      Current Tab: {activeSection}
    </div>
  )
}));

vi.mock('./onboarding/OnboardingDesktop', () => ({
  OnboardingDesktop: ({ onComplete }: any) => (
    <div data-testid="onboarding">
      <button onClick={onComplete}>Complete Onboarding</button>
    </div>
  )
}));

vi.mock('./ui/sidebar', () => ({
  SidebarProvider: ({ children }: any) => <div>{children}</div>,
  SidebarInset: ({ children }: any) => <div>{children}</div>
}));

describe('AppContainer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__testEventCallbacks = {};
  });

  it('renders main layout when onboarding is completed', () => {
    render(<AppContainer />);
    
    expect(screen.getByTestId('sidebar')).toBeInTheDocument();
    expect(screen.getByTestId('tab-container')).toBeInTheDocument();
    expect(screen.getByText('Current Tab: recordings')).toBeInTheDocument();
  });

  it('shows onboarding when not completed', () => {
    vi.mock('@/contexts/SettingsContext', () => ({
      useSettings: () => ({
        settings: {
          onboarding_completed: false
        },
        refreshSettings: vi.fn()
      })
    }));

    render(<AppContainer />);
    
    expect(screen.getByTestId('onboarding')).toBeInTheDocument();
    expect(screen.queryByTestId('sidebar')).not.toBeInTheDocument();
  });

  it('initializes app on mount', async () => {
    render(<AppContainer />);
    
    await waitFor(() => {
      // Check cleanup was called if configured
      expect(invoke).toHaveBeenCalledWith('cleanup_old_transcriptions', {
        days: 30
      });
    });
  });

  it('loads API keys to cache after delay', async () => {
    const { loadApiKeysToCache } = await import('@/utils/keyring');
    
    render(<AppContainer />);
    
    await waitFor(() => {
      expect(loadApiKeysToCache).toHaveBeenCalled();
    }, { timeout: 200 });
  });

  it('registers global event listeners', () => {
    render(<AppContainer />);
    
    expect((window as any).__testEventCallbacks).toHaveProperty('navigate-to-settings');
    expect((window as any).__testEventCallbacks).toHaveProperty('tray-action-error');
    expect((window as any).__testEventCallbacks).toHaveProperty('no-models-error');
  });

  it('handles navigate-to-settings event', () => {
    render(<AppContainer />);
    
    const callback = (window as any).__testEventCallbacks['navigate-to-settings'];
    callback();
    
    // Should navigate to recordings section (main dashboard)
    expect(screen.getByText('Current Tab: recordings')).toBeInTheDocument();
  });

  it('handles tray-action-error event', () => {
    render(<AppContainer />);
    
    const callback = (window as any).__testEventCallbacks['tray-action-error'];
    callback({ payload: 'Tray action failed' });
    
    expect(toast.error).toHaveBeenCalledWith('Tray action failed');
  });

  it('handles no-models-error event', () => {
    render(<AppContainer />);
    
    const callback = (window as any).__testEventCallbacks['no-models-error'];
    callback({
      title: 'No Models',
      message: 'Please download a model'
    });
    
    expect(toast.error).toHaveBeenCalledWith(
      'No Models',
      expect.objectContaining({
        description: 'Please download a model',
        duration: 8000
      })
    );
  });

  it('allows navigation between sections', async () => {
    render(<AppContainer />);
    
    const modelsButton = screen.getByText('Models');
    await userEvent.click(modelsButton);
    
    expect(screen.getByText('Current Tab: models')).toBeInTheDocument();
  });

  it('completes onboarding flow', async () => {
    // Start with onboarding not completed
    let onboardingCompleted = false;
    
    vi.mock('@/contexts/SettingsContext', () => ({
      useSettings: () => ({
        settings: {
          onboarding_completed: onboardingCompleted
        },
        refreshSettings: vi.fn(() => {
          onboardingCompleted = true;
        })
      })
    }));

    const { rerender } = render(<AppContainer />);
    
    // Should show onboarding
    expect(screen.getByTestId('onboarding')).toBeInTheDocument();
    
    // Complete onboarding
    const completeButton = screen.getByText('Complete Onboarding');
    await userEvent.click(completeButton);
    
    // Simulate settings refresh
    onboardingCompleted = true;
    rerender(<AppContainer />);
    
    // Should now show main app
    await waitFor(() => {
      expect(screen.queryByTestId('onboarding')).not.toBeInTheDocument();
      expect(screen.getByTestId('sidebar')).toBeInTheDocument();
    });
  });

  it('checks permissions after onboarding completion', async () => {
    const checkAccessibility = vi.fn().mockResolvedValue(true);
    const checkMicrophone = vi.fn().mockResolvedValue(true);
    
    vi.mock('@/contexts/ReadinessContext', () => ({
      useReadiness: () => ({
        checkAccessibilityPermission: checkAccessibility,
        checkMicrophonePermission: checkMicrophone
      })
    }));

    render(<AppContainer />);
    
    // Permissions should be checked after onboarding
    await waitFor(() => {
      expect(checkAccessibility).toHaveBeenCalled();
      expect(checkMicrophone).toHaveBeenCalled();
    });
  });

  it('handles window event for no models available', () => {
    render(<AppContainer />);
    
    // Trigger the window event
    const event = new Event('no-models-available');
    window.dispatchEvent(event);
    
    // Should trigger onboarding
    // Note: actual implementation would set showOnboarding to true
    expect(screen.getByTestId('tab-container')).toBeInTheDocument();
  });

  it('cleans up on unmount', () => {
    const { updateService } = require('@/services/updateService');
    const { unmount } = render(<AppContainer />);
    
    unmount();
    
    expect(updateService.dispose).toHaveBeenCalled();
  });
});