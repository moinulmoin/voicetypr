import { render, screen, waitFor, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { TabContainer } from './TabContainer';

let invokeMock = vi.fn(async (cmd: string, _args?: Record<string, unknown>) => {
  if (cmd === 'get_transcription_history') return [];
  if (cmd === 'get_transcription_count') return 0;
  return null;
});

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invokeMock(cmd, args),
}));

const registerEventMock = vi.fn((event: string, callback: any) => {
  (window as any).__testEventCallbacks = (window as any).__testEventCallbacks || {};
  (window as any).__testEventCallbacks[event] = callback;
  return vi.fn();
});

// Mock event coordinator hook
vi.mock('@/hooks/useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: registerEventMock,
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

const getRegisteredEvent = (eventName: string) => (window as any).__testEventCallbacks?.[eventName];

describe('TabContainer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__testEventCallbacks = {};
    invokeMock = vi.fn(async (cmd: string, _args?: Record<string, unknown>) => {
      if (cmd === 'get_transcription_history') return [];
      if (cmd === 'get_transcription_count') return 0;
      return null;
    });
  });

  it('renders correct tab based on activeSection', async () => {
    const { rerender } = render(<TabContainer activeSection="overview" />);
    await screen.findByTestId('overview-tab');
    
    rerender(<TabContainer activeSection="recordings" />);
    await screen.findByTestId('recordings-tab');
    
    rerender(<TabContainer activeSection="models" />);
    await screen.findByTestId('models-tab');
    
    rerender(<TabContainer activeSection="general" />);
    await screen.findByTestId('settings-tab');
    
    rerender(<TabContainer activeSection="formatting" />);
    await screen.findByTestId('enhancements-tab');
    
    rerender(<TabContainer activeSection="advanced" />);
    await screen.findByTestId('advanced-tab');
    
    rerender(<TabContainer activeSection="license" />);
    await screen.findByTestId('account-tab');
    
    rerender(<TabContainer activeSection="about" />);
    await screen.findByTestId('about-tab');
    
    rerender(<TabContainer activeSection="help" />);
    await screen.findByTestId('help-tab');
  });

  it('renders overview tab for unknown sections', async () => {
    render(<TabContainer activeSection="unknown" />);
    await screen.findByTestId('overview-tab');
    expect(screen.getByTestId('overview-tab')).toBeInTheDocument();
  });

  it('keeps transcription-updated ownership in TabContainer', async () => {
    render(<TabContainer activeSection="overview" />);

    await waitFor(() => {
      expect(getRegisteredEvent('transcription-updated')).toBeInstanceOf(Function);
      expect(getRegisteredEvent('history-updated')).toBeInstanceOf(Function);
    });

    const transcriptionUpdated = getRegisteredEvent('transcription-updated');
    expect(transcriptionUpdated).toBeInstanceOf(Function);

    await act(async () => {
      await transcriptionUpdated();
    });

    await waitFor(() => {
      expect(invokeMock.mock.calls.filter(([cmd]) => cmd === 'get_transcription_history')).toHaveLength(2);
      expect(invokeMock.mock.calls.filter(([cmd]) => cmd === 'get_transcription_count')).toHaveLength(2);
    });
  });
});
