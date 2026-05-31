import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { HelpSection } from '../HelpSection';
import { formatHotkeyDiagnosticContext } from '@/utils/hotkeyDiagnostics';
import { toast } from 'sonner';

const mockInvoke = vi.fn();
const mockReportBugDialog = vi.fn();

const baseHotkeyDiag = {
  configuredHotkey: 'CommandOrControl+Shift+Space',
  normalizedHotkey: 'CommandOrControl+Shift+Space',
  recordingMode: 'toggle',
  useDifferentPttKey: false,
  pttHotkey: null,
  normalizedPttHotkey: null,
  registrationStatus: 'registered',
  registrationError: null,
  lastRegistrationAttemptAt: '2026-05-31T00:00:00Z',
  lastSuccessfulRegistrationAt: '2026-05-31T00:00:01Z',
  lastEventAt: null,
  lastEventKind: null,
  lastEventState: null,
  eventCount: 0,
  currentRecordingState: 'idle',
  generatedAt: '2026-05-31T00:00:03Z',
  isRegistered: true,
};

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn().mockResolvedValue('1.0.0'),
}));

vi.mock('@tauri-apps/plugin-os', () => ({
  platform: vi.fn(() => 'macos'),
  type: vi.fn(() => 'macos'),
  version: vi.fn(() => '15.0'),
}));

vi.mock('@tauri-apps/plugin-shell', () => ({ open: vi.fn() }));

vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({ settings: { current_model: 'base.en' } }),
}));

vi.mock('@/contexts/ReadinessContext', () => ({
  useCanRecord: () => true,
  useCanAutoInsert: () => true,
}));

vi.mock('sonner', () => ({
  toast: {
    info: vi.fn(),
    success: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('@/components/ReportBugDialog', () => ({
  ReportBugDialog: (props: {
    isOpen: boolean;
    initialMessage?: string;
    diagnosticContext?: string;
    onClose: () => void;
  }) => {
    mockReportBugDialog(props);
    return props.isOpen ? <div data-testid="report-bug-dialog" /> : null;
  },
}));

vi.mock('@/components/ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

vi.mock('@/components/ui/collapsible', () => ({
  Collapsible: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  CollapsibleContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  CollapsibleTrigger: ({ children }: { children: React.ReactNode }) => <button type="button">{children}</button>,
}));

vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ children, open }: { children: React.ReactNode; open?: boolean }) =>
    open ? <div>{children}</div> : null,
  DialogContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DialogDescription: ({ children }: { children: React.ReactNode }) => <p>{children}</p>,
  DialogHeader: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DialogTitle: ({ children }: { children: React.ReactNode }) => <h2>{children}</h2>,
}));

describe('formatHotkeyDiagnosticContext', () => {
  it('formats hotkey registration and event fields for support reports', () => {
    const context = formatHotkeyDiagnosticContext({
      ...baseHotkeyDiag,
      lastEventAt: '2026-05-31T00:00:02Z',
      lastEventKind: 'recording',
      lastEventState: 'pressed',
      eventCount: 3,
    });

    expect(context).toContain('Configured Hotkey: CommandOrControl+Shift+Space');
    expect(context).toContain('Registration Status: Registered');
    expect(context).toContain('Is Registered: true');
    expect(context).toContain('Last Event: recording (pressed) at 2026-05-31T00:00:02Z');
    expect(context).toContain('Event Count: 3');
  });

  it('formats restored_after_failure status readably and keeps registration error', () => {
    const context = formatHotkeyDiagnosticContext({
      ...baseHotkeyDiag,
      registrationStatus: 'restored_after_failure',
      registrationError: 'Shortcut already registered by another app',
      isRegistered: true,
    });

    expect(context).toContain('Registration Status: Restored previous hotkey after failure');
    expect(context).toContain('Registration Error: Shortcut already registered by another app');
  });
});

function mockDefaultInvoke() {
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === 'get_device_id') {
      return Promise.resolve('device-1');
    }
    if (cmd === 'get_hotkey_diagnostics') {
      return Promise.resolve({ ...baseHotkeyDiag });
    }
    return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
  });
}

describe('HelpSection diagnostics flows', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockDefaultInvoke();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  async function renderHelpSection() {
    render(<HelpSection />);
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /run check/i })).toBeInTheDocument();
    });
  }

  it('shows generic diagnostics labels and current hotkey-only issue summary', async () => {
    await renderHelpSection();

    expect(screen.getByRole('heading', { name: 'Diagnostics' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'System check' })).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByText('No issue found yet — run a check to verify input capture')).toBeInTheDocument();
    });
    expect(screen.getByText('Status')).toBeInTheDocument();
    expect(screen.getByText('Not tested yet')).toBeInTheDocument();
    expect(screen.getByText('Latest issue')).toBeInTheDocument();
    expect(screen.getByText('Last checked')).toBeInTheDocument();
    expect(screen.getByText('Just now')).toBeInTheDocument();
    expect(screen.queryByText('Hotkey Diagnostics')).not.toBeInTheDocument();
    expect(screen.queryByText('Configured hotkey')).not.toBeInTheDocument();
    expect(screen.queryByText('Last event')).not.toBeInTheDocument();
  });

  it('shows generic attention summary when registration failed', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_device_id') {
        return Promise.resolve('device-1');
      }
      if (cmd === 'get_hotkey_diagnostics') {
        return Promise.resolve({
          ...baseHotkeyDiag,
          registrationStatus: 'failed',
          isRegistered: false,
        });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    await renderHelpSection();

    await waitFor(() => {
      expect(screen.getByText('Needs attention')).toBeInTheDocument();
      expect(screen.getByText('Global hotkey is not registered')).toBeInTheDocument();
    });
  });

  it('shows restored-after-failure registration error as latest issue', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_device_id') {
        return Promise.resolve('device-1');
      }
      if (cmd === 'get_hotkey_diagnostics') {
        return Promise.resolve({
          ...baseHotkeyDiag,
          registrationStatus: 'restored_after_failure',
          registrationError: 'Shortcut already registered by another app',
          isRegistered: true,
        });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    await renderHelpSection();

    await waitFor(() => {
      expect(screen.getByText('Needs attention')).toBeInTheDocument();
      expect(screen.getByText('Shortcut already registered by another app')).toBeInTheDocument();
    });
  });

  it('shows all good summary when input capture has been observed', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_device_id') {
        return Promise.resolve('device-1');
      }
      if (cmd === 'get_hotkey_diagnostics') {
        return Promise.resolve({
          ...baseHotkeyDiag,
          lastEventAt: '2026-05-31T00:00:02Z',
          lastEventKind: 'recording',
          lastEventState: 'pressed',
          eventCount: 1,
        });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    await renderHelpSection();

    await waitFor(() => {
      expect(screen.getByText('All good')).toBeInTheDocument();
      expect(screen.getByText('None')).toBeInTheDocument();
    });
  });

  it('shows success when eventCount increases during hotkey test', async () => {
    let hotkeyDiagCall = 0;

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_device_id') {
        return Promise.resolve('device-1');
      }
      if (cmd === 'get_hotkey_diagnostics') {
        hotkeyDiagCall += 1;
        const eventCount = hotkeyDiagCall >= 3 ? 1 : 0;
        return Promise.resolve({ ...baseHotkeyDiag, eventCount });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    await renderHelpSection();

    fireEvent.click(screen.getByRole('button', { name: /run check/i }));

    await waitFor(
      () => {
        expect(toast.success).toHaveBeenCalledWith('Hotkey detected');
      },
      { timeout: 3000 }
    );
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('cancels recording if Test Hotkey starts one from idle', async () => {
    let hotkeyDiagCall = 0;

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_device_id') {
        return Promise.resolve('device-1');
      }
      if (cmd === 'cancel_recording') {
        return Promise.resolve(undefined);
      }
      if (cmd === 'get_hotkey_diagnostics') {
        hotkeyDiagCall += 1;
        const detected = hotkeyDiagCall >= 3;
        return Promise.resolve({
          ...baseHotkeyDiag,
          eventCount: detected ? 1 : 0,
          currentRecordingState: detected ? 'recording' : 'idle',
        });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    await renderHelpSection();

    fireEvent.click(screen.getByRole('button', { name: /run check/i }));

    await waitFor(
      () => {
        expect(mockInvoke).toHaveBeenCalledWith('cancel_recording');
        expect(toast.success).toHaveBeenCalledWith('Hotkey detected');
      },
      { timeout: 3000 }
    );
  });

  it('shows timeout error when no hotkey event is detected', async () => {
    const dateNow = vi.spyOn(Date, 'now');
    let now = 1_000_000;
    let hotkeyDiagCall = 0;
    dateNow.mockImplementation(() => now);

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_device_id') {
        return Promise.resolve('device-1');
      }
      if (cmd === 'get_hotkey_diagnostics') {
        hotkeyDiagCall += 1;
        if (hotkeyDiagCall > 2) {
          now += 11_000;
        }
        return Promise.resolve({ ...baseHotkeyDiag, eventCount: 0 });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    await renderHelpSection();

    fireEvent.click(screen.getByRole('button', { name: /run check/i }));

    await waitFor(
      () => {
        expect(toast.error).toHaveBeenCalledWith('No hotkey was detected');
      },
      { timeout: 3000 }
    );
    expect(toast.success).not.toHaveBeenCalled();

    dateNow.mockRestore();
  });

  it('opens Report Bug dialog with hotkey diagnostic context', async () => {
    const user = userEvent.setup();

    await renderHelpSection();

    await user.click(screen.getByRole('button', { name: /report issue/i }));

    await waitFor(() => {
      expect(mockReportBugDialog).toHaveBeenCalled();
    });

    const lastCall = mockReportBugDialog.mock.calls[mockReportBugDialog.mock.calls.length - 1]?.[0];
    expect(lastCall?.isOpen).toBe(true);
    expect(lastCall?.initialMessage).toContain('global hotkey');
    expect(lastCall?.diagnosticContext).toContain('Configured Hotkey: CommandOrControl+Shift+Space');
    expect(screen.getByTestId('report-bug-dialog')).toBeInTheDocument();
  });

  it('stops hotkey test polling after unmount', async () => {
    const invokeCallsBeforeUnmount = { count: 0 };

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_device_id') {
        return Promise.resolve('device-1');
      }
      if (cmd === 'get_hotkey_diagnostics') {
        invokeCallsBeforeUnmount.count += 1;
        return Promise.resolve({ ...baseHotkeyDiag, eventCount: 0 });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    const { unmount } = render(<HelpSection />);
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /run check/i })).toBeInTheDocument();
    });

    vi.useFakeTimers({ toFake: ['setTimeout', 'Date'] });
    fireEvent.click(screen.getByRole('button', { name: /run check/i }));

    const callsWhenStarted = invokeCallsBeforeUnmount.count;
    unmount();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_500);
    });
    vi.useRealTimers();

    expect(invokeCallsBeforeUnmount.count).toBeLessThanOrEqual(callsWhenStarted + 1);
    expect(toast.error).not.toHaveBeenCalledWith('No hotkey was detected');
    expect(toast.success).not.toHaveBeenCalled();
  });
});
