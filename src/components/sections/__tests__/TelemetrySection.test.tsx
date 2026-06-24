import { render, screen, waitFor, fireEvent, act } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TelemetrySection } from '../TelemetrySection';

const mockInvoke = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock('sonner', () => ({
  toast: {
    info: vi.fn(),
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
  },
}));

describe('TelemetrySection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
  });

  it('renders the consent copy and an unchecked switch when off and available', async () => {
    mockInvoke.mockResolvedValue({ enabled: false, available: true });

    render(<TelemetrySection />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('get_telemetry_status');
    });

    expect(
      await screen.findByText('Help improve Voicetypr with anonymous diagnostics'),
    ).toBeInTheDocument();

    const sw = screen.getByRole('switch');
    expect(sw).toBeInTheDocument();
    expect(sw).not.toBeChecked();
    expect(sw).toBeEnabled();
  });

  it('calls set_telemetry_consent with enabled true when toggled on', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'get_telemetry_status') {
        return Promise.resolve({ enabled: false, available: true });
      }
      if (command === 'set_telemetry_consent') {
        return Promise.resolve({ enabled: true, restart_required: true });
      }
      return Promise.resolve(undefined);
    });

    render(<TelemetrySection />);

    await screen.findByText('Help improve Voicetypr with anonymous diagnostics');
    const sw = await screen.findByRole('switch');

    await act(async () => {
      fireEvent.click(sw);
    });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('set_telemetry_consent', {
        enabled: true,
      });
    });
  });

  it('disables the switch and shows the unavailable caption when not available', async () => {
    mockInvoke.mockResolvedValue({ enabled: false, available: false });

    render(<TelemetrySection />);

    const sw = await screen.findByRole('switch');
    await waitFor(() => {
      expect(sw).toBeDisabled();
    });

    expect(
      await screen.findByText(/Not available in this build/),
    ).toBeInTheDocument();
  });

  it('allows opting out even when telemetry is unavailable', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'get_telemetry_status') {
        return Promise.resolve({ enabled: true, available: false });
      }
      if (command === 'set_telemetry_consent') {
        return Promise.resolve({ enabled: false, restart_required: false });
      }
      return Promise.resolve(undefined);
    });

    render(<TelemetrySection />);

    const sw = await screen.findByRole('switch');
    await waitFor(() => {
      expect(sw).toBeChecked();
    });
    expect(sw).toBeEnabled();

    await act(async () => {
      fireEvent.click(sw);
    });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('set_telemetry_consent', {
        enabled: false,
      });
    });
  });
});
