import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RecentRecordings } from '../RecentRecordings';
import type { TranscriptionHistory } from '@/types';

const invokeMock = vi.fn();

const mockSettings: {
  current_model: string;
  current_model_engine: 'whisper' | 'parakeet' | 'soniox';
} = {
  current_model: 'small.en',
  current_model_engine: 'whisper',
};

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: vi.fn(async () => true),
}));

vi.mock('@/contexts/ReadinessContext', () => ({
  useCanRecord: () => true,
  useReadiness: () => ({
    canRecord: true,
    licenseStatus: 'licensed',
    hasModels: true,
    selectedModelAvailable: true,
    remoteSelected: false,
    hasMicrophonePermission: true,
  }),
  useCanAutoInsert: () => true,
}));

vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: mockSettings,
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
    mockSettings.current_model = 'small.en';
    mockSettings.current_model_engine = 'whisper';
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
        case 'get_active_remote_server':
          return null;
        default:
          return null;
      }
    });
  });

  it('uses the active remote server when re-transcribing', async () => {
    const user = userEvent.setup();

    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case 'check_recording_exists':
          return true;
        case 'get_active_remote_server':
          return 'online-server';
        case 'list_remote_servers':
          return [
            { id: 'online-server', name: 'Office PC', host: '10.0.0.4', port: 47842, model: 'large-v3' },
          ];
        case 'get_recordings_directory':
          return '/recordings';
        case 'save_retranscription':
          return 'retry-remote';
        case 'transcribe_remote':
          return 'Remote retry text';
        default:
          return null;
      }
    });

    render(<RecentRecordings history={[historyItem]} onHistoryUpdate={vi.fn()} />);

    const retranscribeButton = await screen.findByTitle('Re-transcribe with current source');
    await user.click(retranscribeButton);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('save_retranscription', {
        text: 'In progress...',
        model: 'Remote: Office PC',
        recordingFile: 'sample.wav',
        sourceRecordingId: '2024-01-01T00:00:00Z',
        status: 'in_progress',
      });
      expect(invokeMock).toHaveBeenCalledWith('transcribe_remote', {
        serverId: 'online-server',
        audioPath: '/recordings/sample.wav',
      });
      expect(invokeMock).toHaveBeenCalledWith('update_transcription', {
        timestamp: 'retry-remote',
        text: 'Remote retry text',
        model: 'Remote: Office PC',
        status: 'completed',
      });
    });
  });

  it('creates a durable in-progress entry before re-transcribing', async () => {
    const user = userEvent.setup();
    const onHistoryUpdate = vi.fn();
    const transcribeDeferred = createDeferred<{ text: string; words: null }>();

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

    const retranscribeButton = await screen.findByTitle('Re-transcribe with current source');
    await user.click(retranscribeButton);

    await waitFor(() => {
      expect(screen.getByTitle('Re-transcribe with current source')).toBeDisabled();
      expect(screen.getByText('Re-transcribing with Small (English)...')).toBeInTheDocument();
      expect(invokeMock).toHaveBeenCalledWith('save_retranscription', {
        text: 'In progress...',
        model: 'Small (English)',
        recordingFile: 'sample.wav',
        sourceRecordingId: '2024-01-01T00:00:00Z',
        status: 'in_progress',
      });
    });

    transcribeDeferred.resolve({ text: 'Re-transcribed text', words: null });

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

    const retranscribeButton = await screen.findByTitle('Re-transcribe with current source');
    await user.click(retranscribeButton);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('update_transcription', {
        timestamp: 'retry-2',
        text: 'Re-transcription failed: Error: remote offline',
        model: 'Small (English)',
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

    const retranscribeButton = await screen.findByTitle('Re-transcribe with current source');

    expect(retranscribeButton).toBeDisabled();
    expect(screen.getByText('Re-transcription in progress with Base (English)...')).toBeInTheDocument();
  });

  it('keeps reconciled failed rows retryable after reload', async () => {

    render(
      <RecentRecordings
        history={[{ ...historyItem, status: 'failed', text: 'Recovered after restart' }]}
        onHistoryUpdate={vi.fn()}
      />
    );

    const retranscribeButton = await screen.findByTitle('Re-transcribe with current source');

    expect(retranscribeButton).toBeEnabled();
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
    expect(screen.queryByTitle('Re-transcribe with current source')).not.toBeInTheDocument();
  });

});

it('uses Soniox when it is the current cloud transcription source', async () => {
  const user = userEvent.setup();
  mockSettings.current_model = 'soniox';
  mockSettings.current_model_engine = 'soniox';

  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'check_recording_exists':
        return true;
      case 'get_active_remote_server':
        return null;
      case 'get_recordings_directory':
        return '/recordings';
      case 'save_retranscription':
        return 'retry-soniox';
      case 'transcribe_audio_file':
        return { text: 'Cloud retry text', words: null };
      default:
        return null;
    }
  });

  render(<RecentRecordings history={[historyItem]} onHistoryUpdate={vi.fn()} />);

  const retranscribeButton = await screen.findByTitle('Re-transcribe with current source');
  await user.click(retranscribeButton);

  await waitFor(() => {
    expect(invokeMock).toHaveBeenCalledWith('save_retranscription', {
      text: 'In progress...',
      model: 'Soniox (Cloud)',
      recordingFile: 'sample.wav',
      sourceRecordingId: '2024-01-01T00:00:00Z',
      status: 'in_progress',
    });
    expect(invokeMock).toHaveBeenCalledWith('transcribe_audio_file', {
      filePath: '/recordings/sample.wav',
      modelName: 'soniox',
      modelEngine: 'soniox',
    });
    expect(invokeMock).toHaveBeenCalledWith('update_transcription', {
      timestamp: 'retry-soniox',
      text: 'Cloud retry text',
      model: 'Soniox (Cloud)',
      status: 'completed',
    });
  });
});
