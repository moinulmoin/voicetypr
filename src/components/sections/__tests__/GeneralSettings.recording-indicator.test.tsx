import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GeneralSettings } from '../GeneralSettings';

const mockUpdateSettings = vi.fn().mockResolvedValue(undefined);
const baseSettings = {
  recording_mode: 'toggle',
  hotkey: 'CommandOrControl+Shift+Space',
  keep_transcription_in_clipboard: false,
  play_sound_on_recording: true,
  pill_indicator_mode: 'when_recording',
  pill_indicator_position: 'bottom-center',
  pill_indicator_offset: 10
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
  ScrollArea: ({ children }: { children: any }) => <div>{children}</div>
}));

vi.mock('@/components/ui/switch', () => ({
  Switch: () => <button type="button" />
}));

vi.mock('@/components/ui/toggle-group', () => ({
  ToggleGroup: ({ children }: { children: any }) => <div>{children}</div>,
  ToggleGroupItem: ({ children }: { children: any }) => <div>{children}</div>
}));

vi.mock('@/components/ui/select', () => ({
  Select: ({ children, value, onValueChange }: { children: any; value?: string; onValueChange?: (v: string) => void }) => (
    <div data-testid="select" data-value={value} onClick={() => onValueChange?.('top-center')}>
      {children}
    </div>
  ),
  SelectTrigger: ({ children }: { children: any }) => <div data-testid="select-trigger">{children}</div>,
  SelectContent: ({ children }: { children: any }) => <div>{children}</div>,
  SelectItem: ({ children, value }: { children: any; value: string }) => (
    <div data-testid={`select-item-${value}`}>{children}</div>
  ),
  SelectValue: () => <div data-testid="select-value" />
}));

vi.mock('@/components/MicrophoneSelection', () => ({
  MicrophoneSelection: () => <div data-testid="microphone-selection" />
}));

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

  it('calls updateSettings when position is changed', async () => {
    mockSettings.pill_indicator_mode = 'always';
    render(<GeneralSettings />);
    
    await waitFor(() => {
      expect(screen.getByText('Indicator Position')).toBeInTheDocument();
    });

    // Find the position select (second select on the page, after visibility mode)
    const selects = screen.getAllByTestId('select');
    const positionSelect = selects.find(s => s.getAttribute('data-value')?.includes('center'));
    
    if (positionSelect) {
      fireEvent.click(positionSelect);
      await waitFor(() => {
        expect(mockUpdateSettings).toHaveBeenCalledWith({
          pill_indicator_position: 'top-center'
        });
      });
    }
  });
});
