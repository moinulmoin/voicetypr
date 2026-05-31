import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { MockInstance } from 'vitest';
import { ReportBugDialog } from './ReportBugDialog';
import { gatherManualReportData, buildReportBody, submitManualReport } from '@/utils/crashReport';
import { toast } from 'sonner';

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: { current_model: 'base.en' },
  }),
}));

vi.mock('@/utils/crashReport', () => ({
  gatherManualReportData: vi.fn(),
  buildReportBody: vi.fn(),
  submitManualReport: vi.fn(),
}));

let writeTextMock: MockInstance<(data: string) => Promise<void>>;

describe('ReportBugDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(gatherManualReportData).mockResolvedValue({
      message: 'The app broke',
      appVersion: '1.0.0',
      platform: 'macos',
      osVersion: '15.0',
      architecture: 'aarch64',
      currentModel: 'base.en',
      deviceId: 'device-123',
      timestamp: '2026-04-27T00:00:00.000Z',
      logFileName: 'voicetypr-2026-04-27.log',
      logContent: 'INFO log line',
      logTruncated: false,
      logStatusNote: '',
    });
    vi.mocked(buildReportBody).mockReturnValue('REPORT BODY with The app broke');
    vi.mocked(submitManualReport).mockResolvedValue({ success: true, message: 'Report submitted' });
    if (!navigator.clipboard) {
      Object.defineProperty(navigator, 'clipboard', {
        configurable: true,
        value: { writeText: async () => undefined },
      });
    }
    writeTextMock = vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue(undefined);
  });

  it('requires a message before submitting', async () => {
    const user = userEvent.setup();

    render(<ReportBugDialog isOpen onClose={vi.fn()} />);

    await user.click(screen.getByRole('button', { name: /submit/i }));

    expect(screen.getByText(/please describe the issue/i)).toBeInTheDocument();
    expect(gatherManualReportData).not.toHaveBeenCalled();
    expect(submitManualReport).not.toHaveBeenCalled();
    expect(screen.getByLabelText(/message/i)).toHaveAttribute('aria-required', 'true');
  });

  it('validates optional email format before submitting', async () => {
    const user = userEvent.setup();

    render(<ReportBugDialog isOpen onClose={vi.fn()} />);

    await user.type(screen.getByLabelText(/email/i), 'not-an-email');
    await user.type(screen.getByLabelText(/message/i), 'The app broke');
    await user.click(screen.getByRole('button', { name: /submit/i }));

    expect(screen.getByText(/enter a valid email/i)).toBeInTheDocument();
    expect(gatherManualReportData).not.toHaveBeenCalled();
    expect(submitManualReport).not.toHaveBeenCalled();
  });

  it('resets form state when reopened', async () => {
    const user = userEvent.setup();
    const { rerender } = render(<ReportBugDialog isOpen onClose={vi.fn()} />);

    await user.type(screen.getByLabelText(/name/i), 'Moin');
    await user.type(screen.getByLabelText(/email/i), 'bad-email');
    await user.type(screen.getByLabelText(/message/i), 'Draft issue');
    await user.click(screen.getByRole('button', { name: /submit/i }));
    expect(screen.getByText(/enter a valid email/i)).toBeInTheDocument();

    rerender(<ReportBugDialog isOpen={false} onClose={vi.fn()} />);
    rerender(<ReportBugDialog isOpen onClose={vi.fn()} />);

    expect(screen.getByLabelText(/name/i)).toHaveValue('');
    expect(screen.getByLabelText(/email/i)).toHaveValue('');
    expect(screen.getByLabelText(/message/i)).toHaveValue('');
    expect(screen.queryByText(/enter a valid email/i)).not.toBeInTheDocument();
  });

  it('keeps the dialog open when gathering report data fails', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.mocked(gatherManualReportData).mockRejectedValueOnce(new Error('invoke failed'));

    render(<ReportBugDialog isOpen onClose={onClose} />);

    await user.type(screen.getByLabelText(/message/i), 'The app broke');
    await user.click(screen.getByRole('button', { name: /submit/i }));

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith('Failed to gather report data');
    });
    expect(submitManualReport).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it('does not show copy fallback before a submit failure', () => {
    render(<ReportBugDialog isOpen onClose={vi.fn()} />);

    expect(screen.queryByRole('button', { name: /copy report/i })).not.toBeInTheDocument();
    expect(screen.getByText(/anonymous device ID/i)).toBeInTheDocument();
  });

  it('submits a report directly to support', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    render(<ReportBugDialog isOpen onClose={onClose} />);

    await user.type(screen.getByLabelText(/name/i), 'Moin');
    await user.type(screen.getByLabelText(/email/i), 'moin@example.com');
    await user.type(screen.getByLabelText(/message/i), 'The app broke');
    await user.click(screen.getByRole('button', { name: /submit/i }));

    await waitFor(() => {
      expect(submitManualReport).toHaveBeenCalledTimes(1);
    });
    expect(gatherManualReportData).toHaveBeenCalledWith(
      'Moin',
      'moin@example.com',
      'The app broke',
      'base.en',
      undefined
    );
    expect(submitManualReport).toHaveBeenCalledWith(expect.objectContaining({
      message: 'The app broke',
      logContent: 'INFO log line',
    }));
    expect(onClose).toHaveBeenCalled();
  });

  it('copies the generated report body after submit failure and keeps the dialog open', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    vi.mocked(submitManualReport).mockResolvedValueOnce({
      success: false,
      message: 'Too many reports. Please try again later.',
    });

    render(<ReportBugDialog isOpen onClose={onClose} />);

    await user.type(screen.getByLabelText(/message/i), 'Copy this report');
    expect(screen.queryByRole('button', { name: /copy report/i })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /submit/i }));
    expect(await screen.findByRole('button', { name: /copy report/i })).toBeEnabled();

    await user.click(screen.getByRole('button', { name: /copy report/i }));
    expect(buildReportBody).toHaveBeenCalled();

    await waitFor(() => {
      expect(writeTextMock).toHaveBeenCalledWith('REPORT BODY with The app broke');
    });
    expect(gatherManualReportData).toHaveBeenCalledTimes(1);
    expect(onClose).not.toHaveBeenCalled();
  });

  it('shows an error and keeps the dialog open when submit fails', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    vi.mocked(submitManualReport).mockResolvedValueOnce({
      success: false,
      message: 'Too many reports. Please try again later.',
    });

    render(<ReportBugDialog isOpen onClose={onClose} />);

    await user.type(screen.getByLabelText(/message/i), 'A very long issue report');
    await user.click(screen.getByRole('button', { name: /submit/i }));

    expect(toast.error).toHaveBeenCalledWith('Too many reports. Please try again later.');
    expect(await screen.findByRole('button', { name: /copy report/i })).toBeEnabled();
    expect(screen.getByText(/copy the prepared report/i)).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  it('still submits when the latest log is unavailable', async () => {
    const user = userEvent.setup();
    vi.mocked(gatherManualReportData).mockResolvedValueOnce({
      message: 'No log case',
      appVersion: '1.0.0',
      platform: 'macos',
      osVersion: '15.0',
      architecture: 'aarch64',
      currentModel: 'base.en',
      deviceId: 'device-123',
      timestamp: '2026-04-27T00:00:00.000Z',
      logFileName: null,
      logContent: '',
      logTruncated: false,
      logStatusNote: 'No log file found.',
    });

    render(<ReportBugDialog isOpen onClose={vi.fn()} />);

    await user.type(screen.getByLabelText(/message/i), 'No log case');
    await user.click(screen.getByRole('button', { name: /submit/i }));

    await waitFor(() => {
      expect(submitManualReport).toHaveBeenCalledTimes(1);
    });
    expect(submitManualReport).toHaveBeenCalledWith(expect.objectContaining({
      logFileName: null,
      logStatusNote: 'No log file found.',
    }));
  });
  it('prefills the message when initialMessage is provided', () => {
    const { rerender } = render(
      <ReportBugDialog isOpen initialMessage="Hotkey issue details" onClose={vi.fn()} />
    );

    expect(screen.getByLabelText(/message/i)).toHaveValue('Hotkey issue details');

    rerender(<ReportBugDialog isOpen={false} onClose={vi.fn()} />);
    rerender(
      <ReportBugDialog isOpen initialMessage="Updated hotkey issue" onClose={vi.fn()} />
    );

    expect(screen.getByLabelText(/message/i)).toHaveValue('Updated hotkey issue');
  });

  it('passes diagnostic context when gathering report data', async () => {
    const user = userEvent.setup();

    render(
      <ReportBugDialog
        isOpen
        diagnosticContext={'Configured Hotkey: Cmd+Shift+Space'}
        onClose={vi.fn()}
      />
    );

    await user.type(screen.getByLabelText(/message/i), 'Hotkey stopped working');
    await user.click(screen.getByRole('button', { name: /submit/i }));

    await waitFor(() => {
      expect(gatherManualReportData).toHaveBeenCalledWith(
        undefined,
        undefined,
        'Hotkey stopped working',
        'base.en',
        'Configured Hotkey: Cmd+Shift+Space'
      );
    });
  });

});
