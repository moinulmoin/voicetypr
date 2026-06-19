import { render, screen, waitFor, act, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { AppContainer } from './AppContainer';

const {
  toastErrorMock,
  sendNotificationMock,
  isPermissionGrantedMock,
  requestPermissionMock,
  checkAccessibilityPermissionMock,
  checkMicrophonePermissionMock,
  requestNotificationPermissionServiceMock,
  refreshSettingsMock,
  mockGetJustUpdatedVersion,
  mockInvoke,
  tauriEventListeners,
} = vi.hoisted(() => ({
  toastErrorMock: vi.fn(),
  sendNotificationMock: vi.fn(),
  isPermissionGrantedMock: vi.fn(),
  requestPermissionMock: vi.fn(),
  checkAccessibilityPermissionMock: vi.fn(),
  checkMicrophonePermissionMock: vi.fn(),
  requestNotificationPermissionServiceMock: vi.fn(),
  refreshSettingsMock: vi.fn(),
  mockGetJustUpdatedVersion: vi.fn(),
  mockInvoke: vi.fn(),
  tauriEventListeners: new Map<string, Set<(event: { payload: unknown }) => void>>(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock('sonner', () => ({
  toast: {
    error: toastErrorMock,
  },
}));

vi.mock('@tauri-apps/plugin-notification', () => ({
  sendNotification: sendNotificationMock,
  isPermissionGranted: isPermissionGrantedMock,
  requestPermission: requestPermissionMock,
}));

// Mock contexts with simple defaults
const mockSettings = {
  onboarding_completed: true,
  transcription_cleanup_days: 30,
  hotkey: 'Cmd+Shift+Space',
};

const mockModelAvailability = {
  hasModels: true as boolean | null,
  selectedModelAvailable: true as boolean | null,
  remoteSelected: false as boolean,
  remoteAvailable: false as boolean,
  remoteStatus: 'unknown' as 'unknown' | 'online' | 'offline' | 'auth_failed' | 'self_connection',
  remoteLastChecked: null as string | number | null,
  isChecking: false as boolean,
  checkModels: vi.fn(),
};

vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: mockSettings,
    refreshSettings: refreshSettingsMock,
  }),
}));

vi.mock('@/contexts/LicenseContext', () => ({
  useLicense: () => ({
    licenseStatus: {
      is_licensed: false,
      license_key: null,
      email: null,
    },
    checkLicense: vi.fn(),
  }),
}));

vi.mock('@/contexts/ReadinessContext', () => ({
  useReadiness: () => ({
    checkAccessibilityPermission: checkAccessibilityPermissionMock,
    checkMicrophonePermission: checkMicrophonePermissionMock,
  }),
}));

// Mock ModelManagementContext that AppContainer actually uses
vi.mock('@/contexts/ModelManagementContext', () => ({
  useModelManagementContext: () => ({
    models: {},
    downloadProgress: {},
    verifyingModels: new Set(),
    downloadModel: vi.fn(),
    retryDownload: vi.fn(),
    cancelDownload: vi.fn(),
    deleteModel: vi.fn(),
    refreshModels: vi.fn(),
    preloadModel: vi.fn(),
    verifyModel: vi.fn(),
  }),
}));

// Mock ModelAvailabilityContext — AppContainer now reads model availability from context
vi.mock('@/contexts/ModelAvailabilityContext', () => ({
  useModelAvailabilityContext: () => ({ ...mockModelAvailability }),
}));

// Mock services
vi.mock('@/services/updateService', () => ({
  updateService: {
    initialize: vi.fn().mockResolvedValue(true),
    dispose: vi.fn(),
    getJustUpdatedVersion: () => mockGetJustUpdatedVersion(),
    requestNotificationPermission: requestNotificationPermissionServiceMock,
    checkForUpdatesManually: vi.fn(),
  },
}));

vi.mock('@/utils/keyring', () => ({
  loadApiKeysToCache: vi.fn().mockResolvedValue(true),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((eventName: string, callback: (event: { payload: unknown }) => void) => {
    const listeners = tauriEventListeners.get(eventName) ?? new Set<(event: { payload: unknown }) => void>();
    listeners.add(callback);
    tauriEventListeners.set(eventName, listeners);

    return Promise.resolve(() => {
      listeners.delete(callback);
    });
  }),
}));

vi.mock('@/components/onboarding/OnboardingDesktop', () => ({
  OnboardingDesktop: ({ onCompletionStart, onCompletionError, onComplete }: any) => {
    (window as any).__testOnboardingStart = onCompletionStart;
    (window as any).__testOnboardingError = onCompletionError;
    (window as any).__testOnboardingComplete = onComplete;
    return (
      <button data-testid="onboarding">
        Onboarding
      </button>
    );
  },
}));

vi.mock('@/components/ui/sidebar', () => ({
  Sidebar: ({ children, onSectionChange }: any) => (
    <div data-testid="sidebar">
      <button onClick={() => onSectionChange('models')}>Models</button>
      {children}
    </div>
  ),
  SidebarProvider: ({ children }: any) => <div>{children}</div>,
  SidebarInset: ({ children }: any) => <div>{children}</div>,
  SidebarContent: ({ children }: any) => <div>{children}</div>,
  SidebarGroup: ({ children }: any) => <div>{children}</div>,
  SidebarGroupLabel: ({ children }: any) => <div>{children}</div>,
  SidebarGroupContent: ({ children }: any) => <div>{children}</div>,
  SidebarHeader: ({ children }: any) => <div>{children}</div>,
  SidebarMenu: ({ children }: any) => <div>{children}</div>,
  SidebarMenuItem: ({ children }: any) => <div>{children}</div>,
  SidebarMenuButton: ({ children }: any) => <button>{children}</button>,
  SidebarTrigger: ({ children }: any) => <button>{children}</button>,
  SidebarFooter: ({ children }: any) => <div>{children}</div>,
  useSidebar: () => ({ isOpen: true, toggle: vi.fn() }),
}));

vi.mock('./tabs/TabContainer', () => ({
  TabContainer: ({ activeSection }: any) => (
    <div data-testid="tab-container">
      Current Tab: {activeSection}
    </div>
  ),
}));

// Mock event coordinator
vi.mock('@/hooks/useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn((event: string, callback: any) => {
      (window as any).__testEventCallbacks = (window as any).__testEventCallbacks || {};
      (window as any).__testEventCallbacks[event] = callback;
      return vi.fn();
    }),
  }),
}));

const getRemoteServerErrorHandler = async () => {
  await waitFor(() => {
    expect((window as any).__testEventCallbacks?.['remote-server-error']).toBeInstanceOf(Function);
  });

  return (window as any).__testEventCallbacks['remote-server-error'] as (payload: any) => Promise<void>;
};


describe('AppContainer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    tauriEventListeners.clear();
    (window as any).__testEventCallbacks = {};
    mockSettings.onboarding_completed = true;
    mockModelAvailability.hasModels = true;
    mockModelAvailability.selectedModelAvailable = true;
    mockModelAvailability.remoteSelected = false;
    mockModelAvailability.remoteAvailable = false;
    mockModelAvailability.remoteStatus = 'unknown';
    mockModelAvailability.remoteLastChecked = null;
    mockModelAvailability.isChecking = false;
    mockModelAvailability.checkModels = vi.fn().mockResolvedValue({
      hasModels: true,
      selectedModelAvailable: true,
      remoteSelected: false,
      remoteAvailable: false,
      remoteStatus: 'unknown',
      remoteLastChecked: null,
    });
    refreshSettingsMock.mockReset();
    mockGetJustUpdatedVersion.mockReset().mockReturnValue(null);
    checkAccessibilityPermissionMock.mockReset().mockResolvedValue(true);
    checkMicrophonePermissionMock.mockReset().mockResolvedValue(true);
    requestNotificationPermissionServiceMock.mockReset();
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'get_model_status') {
        return Promise.resolve({
          models: [
            { name: 'Local Model', downloaded: true, requires_setup: false },
          ],
        });
      }

      if (command === 'get_recognition_availability_snapshot') {
        return Promise.resolve({
          whisper_available: true,
          parakeet_available: true,
          cloud_selected: false,
          cloud_ready: false,
          remote_selected: false,
          remote_status: 'online',
          remote_available: true,
          remote_last_checked: '2026-03-31T00:00:00Z',
        });
      }

      if (command === 'refresh_active_remote_server_status') {
        return Promise.resolve(null);
      }

      return Promise.resolve(null);
    });
  });

  it('shows main app when onboarding is completed', async () => {
    await act(async () => {
      render(<AppContainer />);
    });
    expect(screen.getByTestId('sidebar')).toBeInTheDocument();
    expect(screen.getByTestId('tab-container')).toBeInTheDocument();
  });

  it('shows onboarding when not completed and no remote server is active', async () => {
    mockSettings.onboarding_completed = false;
    render(<AppContainer />);

    await waitFor(() => {
      expect(screen.getByTestId('onboarding')).toBeInTheDocument();
    });

    expect(screen.queryByTestId('sidebar')).not.toBeInTheDocument();
  });

  it('keeps onboarding visible when a remote server is selected but unavailable', async () => {
    mockSettings.onboarding_completed = false;
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'get_model_status') {
        return Promise.resolve({ models: [] });
      }

      if (command === 'get_recognition_availability_snapshot') {
        return Promise.resolve({
          whisper_available: false,
          parakeet_available: false,
          cloud_selected: false,
          cloud_ready: false,
          remote_selected: true,
          remote_status: 'offline',
          remote_available: false,
          remote_last_checked: '2026-03-31T00:00:00Z',
        });
      }

      return Promise.resolve(null);
    });

    render(<AppContainer />);

    await waitFor(() => {
      expect(screen.getByTestId('onboarding')).toBeInTheDocument();
    });

    expect(screen.queryByTestId('sidebar')).not.toBeInTheDocument();
  });

  it('shows onboarding when setup is explicitly reset even if sources are available', async () => {
    mockSettings.onboarding_completed = false;
    render(<AppContainer />);

    await waitFor(() => {
      expect(screen.getByTestId('onboarding')).toBeInTheDocument();
    });

    expect(screen.queryByTestId('sidebar')).not.toBeInTheDocument();
  });

  it('runs post-onboarding side effects after real completion', async () => {
    mockSettings.onboarding_completed = false;
    refreshSettingsMock.mockImplementation(() => {
      mockSettings.onboarding_completed = true;
    });
    const { rerender } = render(<AppContainer />);

    await waitFor(() => {
      expect(screen.getByTestId('onboarding')).toBeInTheDocument();
    });
    await act(async () => {
      mockSettings.onboarding_completed = true;
      rerender(<AppContainer />);
      (window as any).__testOnboardingComplete();
      rerender(<AppContainer />);
    });

    await waitFor(() => {
      expect(refreshSettingsMock).toHaveBeenCalledTimes(1);
      expect(checkAccessibilityPermissionMock).toHaveBeenCalledTimes(1);
      expect(checkMicrophonePermissionMock).toHaveBeenCalledTimes(1);
      expect(requestNotificationPermissionServiceMock).toHaveBeenCalledTimes(1);
    });
  });

  it('does not run post-onboarding side effects when completion save fails', async () => {
    mockSettings.onboarding_completed = false;
    const { rerender } = render(<AppContainer />);

    await waitFor(() => {
      expect(screen.getByTestId('onboarding')).toBeInTheDocument();
    });

    await act(async () => {
      mockSettings.onboarding_completed = true;
      rerender(<AppContainer />);
      (window as any).__testOnboardingError?.();
      rerender(<AppContainer />);
    });

    await waitFor(() => {
      expect(checkAccessibilityPermissionMock).not.toHaveBeenCalled();
      expect(checkMicrophonePermissionMock).not.toHaveBeenCalled();
      expect(requestNotificationPermissionServiceMock).not.toHaveBeenCalled();
    });
  });

  it('keeps the dashboard visible when a stale no-models error is disproven by a fresh check', async () => {
    render(<AppContainer />);

    await waitFor(() => {
      expect((window as any).__testEventCallbacks?.['no-models-error']).toBeInstanceOf(Function);
      expect(screen.getByTestId('sidebar')).toBeInTheDocument();
    });

    const callback = (window as any).__testEventCallbacks['no-models-error'];
    await act(async () => {
      await callback({
        title: 'No models available',
        message: 'Connect a cloud provider or download a local model in Models before recording.',
      });
    });

    await waitFor(() => {
      expect(screen.getByTestId('sidebar')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('onboarding')).not.toBeInTheDocument();
  });

  it('clears onboarding when a remote server recovers after a no-models error', async () => {
    // Start with remote selected but status unknown (hasModels: null — no local models, remote pending)
    mockModelAvailability.hasModels = null;
    mockModelAvailability.selectedModelAvailable = null;
    mockModelAvailability.remoteSelected = true;
    mockModelAvailability.remoteStatus = 'unknown';
    mockModelAvailability.checkModels = vi.fn().mockResolvedValue({
      hasModels: null,
      selectedModelAvailable: null,
      remoteSelected: true,
      remoteAvailable: false,
      remoteStatus: 'unknown',
      remoteLastChecked: null,
    });

    const { rerender } = render(<AppContainer />);

    await waitFor(() => {
      expect((window as any).__testEventCallbacks?.['no-models-error']).toBeInstanceOf(Function);
    });

    const callback = (window as any).__testEventCallbacks['no-models-error'];
    await act(async () => {
      await callback({
        title: 'No models available',
        message: 'Connect a cloud provider or download a local model in Models before recording.',
      });
    });

    await waitFor(() => {
      expect(screen.getByTestId('onboarding')).toBeInTheDocument();
    });

    // Simulate the context updating when the remote server comes online (provider responsibility)
    mockModelAvailability.hasModels = true;
    mockModelAvailability.selectedModelAvailable = true;
    mockModelAvailability.remoteAvailable = true;
    mockModelAvailability.remoteStatus = 'online';
    await act(async () => {
      rerender(<AppContainer />);
    });

    await waitFor(() => {
      expect(screen.getByTestId('sidebar')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('onboarding')).not.toBeInTheDocument();

    expect(checkAccessibilityPermissionMock).not.toHaveBeenCalled();
    expect(checkMicrophonePermissionMock).not.toHaveBeenCalled();
    expect(requestNotificationPermissionServiceMock).not.toHaveBeenCalled();
  });

  it('shows History guidance for retryable remote errors', async () => {
    isPermissionGrantedMock.mockResolvedValue(false);
    requestPermissionMock.mockResolvedValue('granted');

    render(<AppContainer />);

    const handler = await getRemoteServerErrorHandler();
    await handler({
      title: 'Remote recording failed',
      message: 'The remote server could not complete this recording.',
      error_kind: 'recording_failed',
      can_retry_from_history: true,
    });

    await waitFor(() => {
      expect(toastErrorMock).toHaveBeenCalledWith(
        'Remote recording failed',
        expect.objectContaining({
          description: expect.stringContaining('History'),
        })
      );
      expect(requestPermissionMock).toHaveBeenCalled();
      expect(sendNotificationMock).toHaveBeenCalledWith(
        expect.objectContaining({
          title: 'Remote recording failed',
          body: expect.stringContaining('History'),
        })
      );
    });
  });

  it('does not mention History for non-retryable remote errors', async () => {
    isPermissionGrantedMock.mockResolvedValue(true);

    render(<AppContainer />);

    const handler = await getRemoteServerErrorHandler();
    await handler({
      title: 'Remote recording failed',
      message: 'The remote server could not complete this recording.',
      error_kind: 'recording_failed',
      can_retry_from_history: false,
    });

    await waitFor(() => {
      expect(toastErrorMock).toHaveBeenCalled();
      expect(sendNotificationMock).toHaveBeenCalled();
    });

    const [toastTitle, toastOptions] = toastErrorMock.mock.calls[0];
    const [notificationPayload] = sendNotificationMock.mock.calls[0];

    expect(toastTitle).toBe('Remote recording failed');
    expect(toastOptions.description).toBe('The remote server could not complete this recording.');
    expect(toastOptions.description).not.toContain('History');
    expect(notificationPayload.body).toBe('The remote server could not complete this recording.');
    expect(notificationPayload.body).not.toContain('History');
  });

  it('falls back conservatively for legacy remote errors without retryability', async () => {
    isPermissionGrantedMock.mockResolvedValue(true);

    render(<AppContainer />);

    const handler = await getRemoteServerErrorHandler();
    await handler({
      title: 'Remote recording failed',
      message: 'The remote server could not complete this recording.',
    });

    await waitFor(() => {
      expect(toastErrorMock).toHaveBeenCalled();
      expect(sendNotificationMock).toHaveBeenCalled();
    });

    const [toastTitle, toastOptions] = toastErrorMock.mock.calls[0];
    const [notificationPayload] = sendNotificationMock.mock.calls[0];

    expect(toastTitle).toBe('Remote recording failed');
    expect(toastOptions.description).toBe('The remote server could not complete this recording.');
    expect(toastOptions.description).not.toContain('History');
    expect(notificationPayload.body).toBe('The remote server could not complete this recording.');
    expect(notificationPayload.body).not.toContain('History');
  });

  it('allows navigation between sections', async () => {
    await act(async () => {
      render(<AppContainer />);
    });

    expect(screen.getByTestId('sidebar')).toBeInTheDocument();
    expect(screen.getByTestId('tab-container')).toBeInTheDocument();
  });

  describe('post-update modal', () => {
    it('shows update dialog when app was just updated', async () => {
      mockGetJustUpdatedVersion.mockReturnValue('1.13.0');
      render(<AppContainer />);

      await waitFor(() => {
        expect(screen.getByText('VoiceTypr Updated')).toBeInTheDocument();
      });
      expect(screen.getByText(/Successfully updated to version 1\.13\.0/)).toBeInTheDocument();
    });

    it('does not show update dialog when no update marker exists', async () => {
      mockGetJustUpdatedVersion.mockReturnValue(null);
      render(<AppContainer />);

      await waitFor(() => {
        expect(screen.getByTestId('sidebar')).toBeInTheDocument();
      });
      expect(screen.queryByText('VoiceTypr Updated')).not.toBeInTheDocument();
    });

    it('dismisses update dialog when close button is clicked', async () => {
      mockGetJustUpdatedVersion.mockReturnValue('2.0.0');
      render(<AppContainer />);

      await waitFor(() => {
        expect(screen.getByText('VoiceTypr Updated')).toBeInTheDocument();
      });

      const dismissBtn = screen.getByRole('button', { name: /^dismiss$/i });
      fireEvent.click(dismissBtn);

      await waitFor(() => {
        expect(screen.queryByText('VoiceTypr Updated')).not.toBeInTheDocument();
      });
    });
  });
});
