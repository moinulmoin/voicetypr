import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RecentRecordings } from '../RecentRecordings';
import type { TranscriptionHistory } from '@/types';

const invokeMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: vi.fn(async () => true),
}));

vi.mock('@/contexts/ReadinessContext', () => ({
  useCanRecord: () => true,
  useCanAutoInsert: () => true,
}));

vi.mock('@/contexts/ModelManagementContext', () => ({
  useModelManagementContext: () => ({
    modelOrder: ['small.en', 'base.en', 'large-v3'],
  }),
}));

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

const historyItem: TranscriptionHistory = {
  id: '2024-01-01T00:00:00Z',
  text: 'Original transcript',
  timestamp: new Date('2024-01-01T00:00:00Z'),
  model: 'base.en',
  recording_file: 'sample.wav',
};

const createDeferred = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });

  return { promise, resolve };
};

describe('RecentRecordings re-transcription', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case 'check_recording_exists':
          return true;
        case 'get_model_status':
          return {
            models: [{ name: 'small.en', downloaded: true, engine: 'whisper' }],
          };
        case 'list_remote_servers':
          return [];
        default:
          return null;
      }
    });
  });

  it('only lists online remote servers in the re-transcribe menu', async () => {
    const user = userEvent.setup();

    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      switch (cmd) {
        case 'check_recording_exists':
          return true;
        case 'get_model_status':
          return {
            models: [{ name: 'small.en', downloaded: true, engine: 'whisper' }],
          };
        case 'list_remote_servers':
          return [
            { id: 'online-server', name: 'Office PC', host: '10.0.0.4', port: 47842 },
            { id: 'offline-server', name: 'Old PC', host: '10.0.0.8', port: 47842 },
          ];
        case 'test_remote_server':
          if (args?.serverId === 'online-server') {
            return { model: 'large-v3', engine: 'whisper' };
          }
          throw new Error('offline');
        default:
          return null;
      }
    });

    render(<RecentRecordings history={[historyItem]} onHistoryUpdate={vi.fn()} />);

    const retranscribeButton = await screen.findByTitle('Re-transcribe');
    await user.click(retranscribeButton);

    expect(await screen.findByText('Office PC - large-v3')).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.queryByText('Old PC')).not.toBeInTheDocument();
    });
  });

  it('creates a durable in-progress entry before re-transcribing', async () => {
    const user = userEvent.setup();
    const onHistoryUpdate = vi.fn();
    const transcribeDeferred = createDeferred<string>();

    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case 'check_recording_exists':
          return true;
        case 'get_model_status':
          return {
            models: [{ name: 'small.en', downloaded: true, engine: 'whisper' }],
          };
        case 'list_remote_servers':
          return [];
        case 'get_recordings_directory':
          return '/recordings';
        case 'save_retranscription':
          return 'retry-1';
        case 'transcribe_audio_file':
          return transcribeDeferred.promise;
        default:
          return null;
      }
    });

    render(<RecentRecordings history={[historyItem]} onHistoryUpdate={onHistoryUpdate} />);

    const retranscribeButton = await screen.findByTitle('Re-transcribe');
    await user.click(retranscribeButton);

    const sourceOption = await screen.findByRole('menuitem', { name: 'Small (English)' });
    await user.click(sourceOption);

    await waitFor(() => {
      expect(screen.getByTitle('Re-transcribe')).toBeDisabled();
      expect(screen.getByText('Re-transcribing with Small (English)...')).toBeInTheDocument();
      expect(invokeMock).toHaveBeenCalledWith('save_retranscription', {
        text: 'In progress...',
        model: 'small.en',
        recordingFile: 'sample.wav',
        sourceRecordingId: '2024-01-01T00:00:00Z',
        status: 'in_progress',
      });
    });

    transcribeDeferred.resolve('Re-transcribed text');

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('update_transcription', {
        timestamp: 'retry-1',
        text: 'Re-transcribed text',
        model: 'small.en',
        status: 'completed',
      });
    });
    await waitFor(() => {
      expect(onHistoryUpdate).toHaveBeenCalled();
    });
  });

  it('marks the pending retry as failed when re-transcription errors', async () => {
    const user = userEvent.setup();
    const onHistoryUpdate = vi.fn();

    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case 'check_recording_exists':
          return true;
        case 'get_model_status':
          return {
            models: [{ name: 'small.en', downloaded: true, engine: 'whisper' }],
          };
        case 'list_remote_servers':
          return [];
        case 'get_recordings_directory':
          return '/recordings';
        case 'save_retranscription':
          return 'retry-2';
        case 'transcribe_audio_file':
          throw new Error('remote offline');
        default:
          return null;
      }
    });

    render(<RecentRecordings history={[historyItem]} onHistoryUpdate={onHistoryUpdate} />);

    const retranscribeButton = await screen.findByTitle('Re-transcribe');
    await user.click(retranscribeButton);

    const sourceOption = await screen.findByRole('menuitem', { name: 'Small (English)' });
    await user.click(sourceOption);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('update_transcription', {
        timestamp: 'retry-2',
        text: 'Re-transcription failed: Error: remote offline',
        model: 'small.en',
        status: 'failed',
      });
    });

    expect(onHistoryUpdate).toHaveBeenCalled();
  });

  it('keeps a loaded persisted in-progress row blocked until backend reconciliation', async () => {
    render(
      <RecentRecordings
        history={[{ ...historyItem, status: 'in_progress', text: 'Still retrying' }]}
        onHistoryUpdate={vi.fn()}
      />
    );

    const retranscribeButton = await screen.findByTitle('Re-transcribe');

    expect(retranscribeButton).toBeDisabled();
    expect(screen.getByText('Re-transcription in progress with base.en...')).toBeInTheDocument();
  });

  it('keeps reconciled failed rows retryable after reload', async () => {
    const user = userEvent.setup();

    render(
      <RecentRecordings
        history={[{ ...historyItem, status: 'failed', text: 'Recovered after restart' }]}
        onHistoryUpdate={vi.fn()}
      />
    );

    const retranscribeButton = await screen.findByTitle('Re-transcribe');

    expect(retranscribeButton).toBeEnabled();

    await user.click(retranscribeButton);

    expect(await screen.findByRole('menuitem', { name: 'Small (English)' })).toBeInTheDocument();
  });
  it('shows neutral failed copy when the recording is unavailable for retry', async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case 'check_recording_exists':
          return false;
        case 'get_model_status':
          return {
            models: [{ name: 'small.en', downloaded: true, engine: 'whisper' }],
          };
        case 'list_remote_servers':
          return [];
        default:
          return null;
      }
    });

    render(
      <RecentRecordings
        history={[{ ...historyItem, status: 'failed', text: 'Recovered after restart' }]}
        onHistoryUpdate={vi.fn()}
      />
    );

    expect(await screen.findByText('Transcription failed - recording unavailable for retry')).toBeInTheDocument();
    expect(screen.queryByTitle('Re-transcribe')).not.toBeInTheDocument();
  });

});

it('shows Soniox as a cloud retranscription source when configured', async () => {
  const user = userEvent.setup();

  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'check_recording_exists':
        return true;
      case 'get_model_status':
        return {
          models: [
            { name: 'small.en', downloaded: true, engine: 'whisper', kind: 'local', requires_setup: false },
            { name: 'soniox', display_name: 'Soniox (Cloud)', downloaded: true, engine: 'soniox', kind: 'cloud', requires_setup: false },
          ],
        };
      case 'list_remote_servers':
        return [];
      default:
        return null;
    }
  });

  render(<RecentRecordings history={[historyItem]} onHistoryUpdate={vi.fn()} />);

  const retranscribeButton = await screen.findByTitle('Re-transcribe');
  await user.click(retranscribeButton);

  expect(await screen.findByRole('menuitem', { name: 'Soniox (Cloud)' })).toBeInTheDocument();
});
