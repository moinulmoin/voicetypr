import { render, screen, waitFor } from '@testing-library/react';
import { vi, describe, it, expect, beforeEach } from 'vitest';
import { TabContainer } from './TabContainer';

// Mock all context providers that the tabs need
vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: { hotkey: 'Cmd+Shift+Space', current_model: 'tiny' },
    updateSettings: vi.fn(),
    refreshSettings: vi.fn()
  })
}));

vi.mock('@/contexts/LicenseContext', () => ({
  useLicense: () => ({
    license: null,
    checkLicense: vi.fn()
  })
}));

vi.mock('@/hooks/useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn(() => vi.fn())
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

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve([]))
}));

// Mock the sections that the tabs use
vi.mock('../sections/RecentRecordings', () => ({
  RecentRecordings: () => <div data-testid="recent-recordings">Recent Recordings Section</div>
}));

vi.mock('../sections/ModelsSection', () => ({
  ModelsSection: () => <div data-testid="models-section">Models Section</div>
}));

vi.mock('../sections/GeneralSettings', () => ({
  GeneralSettings: () => <div data-testid="general-settings">General Settings Section</div>
}));

vi.mock('../sections/EnhancementsSection', () => ({
  EnhancementsSection: () => <div data-testid="enhancements-section">Enhancements Section</div>
}));

vi.mock('../sections/AdvancedSection', () => ({
  AdvancedSection: () => <div data-testid="advanced-section">Advanced Section</div>
}));

vi.mock('../sections/AccountSection', () => ({
  AccountSection: () => <div data-testid="account-section">Account Section</div>
}));

describe('TabContainer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders recordings tab by default', async () => {
    render(<TabContainer activeSection="recordings" />);
    
    await waitFor(() => {
      expect(screen.getByTestId('recent-recordings')).toBeInTheDocument();
    });
  });

  it('renders models tab when activeSection is models', async () => {
    render(<TabContainer activeSection="models" />);
    
    await waitFor(() => {
      expect(screen.getByTestId('models-section')).toBeInTheDocument();
    });
  });

  it('renders general settings tab when activeSection is general', async () => {
    render(<TabContainer activeSection="general" />);
    
    await waitFor(() => {
      expect(screen.getByTestId('general-settings')).toBeInTheDocument();
    });
  });

  it('renders enhancements tab when activeSection is enhancements', async () => {
    render(<TabContainer activeSection="enhancements" />);
    
    await waitFor(() => {
      expect(screen.getByTestId('enhancements-section')).toBeInTheDocument();
    });
  });

  it('renders advanced tab when activeSection is advanced', async () => {
    render(<TabContainer activeSection="advanced" />);
    
    await waitFor(() => {
      expect(screen.getByTestId('advanced-section')).toBeInTheDocument();
    });
  });

  it('renders account tab when activeSection is account', async () => {
    render(<TabContainer activeSection="account" />);
    
    await waitFor(() => {
      expect(screen.getByTestId('account-section')).toBeInTheDocument();
    });
  });

  it('renders account tab for about section', async () => {
    render(<TabContainer activeSection="about" />);
    
    await waitFor(() => {
      expect(screen.getByTestId('account-section')).toBeInTheDocument();
    });
  });

  it('renders account tab for license section', async () => {
    render(<TabContainer activeSection="license" />);
    
    await waitFor(() => {
      expect(screen.getByTestId('account-section')).toBeInTheDocument();
    });
  });

  it('defaults to recordings tab for unknown section', async () => {
    render(<TabContainer activeSection="unknown-section" />);
    
    await waitFor(() => {
      expect(screen.getByTestId('recent-recordings')).toBeInTheDocument();
    });
  });

  it('switches between tabs correctly', async () => {
    const { rerender } = render(<TabContainer activeSection="recordings" />);
    
    await waitFor(() => {
      expect(screen.getByTestId('recent-recordings')).toBeInTheDocument();
    });
    
    // Switch to models tab
    rerender(<TabContainer activeSection="models" />);
    
    await waitFor(() => {
      expect(screen.queryByTestId('recent-recordings')).not.toBeInTheDocument();
      expect(screen.getByTestId('models-section')).toBeInTheDocument();
    });
    
    // Switch to settings tab
    rerender(<TabContainer activeSection="general" />);
    
    await waitFor(() => {
      expect(screen.queryByTestId('models-section')).not.toBeInTheDocument();
      expect(screen.getByTestId('general-settings')).toBeInTheDocument();
    });
  });

  it('maintains tab state when switching back', async () => {
    const { rerender } = render(<TabContainer activeSection="recordings" />);
    
    await waitFor(() => {
      expect(screen.getByTestId('recent-recordings')).toBeInTheDocument();
    });
    
    // Switch to models
    rerender(<TabContainer activeSection="models" />);
    await waitFor(() => {
      expect(screen.getByTestId('models-section')).toBeInTheDocument();
    });
    
    // Switch back to recordings
    rerender(<TabContainer activeSection="recordings" />);
    await waitFor(() => {
      expect(screen.getByTestId('recent-recordings')).toBeInTheDocument();
    });
  });
});