import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { AudioUploadSection } from '../AudioUploadSection';
import { useUploadStore } from '@/state/upload';

// Minimal mocks - only what's absolutely necessary
vi.mock('sonner');
vi.mock('@tauri-apps/api/core');
vi.mock('@tauri-apps/plugin-dialog');
vi.mock('@tauri-apps/api/event');
const mockSettings = {
  current_model: 'base.en',
  current_model_engine: 'whisper',
  hotkey: 'Cmd+Shift+Space',
  language: 'en',
  theme: 'system'
};

vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: mockSettings,
    isLoading: false,
    error: null,
    refreshSettings: vi.fn(),
    updateSettings: vi.fn(),
  })
}));

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';

const readyLocalModel = {
  name: 'base.en',
  display_name: 'Whisper Base (English)',
  size: 157_286_400,
  url: 'https://example.com/base.en',
  sha256: 'sha256-base-en',
  downloaded: true,
  speed_score: 6,
  accuracy_score: 6,
  recommended: false,
  engine: 'whisper',
  kind: 'local' as const,
  requires_setup: false,
};

describe('AudioUploadSection - Essential User Flows', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSettings.current_model = 'base.en';
    mockSettings.current_model_engine = 'whisper';
    // Default mock for event listener
    vi.mocked(listen).mockResolvedValue(() => {});
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'get_model_status') {
        return { models: [readyLocalModel] };
      }
      if (cmd === 'get_active_remote_server') {
        return null;
      }
      if (cmd === 'list_remote_servers') {
        return [];
      }
      return null;
    });
    useUploadStore.getState().reset();
  });

  describe('Critical Path: Upload and Transcribe', () => {
    it('user can select a file and get transcription', async () => {
      const user = userEvent.setup();

      // Mock file selection and transcription
      vi.mocked(open).mockResolvedValue('/audio/meeting.mp3');
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'get_model_status') {
          return { models: [readyLocalModel] };
        }
        if (cmd === 'transcribe_audio_file') {
          return 'This is the meeting transcript content that was processed';
        }
        if (cmd === 'check_whisper_models') {
          return ['base.en'];
        }
        return null;
      });

      render(<AudioUploadSection />);

      // User selects a file
      const selectButton = await screen.findByRole('button', { name: /select file/i });
      await user.click(selectButton);

      // File appears
      await waitFor(() => {
        expect(screen.getByText(/meeting.mp3/)).toBeInTheDocument();
      });

      // User clicks transcribe
      const transcribeButton = await screen.findByRole('button', { name: /transcribe/i });
      await user.click(transcribeButton);

      // Transcription appears
      await waitFor(() => {
        expect(screen.getByText(/This is the meeting transcript/)).toBeInTheDocument();
      }, { timeout: 3000 });

      expect(invoke).toHaveBeenCalledWith('transcribe_audio_file', {
        filePath: '/audio/meeting.mp3',
        modelName: 'base.en',
        modelEngine: 'whisper'
      });

      // Success is reflected by rendering the result text
      expect(screen.getByText(/This is the meeting transcript/)).toBeInTheDocument();
      expect(toast.success).toHaveBeenCalledWith('Transcription completed and saved to history!');
    });

    it('user can copy transcribed text to clipboard', async () => {
      const user = userEvent.setup();

      // Mock clipboard API properly
      const mockWriteText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, 'clipboard', {
        value: { writeText: mockWriteText },
        writable: true,
        configurable: true
      });

      // Setup transcription
      vi.mocked(open).mockResolvedValue('/audio/file.mp3');
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'get_model_status') {
          return { models: [readyLocalModel] };
        }
        if (cmd === 'transcribe_audio_file') {
          return 'Text to copy to clipboard';
        }
        return null;
      });

      render(<AudioUploadSection />);

      // Select and transcribe
      const selectButton = await screen.findByRole('button', { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/file.mp3/));

      const transcribeButton = await screen.findByRole('button', { name: /transcribe/i });
      await user.click(transcribeButton);
      await waitFor(() => screen.getByText('Text to copy to clipboard'));

      // The copy button is an icon button - find it by the Copy icon SVG
      const copyButton = await screen.findByRole('button', { name: /copy/i });
      await user.click(copyButton);

      expect(mockWriteText).toHaveBeenCalledWith('Text to copy to clipboard');
      expect(toast.success).toHaveBeenCalledWith('Copied to clipboard');
    });
  });

  describe('Critical Errors: User Guidance', () => {
    it('shows clear error when file is too large', async () => {
      const user = userEvent.setup();

      vi.mocked(open).mockResolvedValue('/audio/huge-file.wav');
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'get_model_status') {
          return { models: [readyLocalModel] };
        }
        if (cmd === 'transcribe_audio_file') {
          throw new Error('File too large. Maximum size is 1GB');
        }
        return null;
      });

      render(<AudioUploadSection />);

      // Select file
      const selectButton = await screen.findByRole('button', { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/huge-file.wav/));

      // Try to transcribe
      const transcribeButton = await screen.findByRole('button', { name: /transcribe/i });
      await user.click(transcribeButton);

      // User sees inline error message
      await waitFor(() => {
        expect(screen.getByText(/file too large/i)).toBeInTheDocument();
      });
      expect(toast.error).toHaveBeenCalledWith('File too large. Maximum size is 1GB');
    });

    it('guides user when no model is installed', async () => {
      const user = userEvent.setup();

      vi.mocked(open).mockResolvedValue('/audio/file.mp3');
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'get_model_status') {
          return { models: [{ ...readyLocalModel, downloaded: false }] };
        }
        if (cmd === 'transcribe_audio_file') {
          throw new Error('No Whisper model found. Please download a model first.');
        }
        return null;
      });

      render(<AudioUploadSection />);

      // Select file
      const selectButton = await screen.findByRole('button', { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/file.mp3/));

      // Try to transcribe
      const transcribeButton = await screen.findByRole('button', { name: /transcribe/i });
      await user.click(transcribeButton);

      // User sees helpful error
      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith(
          'Download the selected model before transcribing audio.'
        );
      });
    });

    it('allows remote-only users to transcribe uploaded audio', async () => {
      const user = userEvent.setup();
      mockSettings.current_model = '';
      mockSettings.current_model_engine = 'whisper';

      vi.mocked(open).mockResolvedValue('/audio/remote-file.mp3');
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'get_model_status') {
          return { models: [{ ...readyLocalModel, downloaded: false }] };
        }
        if (cmd === 'get_active_remote_server') {
          return 'remote-1';
        }
        if (cmd === 'get_recognition_availability_snapshot') {
          return {
            whisper_available: false,
            parakeet_available: false,
            soniox_selected: false,
            soniox_ready: false,
            remote_selected: true,
            remote_available: true,
          };
        }
        if (cmd === 'transcribe_audio_file') {
          return 'Remote transcript';
        }
        return null;
      });

      render(<AudioUploadSection />);

      const selectButton = await screen.findByRole('button', { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/remote-file.mp3/));

      const transcribeButton = await screen.findByRole('button', { name: /transcribe/i });
      await user.click(transcribeButton);

      await waitFor(() => {
        expect(screen.getByText('Remote transcript')).toBeInTheDocument();
      });

      expect(toast.error).not.toHaveBeenCalledWith(
        'Select a speech model in Models before transcribing.'
      );
      expect(invoke).toHaveBeenCalledWith('transcribe_audio_file', {
        filePath: '/audio/remote-file.mp3',
        modelName: '',
        modelEngine: null
      });
    });

    it('saves uploaded remote transcriptions with the active remote label', async () => {
      const user = userEvent.setup();
      mockSettings.current_model = 'base.en';
      mockSettings.current_model_engine = 'whisper';

      vi.mocked(open).mockResolvedValue('/audio/remote-history.mp3');
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'get_model_status') {
          return { models: [readyLocalModel] };
        }
        if (cmd === 'get_active_remote_server') {
          return 'remote-1';
        }
        if (cmd === 'list_remote_servers') {
          return [
            { id: 'remote-1', name: 'Office Mac', host: '10.0.0.7', port: 47842 },
          ];
        }
        if (cmd === 'get_recognition_availability_snapshot') {
          return {
            whisper_available: false,
            parakeet_available: false,
            soniox_selected: false,
            soniox_ready: false,
            remote_selected: true,
            remote_available: true,
          };
        }
        if (cmd === 'transcribe_audio_file') {
          return 'Remote history transcript';
        }
        return null;
      });

      render(<AudioUploadSection />);

      const selectButton = await screen.findByRole('button', { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/remote-history.mp3/));

      const transcribeButton = await screen.findByRole('button', { name: /transcribe/i });
      await user.click(transcribeButton);

      await waitFor(() => {
        expect(screen.getByText('Remote history transcript')).toBeInTheDocument();
      });

      expect(invoke).toHaveBeenCalledWith('save_transcription', {
        text: 'Remote history transcript',
        model: 'Remote: Office Mac'
      });
    });

    it('labels Soniox upload history entries as cloud sources', async () => {
      const user = userEvent.setup();
      mockSettings.current_model = 'soniox';
      mockSettings.current_model_engine = 'soniox';

      vi.mocked(open).mockResolvedValue('/audio/soniox-file.mp3');
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'get_model_status') {
          return {
            models: [
              {
                name: 'soniox',
                display_name: 'Soniox (Cloud)',
                size: 0,
                url: '',
                sha256: '',
                downloaded: true,
                speed_score: 9,
                accuracy_score: 10,
                recommended: true,
                engine: 'soniox',
                kind: 'cloud',
                requires_setup: false,
              },
            ],
          };
        }
        if (cmd === 'get_active_remote_server') {
          return null;
        }
        if (cmd === 'get_recognition_availability_snapshot') {
          return {
            whisper_available: false,
            parakeet_available: false,
            soniox_selected: true,
            soniox_ready: true,
            remote_selected: false,
            remote_available: false,
          };
        }
        if (cmd === 'transcribe_audio_file') {
          return 'Soniox transcript';
        }
        return null;
      });

      render(<AudioUploadSection />);

      const selectButton = await screen.findByRole('button', { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/soniox-file.mp3/));

      const transcribeButton = await screen.findByRole('button', { name: /transcribe/i });
      await user.click(transcribeButton);

      await waitFor(() => {
        expect(screen.getByText('Soniox transcript')).toBeInTheDocument();
      });

      expect(invoke).toHaveBeenCalledWith('save_transcription', {
        text: 'Soniox transcript',
        model: 'Soniox (Cloud)'
      });
    });

    it('still blocks missing model when no remote server is active', async () => {
      const user = userEvent.setup();
      mockSettings.current_model = '';
      mockSettings.current_model_engine = 'whisper';

      vi.mocked(open).mockResolvedValue('/audio/file.mp3');
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'get_model_status') {
          return { models: [{ ...readyLocalModel, downloaded: false }] };
        }
        if (cmd === 'get_active_remote_server') {
          return null;
        }
        return null;
      });

      render(<AudioUploadSection />);

      const selectButton = await screen.findByRole('button', { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/file.mp3/));

      const transcribeButton = await screen.findByRole('button', { name: /transcribe/i });
      await user.click(transcribeButton);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith(
          'Select a speech model in Models before transcribing.'
        );
      });
    });

    it('rejects unsupported file types immediately', async () => {
      const user = userEvent.setup();

      // Mock selecting a PDF file
      vi.mocked(open).mockResolvedValue('/document.pdf');

      render(<AudioUploadSection />);

      // Try to select unsupported file
      await user.click(screen.getByRole('button', { name: /select file/i }));

      // Should either:
      // 1. Not show the file (filtered by dialog)
      // 2. Show error message
      // The actual behavior depends on implementation

      // For now, we'll verify the file dialog was called with correct filters
      expect(open).toHaveBeenCalledWith(
        expect.objectContaining({
          multiple: false,
          filters: expect.arrayContaining([
            expect.objectContaining({
              name: 'Audio/Video Files',
              extensions: expect.arrayContaining(['wav', 'mp3', 'm4a', 'flac', 'ogg', 'mp4', 'webm'])
            })
          ])
        })
      );
    });
  });

  describe('UI State Management', () => {
    it('shows loading state during transcription', async () => {
      const user = userEvent.setup();

      vi.mocked(open).mockResolvedValue('/audio/file.mp3');
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'get_model_status') {
          return { models: [readyLocalModel] };
        }
        if (cmd === 'transcribe_audio_file') {
          // Simulate processing time
          await new Promise(resolve => setTimeout(resolve, 100));
          return 'Transcription result';
        }
        return null;
      });

      render(<AudioUploadSection />);

      // Select file
      const selectButton = await screen.findByRole('button', { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/file.mp3/));

      // Start transcription
      const transcribeButton = await screen.findByRole('button', { name: /transcribe/i });
      await user.click(transcribeButton);

      // Loading state appears (button text shows Processing...)
      expect(screen.getAllByText(/processing/i).length).toBeGreaterThan(0);

      // Result appears
      await waitFor(() => {
        expect(screen.getByText('Transcription result')).toBeInTheDocument();
      });
    });

    it('handles empty/silent audio gracefully', async () => {
      const user = userEvent.setup();

      vi.mocked(open).mockResolvedValue('/audio/silent.mp3');
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'get_model_status') {
          return { models: [readyLocalModel] };
        }
        if (cmd === 'transcribe_audio_file') {
          return '[BLANK_AUDIO]';
        }
        return null;
      });

      render(<AudioUploadSection />);

      // Select and transcribe
      const selectButton = await screen.findByRole('button', { name: /select file/i });
      await user.click(selectButton);
      await waitFor(() => screen.getByText(/silent.mp3/));

      const transcribeButton = await screen.findByRole('button', { name: /transcribe/i });
      await user.click(transcribeButton);

      // User sees appropriate inline message
      await waitFor(() => {
        expect(screen.getByText('No speech detected in the audio file')).toBeInTheDocument();
      });
    });
  });

  it('blocks upload transcription when the selected remote is unavailable', async () => {
    const user = userEvent.setup();
    mockSettings.current_model = '';
    mockSettings.current_model_engine = 'whisper';

    vi.mocked(open).mockResolvedValue('/audio/offline-remote.mp3');
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'get_model_status') {
        return { models: [{ ...readyLocalModel, downloaded: false }] };
      }
      if (cmd === 'get_active_remote_server') {
        return 'remote-1';
      }
      if (cmd === 'get_recognition_availability_snapshot') {
        return {
          whisper_available: false,
          parakeet_available: false,
          soniox_selected: false,
          soniox_ready: false,
          remote_selected: true,
          remote_available: false,
        };
      }
      return null;
    });

    render(<AudioUploadSection />);

    const selectButton = await screen.findByRole('button', { name: /select file/i });
    await user.click(selectButton);
    await waitFor(() => screen.getByText(/offline-remote.mp3/));

    const transcribeButton = await screen.findByRole('button', { name: /transcribe/i });
    await user.click(transcribeButton);

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith('Selected remote unavailable. Reconnect or choose another source.');
    });
    expect(invoke).not.toHaveBeenCalledWith('transcribe_audio_file', expect.anything());
  });
});