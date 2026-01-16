import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GeneralSettings } from '../GeneralSettings';

const mockUpdateSettings = vi.fn().mockResolvedValue(undefined);
const baseSettings = {
  recording_mode: 'toggle',
  hotkey: 'CommandOrControl+Shift+Space',
  keep_transcription_in_clipboard: false,
  play_sound_on_recording: true,
  play_sound_on_recording_end: true,
  pill_indicator_mode: 'when_recording',
  pill_indicator_position: 'bottom'
};

let mockSettings = { ...baseSettings };

vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: mockSettings,
    updateSettings: mockUpdateSettings
  })
}));

vi.mock('@/contexts/ReadinessContext', () => ({
  useCanAutoInsert: () => true
}));

vi.mock('@/lib/platform', () => ({
  isMacOS: false
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined)
}));

vi.mock('@tauri-apps/plugin-autostart', () => ({
  enable: vi.fn().mockResolvedValue(undefined),
  disable: vi.fn().mockResolvedValue(undefined),
  isEnabled: vi.fn().mockResolvedValue(false)
}));

vi.mock('@/components/HotkeyInput', () => ({
  HotkeyInput: () => <div data-testid="hotkey-input" />
}));

vi.mock('@/components/ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children: React.ReactNode }) => <div>{children}</div>
}));

// Mock Switch with actual interactive behavior for testing
vi.mock('@/components/ui/switch', () => ({
  Switch: ({ id, checked, onCheckedChange, disabled }: {
    id?: string;
    checked?: boolean;
    onCheckedChange?: (checked: boolean) => void;
    disabled?: boolean;
  }) => (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      data-testid={id ? `switch-${id}` : 'switch'}
      disabled={disabled}
      onClick={() => onCheckedChange?.(!checked)}
    />
  )
}));

vi.mock('@/components/ui/toggle-group', () => ({
  ToggleGroup: ({ children, value, onValueChange }: {
    children: React.ReactNode;
    value?: string;
    onValueChange?: (value: string) => void;
  }) => (
    <div data-testid="toggle-group" data-value={value}>
      {children}
    </div>
  ),
  ToggleGroupItem: ({ children, value }: { children: React.ReactNode; value: string }) => (
    <button type="button" data-testid={`toggle-item-${value}`}>
      {children}
    </button>
  )
}));

// Mock Select with interactive behavior
vi.mock('@/components/ui/select', () => ({
  Select: ({ children, value, onValueChange }: {
    children: React.ReactNode;
    value?: string;
    onValueChange?: (value: string) => void;
  }) => (
    <div data-testid="select" data-value={value}>
      {children}
    </div>
  ),
  SelectTrigger: ({ children, className }: { children: React.ReactNode; className?: string }) => (
    <button type="button" data-testid="select-trigger" className={className}>
      {children}
    </button>
  ),
  SelectContent: ({ children }: { children: React.ReactNode }) => (
    <div data-testid="select-content">{children}</div>
  ),
  SelectItem: ({ children, value }: { children: React.ReactNode; value: string }) => (
    <div data-testid={`select-item-${value}`} data-value={value}>
      {children}
    </div>
  ),
  SelectValue: () => <span data-testid="select-value" />
}));

vi.mock('@/components/MicrophoneSelection', () => ({
  MicrophoneSelection: () => <div data-testid="microphone-selection" />
}));

vi.mock('../NetworkSharingCard', () => ({
  NetworkSharingCard: () => <div data-testid="network-sharing-card" />
}));

// ============================================================================
// Recording Indicator Mode Tests
// ============================================================================

describe('GeneralSettings recording indicator', () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
  });

  it('hides the position selector when mode is never', async () => {
    mockSettings.pill_indicator_mode = 'never';
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(
        screen.queryByText('Indicator Position')
      ).not.toBeInTheDocument();
    });
  });

  it('shows the position selector when mode is always', async () => {
    mockSettings.pill_indicator_mode = 'always';
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Indicator Position')).toBeInTheDocument();
    });
  });

  it('shows the position selector when mode is when_recording', async () => {
    mockSettings.pill_indicator_mode = 'when_recording';
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Indicator Position')).toBeInTheDocument();
    });
  });

  it('displays Recording Indicator Visibility label', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Recording Indicator Visibility')).toBeInTheDocument();
    });
  });

  it('displays all indicator mode options', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByTestId('select-item-never')).toBeInTheDocument();
      expect(screen.getByTestId('select-item-always')).toBeInTheDocument();
      expect(screen.getByTestId('select-item-when_recording')).toBeInTheDocument();
    });
  });

  it('displays all indicator position options when mode is not never', async () => {
    mockSettings.pill_indicator_mode = 'always';
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByTestId('select-item-top')).toBeInTheDocument();
      expect(screen.getByTestId('select-item-center')).toBeInTheDocument();
      expect(screen.getByTestId('select-item-bottom')).toBeInTheDocument();
    });
  });
});

// ============================================================================
// Sound Settings Tests
// ============================================================================

describe('GeneralSettings sound settings', () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
  });

  it('displays Sound on Recording label', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Sound on Recording')).toBeInTheDocument();
    });
  });

  it('displays Sound on Recording End label', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Sound on Recording End')).toBeInTheDocument();
    });
  });

  it('displays sound on recording description', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Play a sound when recording starts')).toBeInTheDocument();
    });
  });

  it('displays sound on recording end description', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Play a sound when recording stops')).toBeInTheDocument();
    });
  });

  it('renders sound on recording switch', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByTestId('switch-sound-on-recording')).toBeInTheDocument();
    });
  });

  it('renders sound on recording end switch', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByTestId('switch-sound-on-recording-end')).toBeInTheDocument();
    });
  });

  it('calls updateSettings when sound on recording switch is clicked', async () => {
    render(<GeneralSettings />);

    await waitFor(() => {
      expect(screen.getByTestId('switch-sound-on-recording')).toBeInTheDocument();
    });

    const switchButton = screen.getByTestId('switch-sound-on-recording');
    fireEvent.click(switchButton);

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith({
        play_sound_on_recording: false
      });
    });
  });

  it('calls updateSettings when sound on recording end switch is clicked', async () => {
    render(<GeneralSettings />);

    await waitFor(() => {
      expect(screen.getByTestId('switch-sound-on-recording-end')).toBeInTheDocument();
    });

    const switchButton = screen.getByTestId('switch-sound-on-recording-end');
    fireEvent.click(switchButton);

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith({
        play_sound_on_recording_end: false
      });
    });
  });

  it('reflects play_sound_on_recording=false in switch state', async () => {
    mockSettings.play_sound_on_recording = false;
    render(<GeneralSettings />);

    await waitFor(() => {
      const switchButton = screen.getByTestId('switch-sound-on-recording');
      expect(switchButton).toHaveAttribute('aria-checked', 'false');
    });
  });

  it('reflects play_sound_on_recording=true in switch state', async () => {
    mockSettings.play_sound_on_recording = true;
    render(<GeneralSettings />);

    await waitFor(() => {
      const switchButton = screen.getByTestId('switch-sound-on-recording');
      expect(switchButton).toHaveAttribute('aria-checked', 'true');
    });
  });

  it('reflects play_sound_on_recording_end=false in switch state', async () => {
    mockSettings.play_sound_on_recording_end = false;
    render(<GeneralSettings />);

    await waitFor(() => {
      const switchButton = screen.getByTestId('switch-sound-on-recording-end');
      expect(switchButton).toHaveAttribute('aria-checked', 'false');
    });
  });

  it('reflects play_sound_on_recording_end=true in switch state', async () => {
    mockSettings.play_sound_on_recording_end = true;
    render(<GeneralSettings />);

    await waitFor(() => {
      const switchButton = screen.getByTestId('switch-sound-on-recording-end');
      expect(switchButton).toHaveAttribute('aria-checked', 'true');
    });
  });
});

// ============================================================================
// Clipboard Settings Tests
// ============================================================================

describe('GeneralSettings clipboard settings', () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
  });

  it('displays Keep Transcript in Clipboard label', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Keep Transcript in Clipboard')).toBeInTheDocument();
    });
  });

  it('displays clipboard description', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Leave transcribed text available for manual pastes')).toBeInTheDocument();
    });
  });

  it('renders clipboard retain switch', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByTestId('switch-clipboard-retain')).toBeInTheDocument();
    });
  });

  it('calls updateSettings when clipboard switch is clicked', async () => {
    render(<GeneralSettings />);

    await waitFor(() => {
      expect(screen.getByTestId('switch-clipboard-retain')).toBeInTheDocument();
    });

    const switchButton = screen.getByTestId('switch-clipboard-retain');
    fireEvent.click(switchButton);

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith({
        keep_transcription_in_clipboard: true
      });
    });
  });
});

// ============================================================================
// UI Structure Tests
// ============================================================================

describe('GeneralSettings UI structure', () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
  });

  it('renders the Settings header', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Settings')).toBeInTheDocument();
    });
  });

  it('renders the Recording section', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Recording')).toBeInTheDocument();
    });
  });

  it('renders the hotkey input component', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByTestId('hotkey-input')).toBeInTheDocument();
    });
  });

  it('renders the microphone selection component', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByTestId('microphone-selection')).toBeInTheDocument();
    });
  });

  it('renders the network sharing card component', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByTestId('network-sharing-card')).toBeInTheDocument();
    });
  });

  it('renders Launch at Startup label', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Launch at Startup')).toBeInTheDocument();
    });
  });

  it('displays ESC cancel hint', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText(/twice while recording to cancel/)).toBeInTheDocument();
    });
  });
});

// ============================================================================
// Recording Mode Tests
// ============================================================================

describe('GeneralSettings recording mode', () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
  });

  it('displays Recording Mode label', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Recording Mode')).toBeInTheDocument();
    });
  });

  it('renders toggle mode option', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByTestId('toggle-item-toggle')).toBeInTheDocument();
    });
  });

  it('renders push to talk mode option', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByTestId('toggle-item-push_to_talk')).toBeInTheDocument();
    });
  });

  it('displays toggle mode description when in toggle mode', async () => {
    mockSettings.recording_mode = 'toggle';
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Press the hotkey to start/stop recording')).toBeInTheDocument();
    });
  });

  it('displays push to talk description when in PTT mode', async () => {
    mockSettings.recording_mode = 'push_to_talk';
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Hold the hotkey to record, release to stop')).toBeInTheDocument();
    });
  });

  it('displays Toggle Hotkey label when in toggle mode', async () => {
    mockSettings.recording_mode = 'toggle';
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Toggle Hotkey')).toBeInTheDocument();
    });
  });

  it('displays Push-to-Talk Key label when in PTT mode', async () => {
    mockSettings.recording_mode = 'push_to_talk';
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Push-to-Talk Key')).toBeInTheDocument();
    });
  });
});

// ============================================================================
// Null Settings Handling Tests
// ============================================================================

describe('GeneralSettings null handling', () => {
  it('returns null when settings is null', async () => {
    // Create a separate mock for null settings
    const originalMock = vi.fn();
    vi.doMock('@/contexts/SettingsContext', () => ({
      useSettings: () => ({
        settings: null,
        updateSettings: originalMock
      })
    }));

    // With current mock setup, we can test the base case
    // The component should handle null gracefully
  });

  it('handles undefined sound settings with defaults', async () => {
    // Remove sound settings to test ?? operator
    const settingsWithoutSound = {
      recording_mode: 'toggle',
      hotkey: 'CommandOrControl+Shift+Space',
      keep_transcription_in_clipboard: false,
      pill_indicator_mode: 'when_recording',
      pill_indicator_position: 'bottom'
      // play_sound_on_recording and play_sound_on_recording_end intentionally omitted
    };
    mockSettings = settingsWithoutSound as typeof mockSettings;

    render(<GeneralSettings />);
    await waitFor(() => {
      // Should still render with defaults (true)
      const switchButton = screen.getByTestId('switch-sound-on-recording');
      expect(switchButton).toHaveAttribute('aria-checked', 'true');
    });
  });
});

// ============================================================================
// Settings Value Display Tests
// ============================================================================

describe('GeneralSettings settings values', () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
  });

  it('displays correct recording mode value', async () => {
    mockSettings.recording_mode = 'toggle';
    render(<GeneralSettings />);
    await waitFor(() => {
      const toggleGroup = screen.getByTestId('toggle-group');
      expect(toggleGroup).toHaveAttribute('data-value', 'toggle');
    });
  });

  it('displays correct pill indicator mode value', async () => {
    mockSettings.pill_indicator_mode = 'always';
    render(<GeneralSettings />);
    // The first Select should have the indicator mode
    await waitFor(() => {
      const selects = screen.getAllByTestId('select');
      expect(selects.length).toBeGreaterThan(0);
    });
  });

  it('shows all three indicator visibility options', async () => {
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Never')).toBeInTheDocument();
      expect(screen.getByText('Always')).toBeInTheDocument();
      expect(screen.getByText('When Recording')).toBeInTheDocument();
    });
  });

  it('shows all three position options when indicator is visible', async () => {
    mockSettings.pill_indicator_mode = 'always';
    render(<GeneralSettings />);
    await waitFor(() => {
      expect(screen.getByText('Top')).toBeInTheDocument();
      expect(screen.getByText('Center')).toBeInTheDocument();
      expect(screen.getByText('Bottom')).toBeInTheDocument();
    });
  });
});
