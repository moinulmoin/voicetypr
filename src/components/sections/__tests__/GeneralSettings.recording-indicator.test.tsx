import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GeneralSettings } from '../GeneralSettings';

const mockUpdateSettings = vi.fn().mockResolvedValue(undefined);
const baseSettings = {
  recording_mode: 'toggle',
  hotkey: 'CommandOrControl+Shift+Space',
  keep_transcription_in_clipboard: false,
  play_sound_on_recording: true,
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
  Select: ({ children }: { children: any }) => <div>{children}</div>,
  SelectTrigger: ({ children }: { children: any }) => <div>{children}</div>,
  SelectContent: ({ children }: { children: any }) => <div>{children}</div>,
  SelectItem: ({ children }: { children: any }) => <div>{children}</div>,
  SelectValue: () => <div />
}));

vi.mock('@/components/MicrophoneSelection', () => ({
  MicrophoneSelection: () => <div data-testid="microphone-selection" />
}));

vi.mock('../NetworkSharingCard', () => ({
  NetworkSharingCard: () => <div data-testid="network-sharing-card" />
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
});
