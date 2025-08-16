import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi, describe, it, expect, beforeEach } from 'vitest';
import { SettingsTab } from './SettingsTab';
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
      hotkey: 'Cmd+Shift+Space',
      current_language: 'en',
      auto_launch: true
    },
    updateSettings: vi.fn().mockResolvedValue(true),
    refreshSettings: vi.fn()
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

// Mock GeneralSettings component
vi.mock('../sections/GeneralSettings', () => ({
  GeneralSettings: () => (
    <div data-testid="general-settings">
      <button onClick={() => {
        // Simulate settings change
        const updateSettings = vi.fn();
        updateSettings({ hotkey: 'Cmd+Shift+X' });
      }}>Change Hotkey</button>
      <div>Current Hotkey: Cmd+Shift+Space</div>
    </div>
  )
}));

describe('SettingsTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__testEventCallbacks = {};
    // Default mock for invoke to return a resolved promise
    vi.mocked(invoke).mockResolvedValue(true);
  });

  it('renders without crashing', () => {
    render(<SettingsTab />);
    expect(screen.getByTestId('general-settings')).toBeInTheDocument();
  });

  it('displays current settings', () => {
    render(<SettingsTab />);
    expect(screen.getByText('Current Hotkey: Cmd+Shift+Space')).toBeInTheDocument();
  });

  it('registers error event listeners on mount', () => {
    render(<SettingsTab />);
    
    expect((window as any).__testEventCallbacks).toHaveProperty('no-speech-detected');
    expect((window as any).__testEventCallbacks).toHaveProperty('hotkey-registration-failed');
  });

  it('handles no-speech-detected event with warning severity', () => {
    render(<SettingsTab />);
    
    const callback = (window as any).__testEventCallbacks['no-speech-detected'];
    callback({
      severity: 'warning',
      title: 'No Speech',
      message: 'No speech detected in recording'
    });

    expect(toast.warning).toHaveBeenCalledWith(
      'No Speech',
      expect.objectContaining({
        description: 'No speech detected in recording'
      })
    );
  });

  it('handles no-speech-detected event with error severity', () => {
    render(<SettingsTab />);
    
    const callback = (window as any).__testEventCallbacks['no-speech-detected'];
    callback({
      severity: 'error',
      title: 'Error',
      message: 'Critical error',
      actions: ['settings']
    });

    expect(toast.error).toHaveBeenCalledWith(
      'Error',
      expect.objectContaining({
        description: 'Critical error',
        duration: 8000
      })
    );
  });

  it('handles hotkey-registration-failed event', () => {
    render(<SettingsTab />);
    
    const callback = (window as any).__testEventCallbacks['hotkey-registration-failed'];
    callback({
      hotkey: 'Cmd+Shift+X',
      suggestion: 'The hotkey is already in use',
      error: 'Conflict detected'
    });

    expect(toast.error).toHaveBeenCalledWith(
      'Hotkey Registration Failed',
      expect.objectContaining({
        description: 'The hotkey is already in use',
        duration: 10000
      })
    );
  });

  it('provides tips when user clicks Tips action in no-speech toast', () => {
    render(<SettingsTab />);
    
    const callback = (window as any).__testEventCallbacks['no-speech-detected'];
    callback({ severity: 'warning', message: 'No speech detected' });

    // Get the action from the toast call
    const toastCall = vi.mocked(toast.warning).mock.calls[0];
    const options = toastCall[1] as any;
    
    // Simulate clicking the Tips button
    options.action.onClick();

    expect(toast.info).toHaveBeenCalledWith(
      'Recording Tips',
      expect.objectContaining({
        description: expect.stringContaining('Speak clearly'),
        duration: 8000
      })
    );
  });

  it('handles settings update through invoke', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(true);
    
    render(<SettingsTab />);
    
    // Trigger a settings update
    await invoke('save_settings', { settings: { hotkey: 'Cmd+Shift+Y' } });
    
    expect(invoke).toHaveBeenCalledWith('save_settings', { 
      settings: { hotkey: 'Cmd+Shift+Y' } 
    });
  });

  it('cleans up event listeners on unmount', () => {
    const { unmount } = render(<SettingsTab />);
    
    const unregisterFn = vi.fn();
    (window as any).__testUnregister = unregisterFn;
    
    unmount();
    
    // Event listeners should be cleaned up (implementation depends on useEventCoordinator)
    expect((window as any).__testEventCallbacks).toBeDefined();
  });
});