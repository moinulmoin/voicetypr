import { render, screen, waitFor, fireEvent, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { AppContainer } from './AppContainer';

// Mock contexts with simple defaults
const mockSettings = {
  onboarding_completed: true,
  transcription_cleanup_days: 30,
  hotkey: 'Cmd+Shift+Space'
};

vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: mockSettings,
    refreshSettings: vi.fn()
  })
}));

vi.mock('@/contexts/LicenseContext', () => ({
  useLicense: () => ({
    licenseStatus: {
      is_licensed: false,
      license_key: null,
      email: null
    },
    checkLicense: vi.fn()
  })
}));

vi.mock('@/contexts/ReadinessContext', () => ({
  useReadiness: () => ({
    checkAccessibilityPermission: vi.fn().mockResolvedValue(true),
    checkMicrophonePermission: vi.fn().mockResolvedValue(true)
  })
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
    verifyModel: vi.fn()
  })
}));

// Mock services
const mockGetJustUpdatedVersion = vi.fn().mockReturnValue(null);
vi.mock('@/services/updateService', () => ({
  updateService: {
    initialize: vi.fn().mockResolvedValue(true),
    dispose: vi.fn(),
    getJustUpdatedVersion: () => mockGetJustUpdatedVersion(),
    checkForUpdatesManually: vi.fn().mockResolvedValue(undefined),
    requestNotificationPermission: vi.fn()
  }
}));


// Mock components
vi.mock('@/components/onboarding/OnboardingDesktop', () => ({
  OnboardingDesktop: () => <div data-testid="onboarding">Onboarding</div>
}));

vi.mock('@/components/ui/sidebar', () => ({
  Sidebar: ({ children, collapsible }: any) => (
    <div data-testid="sidebar" data-collapsible={collapsible}>
      {children}
    </div>
  ),
  SidebarProvider: ({ children }: any) => <div>{children}</div>,
  SidebarInset: ({ children }: any) => <div>{children}</div>,
  SidebarContent: ({ children }: any) => <div>{children}</div>,
  SidebarGroup: ({ children }: any) => <div>{children}</div>,
  SidebarGroupContent: ({ children }: any) => <div>{children}</div>,
  SidebarHeader: ({ children }: any) => <div>{children}</div>,
  SidebarMenu: ({ children }: any) => <div>{children}</div>,
  SidebarMenuItem: ({ children }: any) => <div>{children}</div>,
  SidebarMenuButton: ({ children }: any) => <button>{children}</button>,
  SidebarTrigger: ({ children }: any) => <button>{children}</button>,
  SidebarFooter: ({ children }: any) => <div>{children}</div>,
  useSidebar: () => ({ isOpen: true, toggle: vi.fn() })
}));

vi.mock('./tabs/TabContainer', () => ({
  TabContainer: ({ activeSection }: any) => (
    <div data-testid="tab-container">
      Current Tab: {activeSection}
      <input aria-label="Editable model setting" />
    </div>
  )
}));

// Captured mock for registerEvent so we can verify calls and cleanup
const mockRegisterEvent = vi.fn();
const mockUnlisteners: Array<ReturnType<typeof vi.fn>> = [];
vi.mock('@/hooks/useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: mockRegisterEvent
  })
}));

// Mock Tauri invoke for focus_main_window
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined)
}));

describe('AppContainer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSettings.onboarding_completed = true;
    window.sessionStorage.clear();
    mockUnlisteners.length = 0;
    mockRegisterEvent.mockImplementation(() => {
      const unlisten = vi.fn();
      mockUnlisteners.push(unlisten);
      return Promise.resolve(unlisten);
    });
  });

  it('shows main app when onboarding is completed', () => {
    render(<AppContainer />);
    expect(screen.getByTestId('sidebar')).toBeInTheDocument();
    expect(screen.getByTestId('tab-container')).toBeInTheDocument();
  });

  it('shows onboarding when not completed', () => {
    mockSettings.onboarding_completed = false;
    render(<AppContainer />);
    expect(screen.getByTestId('onboarding')).toBeInTheDocument();
    expect(screen.queryByTestId('sidebar')).not.toBeInTheDocument();
  });

  it('renders sidebar as non-collapsible', () => {
    render(<AppContainer />);
    const sidebar = screen.getByTestId('sidebar');
    expect(sidebar).toHaveAttribute('data-collapsible', 'none');
  });

  it('keeps sidebar visible after Cmd+B keyboard shortcut', () => {
    render(<AppContainer />);
    // Simulate Cmd+B being pressed
    fireEvent.keyDown(window, { key: 'b', metaKey: true });
    // Sidebar must still be in the document
    expect(screen.getByTestId('sidebar')).toBeInTheDocument();
  });

  it('navigate-to-settings event navigates to general tab', () => {
    render(<AppContainer />);

    // Find the registerEvent call for navigate-to-settings
    const settingsCall = mockRegisterEvent.mock.calls.find(
      (call: any[]) => call[0] === 'navigate-to-settings'
    );
    expect(settingsCall).toBeDefined();

    // Invoke the handler
    const handler = settingsCall![1];
    act(() => handler());

    // TabContainer should show 'general' tab
    expect(screen.getByTestId('tab-container')).toHaveTextContent('Current Tab: general');
  });

  it('navigate-to-settings does not navigate to overview', () => {
    render(<AppContainer />);

    const settingsCall = mockRegisterEvent.mock.calls.find(
      (call: any[]) => call[0] === 'navigate-to-settings'
    );
    expect(settingsCall).toBeDefined();

    const handler = settingsCall![1];
    act(() => handler());

    expect(screen.getByTestId('tab-container')).not.toHaveTextContent('Current Tab: overview');
  });

  it('restores the active section after a WebView refresh', () => {
    window.sessionStorage.setItem('voicetypr:activeSection', 'models');

    render(<AppContainer />);

    expect(screen.getByTestId('tab-container')).toHaveTextContent('Current Tab: models');
  });

  it('persists section changes, blocks shell refresh, and preserves edit menus', () => {
    render(<AppContainer />);

    const settingsCall = mockRegisterEvent.mock.calls.find(
      (call: any[]) => call[0] === 'navigate-to-settings'
    );
    expect(settingsCall).toBeDefined();

    act(() => settingsCall![1]());
    expect(window.sessionStorage.getItem('voicetypr:activeSection')).toBe('general');

    const appShell = screen.getByTestId('sidebar').parentElement?.parentElement;
    expect(appShell).toBeTruthy();
    expect(fireEvent.contextMenu(appShell!)).toBe(false);
    expect(fireEvent.contextMenu(screen.getByLabelText('Editable model setting'))).toBe(true);
  });

  it('registerEvent is called once per event type on mount', () => {
    render(<AppContainer />);

    // Collect all event names registered
    const eventNames = mockRegisterEvent.mock.calls.map((call: any[]) => call[0]);
    const counts: Record<string, number> = {};
    for (const name of eventNames) {
      counts[name] = (counts[name] || 0) + 1;
    }

    // Each event should be registered exactly once
    for (const [name, count] of Object.entries(counts)) {
      expect(count, `Event '${name}' should be registered once`).toBe(1);
    }
  });

  it('cleans up registered Tauri event listeners on unmount', async () => {
    const { unmount } = render(<AppContainer />);

    await waitFor(() => {
      expect(mockUnlisteners).toHaveLength(mockRegisterEvent.mock.calls.length);
    });

    unmount();

    for (const unlisten of mockUnlisteners) {
      expect(unlisten).toHaveBeenCalledTimes(1);
    }
  });

  it('allows navigation between sections', () => {
    render(<AppContainer />);
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