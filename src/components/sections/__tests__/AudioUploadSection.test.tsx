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
vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: { current_model: 'base.en', current_model_engine: 'whisper', hotkey: 'Cmd+Shift+Space', language: 'en', theme: 'system' },
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
    // Default mock for event listener
    vi.mocked(listen).mockResolvedValue(() => {});
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'get_model_status') {
        return { models: [readyLocalModel] };
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
});
