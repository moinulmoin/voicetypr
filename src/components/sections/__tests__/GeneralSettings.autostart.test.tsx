import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GeneralSettings } from '../GeneralSettings';

const mockUpdateSettings = vi.fn().mockResolvedValue(undefined);
const mockInvoke = vi.fn().mockResolvedValue(false);
const baseSettings = {
  recording_mode: 'toggle',
  hotkey: 'CommandOrControl+Shift+Space',
  keep_transcription_in_clipboard: false,
  play_sound_on_recording: true,
  pill_indicator_mode: 'when_recording',
  pill_indicator_position: 'bottom-center',
  pill_indicator_offset: 10,
  transcription_acceleration: 'auto',
};

let mockSettings = { ...baseSettings };
const accelerationStatus = {
  mode: 'auto',
  effective_backend: 'unknown',
  gpu_available: null,
  message: 'GPU acceleration has not been tested yet.',
  diagnostic_code: 'not_tested',
  recommended_action: 'none',
  last_error: null,
};


vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: mockSettings,
    updateSettings: mockUpdateSettings,
  }),
}));

vi.mock('@/contexts/ReadinessContext', () => ({
  useCanAutoInsert: () => true,
}));

vi.mock('@/lib/platform', () => ({
  isMacOS: false,
  isWindows: false,
}));

// Mock invoke — the new backend commands replace the autostart plugin
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// The autostart plugin should NO LONGER be imported by the component.
// We still mock it here so that if it is accidentally imported,
// the import won't crash — but our tests assert it is never called.
vi.mock('@tauri-apps/plugin-autostart', () => ({
  enable: vi.fn(),
  disable: vi.fn(),
  isEnabled: vi.fn(),
}));

vi.mock('@/components/HotkeyInput', () => ({
  HotkeyInput: () => <div data-testid="hotkey-input" />,
}));

vi.mock('@/components/ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

vi.mock('@/components/ui/switch', () => ({
  Switch: ({
    checked,
    onCheckedChange,
    disabled,
    id,
  }: {
    checked?: boolean;
    onCheckedChange?: (v: boolean) => void;
    disabled?: boolean;
    id?: string;
  }) => (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={id}
      data-testid={id}
      data-disabled={disabled}
      onClick={() => onCheckedChange?.(!checked)}
    />
  ),
}));

vi.mock('@/components/ui/toggle-group', () => ({
  ToggleGroup: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  ToggleGroupItem: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

vi.mock('@/components/ui/select', () => ({
  Select: ({
    children,
    value,
    onValueChange,
  }: {
    children: ReactNode;
    value?: string;
    onValueChange?: (v: string) => void;
  }) => (
    <div
      data-testid="select"
      data-value={value}
      onClick={() => onValueChange?.('top-center')}
    >
      {children}
    </div>
  ),
  SelectTrigger: ({ children }: { children: ReactNode }) => (
    <div data-testid="select-trigger">{children}</div>
  ),
  SelectContent: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  SelectItem: ({
    children,
    value,
  }: {
    children: ReactNode;
    value: string;
  }) => <div data-testid={`select-item-${value}`}>{children}</div>,
  SelectValue: () => <div data-testid="select-value" />,
}));

vi.mock('@/components/MicrophoneSelection', () => ({
  MicrophoneSelection: () => <div data-testid="microphone-selection" />,
}));

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
    info: vi.fn(),
  },
}));

describe('GeneralSettings autostart via backend commands', () => {
  beforeEach(() => {
    mockSettings = { ...baseSettings };
    vi.clearAllMocks();
    mockInvoke.mockImplementation((command: string, args?: { enabled?: boolean }) => {
      if (command === 'get_autostart_status') return Promise.resolve(false);
      if (command === 'set_autostart') return Promise.resolve(args?.enabled ?? false);
      if (command === 'get_transcription_acceleration_status') {
        return Promise.resolve(accelerationStatus);
      }
      if (command === 'test_transcription_acceleration') {
        return Promise.resolve({
          ...accelerationStatus,
          diagnostic_code: 'ready',
          recommended_action: 'none',
          gpu_available: true,
        });
      }
      return Promise.resolve(false);
    });
  });

  it('calls get_autostart_status on mount and renders switch off', async () => {

    render(<GeneralSettings />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('get_autostart_status');
    });

    const autostartSwitch = screen.getByRole('switch', { name: /autostart/i });
    expect(autostartSwitch).toHaveAttribute('aria-checked', 'false');
  });

  it('renders switch on when backend reports autostart enabled', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'get_autostart_status') return Promise.resolve(true);
      if (command === 'get_transcription_acceleration_status') {
        return Promise.resolve(accelerationStatus);
      }
      return Promise.resolve(false);
    });

    render(<GeneralSettings />);

    await waitFor(() => {
      const autostartSwitch = screen.getByRole('switch', { name: /autostart/i });
      expect(autostartSwitch).toHaveAttribute('aria-checked', 'true');
    });
  });

  it('calls set_autostart when toggled and updates UI from response', async () => {

    render(<GeneralSettings />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('get_autostart_status');
    });

    const autostartSwitch = screen.getByRole('switch', { name: /autostart/i });
    fireEvent.click(autostartSwitch);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('set_autostart', {
        enabled: true,
      });
    });

    // UI should reflect the backend response (true)
    await waitFor(() => {
      expect(
        screen.getByRole('switch', { name: /autostart/i }),
      ).toHaveAttribute('aria-checked', 'true');
    });

    // Settings should be updated with the actual backend state
    expect(mockUpdateSettings).toHaveBeenCalledWith({
      launch_at_startup: true,
    });
  });

  it('shows correct UI when backend returns different state than requested', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'get_autostart_status') return Promise.resolve(false);
      if (command === 'set_autostart') return Promise.resolve(false);
      if (command === 'get_transcription_acceleration_status') {
        return Promise.resolve(accelerationStatus);
      }
      return Promise.resolve(false);
    });

    render(<GeneralSettings />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('get_autostart_status');
    });

    const autostartSwitch = screen.getByRole('switch', { name: /autostart/i });
    fireEvent.click(autostartSwitch);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('set_autostart', {
        enabled: true,
      });
    });

    // UI should show the actual backend state (false), not the requested state (true)
    await waitFor(() => {
      expect(
        screen.getByRole('switch', { name: /autostart/i }),
      ).toHaveAttribute('aria-checked', 'false');
    });

    // Settings should reflect actual state
    expect(mockUpdateSettings).toHaveBeenCalledWith({
      launch_at_startup: false,
    });
  });

  it('does not call autostart plugin directly', async () => {
    // Import the mocked plugin to verify it was never called
    const autostartPlugin = await import('@tauri-apps/plugin-autostart');

    render(<GeneralSettings />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('get_autostart_status');
    });

    expect(autostartPlugin.enable).not.toHaveBeenCalled();
    expect(autostartPlugin.disable).not.toHaveBeenCalled();
    expect(autostartPlugin.isEnabled).not.toHaveBeenCalled();
  });

  it('renders actionable GPU driver diagnostics and uses backend failure toast message', async () => {
    const { toast } = await import('sonner');
    const driverFailureStatus = {
      ...accelerationStatus,
      effective_backend: 'cpu',
      gpu_available: false,
      message: 'Vulkan runtime failed to initialize.',
      diagnostic_code: 'driver_or_runtime_failed',
      recommended_action: 'update_graphics_driver',
      last_error: 'The Vulkan loader could not create a device.',
    };

    mockInvoke.mockImplementation((command: string, args?: { enabled?: boolean }) => {
      if (command === 'get_autostart_status') return Promise.resolve(false);
      if (command === 'set_autostart') return Promise.resolve(args?.enabled ?? false);
      if (command === 'get_transcription_acceleration_status') {
        return Promise.resolve(driverFailureStatus);
      }
      if (command === 'test_transcription_acceleration') {
        return Promise.resolve(driverFailureStatus);
      }
      return Promise.resolve(false);
    });

    render(<GeneralSettings />);

    await waitFor(() => {
      expect(
        screen.getByText(/Vulkan-capable NVIDIA, AMD, or Intel graphics driver/),
      ).toBeInTheDocument();
    });
    expect(
      screen.getByText(/Voicetypr will keep using CPU transcription safely/),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /check status/i }));

    await waitFor(() => {
      expect(toast.warning).toHaveBeenCalledWith(
        'Vulkan runtime failed to initialize.',
        {
          description: 'Update or install your graphics driver, then retry Test GPU.',
        },
      );
    });
  });


  it('shows timeout-specific GPU guidance', async () => {
    const timeoutStatus = {
      ...accelerationStatus,
      effective_backend: 'cpu',
      gpu_available: false,
      message: 'The Vulkan helper did not respond in time.',
      diagnostic_code: 'sidecar_timeout',
      recommended_action: 'use_cpu',
      last_error: 'Vulkan sidecar request timed out',
    };

    mockInvoke.mockImplementation((command: string, args?: { enabled?: boolean }) => {
      if (command === 'get_autostart_status') return Promise.resolve(false);
      if (command === 'set_autostart') return Promise.resolve(args?.enabled ?? false);
      if (command === 'get_transcription_acceleration_status') {
        return Promise.resolve(timeoutStatus);
      }
      return Promise.resolve(false);
    });

    render(<GeneralSettings />);

    await waitFor(() => {
      expect(
        screen.getByText(/retry Test GPU after updating your graphics driver/),
      ).toBeInTheDocument();
    });
  });

  it('checks acceleration status through backend command without showing a false GPU warning', async () => {
    const { toast } = await import('sonner');
    mockInvoke.mockImplementation((command: string, args?: { enabled?: boolean }) => {
      if (command === 'get_autostart_status') return Promise.resolve(false);
      if (command === 'set_autostart') return Promise.resolve(args?.enabled ?? false);
      if (command === 'get_transcription_acceleration_status') {
        return Promise.resolve(accelerationStatus);
      }
      if (command === 'test_transcription_acceleration') {
        return Promise.resolve({
          ...accelerationStatus,
          effective_backend: 'metal',
          message: 'GPU controls only apply to Windows Vulkan builds.',
          gpu_available: null,
        });
      }
      return Promise.resolve(false);
    });

    render(<GeneralSettings />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        'get_transcription_acceleration_status',
      );
    });

    fireEvent.click(screen.getByRole('button', { name: /check status/i }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('test_transcription_acceleration');
    });
    expect(toast.info).toHaveBeenCalledWith('GPU controls only apply to Windows Vulkan builds.');
    expect(toast.warning).not.toHaveBeenCalledWith(
      'GPU acceleration is unavailable; CPU mode will be used',
    );
  });
});
