import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { TabContainer } from './TabContainer';

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

// Mock event coordinator hook
vi.mock('@/hooks/useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn(),
    unregisterEvent: vi.fn(),
  }),
}));

// Mock all tab components with simple test versions
vi.mock('./RecordingsTab', () => ({
  RecordingsTab: () => <div data-testid="recordings-tab">Recordings</div>
}));

vi.mock('./OverviewTab', () => ({
  OverviewTab: () => <div data-testid="overview-tab">Overview</div>
}));

vi.mock('./ModelsTab', () => ({
  ModelsTab: () => <div data-testid="models-tab">Models</div>
}));

vi.mock('./SettingsTab', () => ({
  SettingsTab: () => <div data-testid="settings-tab">Settings</div>
}));

vi.mock('./EnhancementsTab', () => ({
  EnhancementsTab: () => <div data-testid="enhancements-tab">Enhancements</div>
}));

vi.mock('./AdvancedTab', () => ({
  AdvancedTab: () => <div data-testid="advanced-tab">Advanced</div>
}));

vi.mock('./AccountTab', () => ({
  AccountTab: () => <div data-testid="account-tab">Account</div>
}));

vi.mock('./AboutTab', () => ({
  AboutTab: () => <div data-testid="about-tab">About</div>
}));

vi.mock('./HelpTab', () => ({
  HelpTab: () => <div data-testid="help-tab">Help</div>
}));

describe('TabContainer', () => {
  it('renders correct tab based on activeSection', () => {
    const { rerender } = render(<TabContainer activeSection="overview" />);
    expect(screen.getByTestId('overview-tab')).toBeInTheDocument();
    
    rerender(<TabContainer activeSection="recordings" />);
    expect(screen.getByTestId('recordings-tab')).toBeInTheDocument();
    
    rerender(<TabContainer activeSection="models" />);
    expect(screen.getByTestId('models-tab')).toBeInTheDocument();
    
    rerender(<TabContainer activeSection="general" />);
    expect(screen.getByTestId('settings-tab')).toBeInTheDocument();
    
    rerender(<TabContainer activeSection="formatting" />);
    expect(screen.getByTestId('enhancements-tab')).toBeInTheDocument();
    
    rerender(<TabContainer activeSection="advanced" />);
    expect(screen.getByTestId('advanced-tab')).toBeInTheDocument();
    
    rerender(<TabContainer activeSection="license" />);
    expect(screen.getByTestId('account-tab')).toBeInTheDocument();
    
    rerender(<TabContainer activeSection="about" />);
    expect(screen.getByTestId('about-tab')).toBeInTheDocument();
    
    rerender(<TabContainer activeSection="help" />);
    expect(screen.getByTestId('help-tab')).toBeInTheDocument();
  });

  it('renders overview tab for unknown sections', () => {
    render(<TabContainer activeSection="unknown" />);
    expect(screen.getByTestId('overview-tab')).toBeInTheDocument();
  });
});