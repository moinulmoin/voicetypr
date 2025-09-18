import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { AudioUploadSection } from '../AudioUploadSection';
import { toast } from 'sonner';
import * as tauriCore from '@tauri-apps/api/core';
import * as tauriDialog from '@tauri-apps/plugin-dialog';
import * as tauriEvent from '@tauri-apps/api/event';

// Mock all external dependencies
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}));

vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: vi.fn(),
}));

// Create a mock settings context
const mockUseSettings = vi.mocked(vi.importMock('@/contexts/SettingsContext')).useSettings;

describe('AudioUploadSection', () => {
  const mockInvoke = vi.mocked(tauriCore.invoke);
  const mockOpen = vi.mocked(tauriDialog.open);
  const mockListen = vi.mocked(tauriEvent.listen);

  beforeEach(() => {
    // Reset all mocks
    vi.clearAllMocks();

    // Setup default settings mock
    mockUseSettings.mockReturnValue({
      settings: {
        current_model: 'base.en',
      },
    });

    // Setup default tauri event listeners
    mockListen.mockImplementation(() => Promise.resolve(() => {}));
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  describe('Initial Render', () => {
    it('renders upload section with proper initial state', () => {
      render(<AudioUploadSection />);

      expect(screen.getByText('Audio Upload')).toBeInTheDocument();
      expect(screen.getByText('Transcribe audio files locally')).toBeInTheDocument();
      expect(screen.getByText('Drag & drop your audio file here')).toBeInTheDocument();
      expect(screen.getByText('Select File')).toBeInTheDocument();
    });

    it('shows supported file formats information', () => {
      render(<AudioUploadSection />);

      expect(screen.getByText('Supported Formats:')).toBeInTheDocument();
      expect(screen.getByText(/WAV, MP3, M4A, FLAC, OGG, MP4, WebM/)).toBeInTheDocument();
    });

    it('displays important information card', () => {
      render(<AudioUploadSection />);

      expect(screen.getByText('Important Information')).toBeInTheDocument();
      expect(screen.getByText(/Processing.*locally on your device/)).toBeInTheDocument();
      expect(screen.getByText(/~230MB per hour of audio/)).toBeInTheDocument();
    });
  });

  describe('File Selection', () => {
    it('handles file selection via dialog', async () => {
      mockOpen.mockResolvedValue('/path/to/test.wav');

      render(<AudioUploadSection />);

      const selectButton = screen.getByText('Select File');
      fireEvent.click(selectButton);

      await waitFor(() => {
        expect(mockOpen).toHaveBeenCalledWith({
          multiple: false,
          filters: [
            {
              name: 'Audio Files',
              extensions: ['wav', 'mp3', 'm4a', 'flac', 'ogg', 'mp4', 'webm'],
            },
          ],
        });
      });

      expect(toast.success).toHaveBeenCalledWith('Selected: test.wav');
    });

    it('handles file selection cancellation', async () => {
      mockOpen.mockResolvedValue(null);

      render(<AudioUploadSection />);

      const selectButton = screen.getByText('Select File');
      fireEvent.click(selectButton);

      await waitFor(() => {
        expect(mockOpen).toHaveBeenCalled();
      });

      expect(toast.success).not.toHaveBeenCalled();
    });

    it('handles file selection error', async () => {
      mockOpen.mockRejectedValue(new Error('File selection failed'));

      render(<AudioUploadSection />);

      const selectButton = screen.getByText('Select File');
      fireEvent.click(selectButton);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith('Failed to select file');
      });
    });

    it('shows selected file information', async () => {
      mockOpen.mockResolvedValue('/path/to/selected_audio.wav');

      render(<AudioUploadSection />);

      fireEvent.click(screen.getByText('Select File'));

      await waitFor(() => {
        expect(screen.getByText('selected_audio.wav')).toBeInTheDocument();
        expect(screen.getByText('Change')).toBeInTheDocument();
        expect(screen.getByText('Transcribe')).toBeInTheDocument();
      });
    });
  });

  describe('Drag and Drop', () => {
    beforeEach(() => {
      // Mock the event listeners for drag and drop
      let dragDropCallback: any;
      let dragHoverCallback: any;
      let dragLeaveCallback: any;

      mockListen.mockImplementation((eventName: string, callback: any) => {
        if (eventName === 'tauri://drag-drop') {
          dragDropCallback = callback;
        } else if (eventName === 'tauri://drag-hover') {
          dragHoverCallback = callback;
        } else if (eventName === 'tauri://drag-leave') {
          dragLeaveCallback = callback;
        }
        return Promise.resolve(() => {});
      });

      // Store callbacks globally for test access
      (global as any).mockDragCallbacks = {
        drop: (payload: any) => dragDropCallback?.({ payload }),
        hover: () => dragHoverCallback?.(),
        leave: () => dragLeaveCallback?.(),
      };
    });

    it('handles drag hover state', async () => {
      render(<AudioUploadSection />);

      // Simulate drag hover
      (global as any).mockDragCallbacks.hover();

      await waitFor(() => {
        expect(screen.getByText('Drop your audio file here')).toBeInTheDocument();
      });
    });

    it('handles file drop with valid audio file', async () => {
      render(<AudioUploadSection />);

      // Simulate file drop
      const dropPayload = {
        paths: ['/path/to/dropped_file.mp3'],
        position: { x: 100, y: 200 },
      };

      (global as any).mockDragCallbacks.drop(dropPayload);

      await waitFor(() => {
        expect(toast.success).toHaveBeenCalledWith('Selected: dropped_file.mp3');
        expect(screen.getByText('dropped_file.mp3')).toBeInTheDocument();
      });
    });

    it('rejects unsupported file formats', async () => {
      render(<AudioUploadSection />);

      const dropPayload = {
        paths: ['/path/to/document.txt'],
        position: { x: 100, y: 200 },
      };

      (global as any).mockDragCallbacks.drop(dropPayload);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith(
          'Unsupported file format. Please drop an audio or video file.'
        );
      });
    });

    it('handles multiple files by taking first one', async () => {
      render(<AudioUploadSection />);

      const dropPayload = {
        paths: ['/path/to/first.wav', '/path/to/second.mp3'],
        position: { x: 100, y: 200 },
      };

      (global as any).mockDragCallbacks.drop(dropPayload);

      await waitFor(() => {
        expect(toast.success).toHaveBeenCalledWith('Selected: first.wav');
      });
    });

    it('handles drag leave state', async () => {
      render(<AudioUploadSection />);

      // First trigger hover to set dragging state
      (global as any).mockDragCallbacks.hover();

      await waitFor(() => {
        expect(screen.getByText('Drop your audio file here')).toBeInTheDocument();
      });

      // Then trigger leave
      (global as any).mockDragCallbacks.leave();

      await waitFor(() => {
        expect(screen.getByText('Drag & drop your audio file here')).toBeInTheDocument();
      });
    });
  });

  describe('Transcription Process', () => {
    beforeEach(async () => {
      // Setup file selection first
      mockOpen.mockResolvedValue('/path/to/test.wav');
      render(<AudioUploadSection />);
      fireEvent.click(screen.getByText('Select File'));
      await waitFor(() => {
        expect(screen.getByText('Transcribe')).toBeInTheDocument();
      });
    });

    it('prevents transcription without selected file', async () => {
      render(<AudioUploadSection />);

      // Try to transcribe without selecting file
      const transcribeButton = screen.queryByText('Transcribe');
      expect(transcribeButton).not.toBeInTheDocument();
    });

    it('prevents transcription without model', async () => {
      mockUseSettings.mockReturnValue({
        settings: {
          current_model: null,
        },
      });

      render(<AudioUploadSection />);
      mockOpen.mockResolvedValue('/path/to/test.wav');
      fireEvent.click(screen.getByText('Select File'));

      await waitFor(() => {
        const transcribeButton = screen.getByText('Transcribe');
        fireEvent.click(transcribeButton);
      });

      expect(toast.error).toHaveBeenCalledWith('Please download a model first from Settings');
    });

    it('shows loading state during transcription', async () => {
      // Mock a pending transcription
      mockInvoke.mockImplementation(() => new Promise(() => {})); // Never resolves

      const transcribeButton = screen.getByText('Transcribe');
      fireEvent.click(transcribeButton);

      await waitFor(() => {
        expect(screen.getByText('Transcribing...')).toBeInTheDocument();
        expect(screen.getByLabelText('Loading')).toBeInTheDocument();
      });
    });

    it('handles successful transcription', async () => {
      const mockTranscriptionResult = 'Hello, this is a test transcription.';
      mockInvoke.mockResolvedValueOnce(mockTranscriptionResult);
      mockInvoke.mockResolvedValueOnce(undefined); // For save_transcription

      const transcribeButton = screen.getByText('Transcribe');
      fireEvent.click(transcribeButton);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith('transcribe_audio_file', {
          filePath: '/path/to/test.wav',
          modelName: 'base.en',
        });
      });

      await waitFor(() => {
        expect(screen.getByText(mockTranscriptionResult)).toBeInTheDocument();
        expect(screen.getByText('6 words')).toBeInTheDocument();
        expect(screen.getByText('Transcribe Another File')).toBeInTheDocument();
      });

      expect(toast.success).toHaveBeenCalledWith('Transcription completed and saved to history!');
    });

    it('handles empty transcription result', async () => {
      mockInvoke.mockResolvedValue('');

      const transcribeButton = screen.getByText('Transcribe');
      fireEvent.click(transcribeButton);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith('No speech detected in the audio file');
      });
    });

    it('handles blank audio result', async () => {
      mockInvoke.mockResolvedValue('[BLANK_AUDIO]');

      const transcribeButton = screen.getByText('Transcribe');
      fireEvent.click(transcribeButton);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith('No speech detected in the audio file');
      });
    });

    it('handles transcription error', async () => {
      const errorMessage = 'Model not found';
      mockInvoke.mockRejectedValue(new Error(errorMessage));

      const transcribeButton = screen.getByText('Transcribe');
      fireEvent.click(transcribeButton);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith(`Transcription failed: Error: ${errorMessage}`);
      });
    });

    it('disables buttons during processing', async () => {
      // Mock a pending transcription
      mockInvoke.mockImplementation(() => new Promise(() => {}));

      const transcribeButton = screen.getByText('Transcribe');
      const selectButton = screen.getByText('Change');

      fireEvent.click(transcribeButton);

      await waitFor(() => {
        expect(screen.getByText('Transcribing...')).toBeInTheDocument();
      });

      // Buttons should be disabled during processing
      expect(selectButton.closest('button')).toBeDisabled();
    });
  });

  describe('Transcription Results', () => {
    const setupSuccessfulTranscription = async () => {
      const mockResult = 'This is a successful transcription result.';
      mockOpen.mockResolvedValue('/path/to/audio.wav');
      mockInvoke.mockResolvedValueOnce(mockResult);
      mockInvoke.mockResolvedValueOnce(undefined); // For save_transcription

      render(<AudioUploadSection />);
      fireEvent.click(screen.getByText('Select File'));

      await waitFor(() => {
        fireEvent.click(screen.getByText('Transcribe'));
      });

      await waitFor(() => {
        expect(screen.getByText(mockResult)).toBeInTheDocument();
      });

      return mockResult;
    };

    it('displays transcription result with metadata', async () => {
      const result = await setupSuccessfulTranscription();

      expect(screen.getByText(result)).toBeInTheDocument();
      expect(screen.getByText('audio.wav')).toBeInTheDocument();
      expect(screen.getByText('7 words')).toBeInTheDocument();
    });

    it('allows copying transcription result', async () => {
      await setupSuccessfulTranscription();

      // Mock clipboard API
      Object.assign(navigator, {
        clipboard: {
          writeText: vi.fn().mockResolvedValue(undefined),
        },
      });

      const copyButton = screen.getByLabelText('Copy');
      fireEvent.click(copyButton);

      await waitFor(() => {
        expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
          'This is a successful transcription result.'
        );
        expect(toast.success).toHaveBeenCalledWith('Copied to clipboard');
      });

      // Should show check icon temporarily
      expect(screen.getByLabelText('Check')).toBeInTheDocument();
    });

    it('handles copy failure', async () => {
      await setupSuccessfulTranscription();

      // Mock clipboard API failure
      Object.assign(navigator, {
        clipboard: {
          writeText: vi.fn().mockRejectedValue(new Error('Copy failed')),
        },
      });

      const copyButton = screen.getByLabelText('Copy');
      fireEvent.click(copyButton);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith('Failed to copy to clipboard');
      });
    });

    it('allows resetting to transcribe another file', async () => {
      await setupSuccessfulTranscription();

      const resetButton = screen.getByText('Transcribe Another File');
      fireEvent.click(resetButton);

      await waitFor(() => {
        expect(screen.getByText('Drag & drop your audio file here')).toBeInTheDocument();
        expect(screen.getByText('Select File')).toBeInTheDocument();
      });

      // Previous result should be cleared
      expect(screen.queryByText('This is a successful transcription result.')).not.toBeInTheDocument();
    });
  });

  describe('Error Handling and Edge Cases', () => {
    it('handles very long filenames gracefully', async () => {
      const longFilename = 'a'.repeat(200) + '.wav';
      const longPath = `/path/to/${longFilename}`;
      mockOpen.mockResolvedValue(longPath);

      render(<AudioUploadSection />);
      fireEvent.click(screen.getByText('Select File'));

      await waitFor(() => {
        expect(screen.getByText(longFilename)).toBeInTheDocument();
      });
    });

    it('handles special characters in filenames', async () => {
      const specialFilename = 'test-file_with.special[chars].wav';
      mockOpen.mockResolvedValue(`/path/to/${specialFilename}`);

      render(<AudioUploadSection />);
      fireEvent.click(screen.getByText('Select File'));

      await waitFor(() => {
        expect(screen.getByText(specialFilename)).toBeInTheDocument();
      });
    });

    it('handles unicode filenames', async () => {
      const unicodeFilename = '测试音频文件.wav';
      mockOpen.mockResolvedValue(`/path/to/${unicodeFilename}`);

      render(<AudioUploadSection />);
      fireEvent.click(screen.getByText('Select File'));

      await waitFor(() => {
        expect(screen.getByText(unicodeFilename)).toBeInTheDocument();
      });
    });

    it('handles network-like paths', async () => {
      const networkPath = '\\\\server\\share\\audio.wav';
      mockOpen.mockResolvedValue(networkPath);

      render(<AudioUploadSection />);
      fireEvent.click(screen.getByText('Select File'));

      await waitFor(() => {
        expect(screen.getByText('audio.wav')).toBeInTheDocument();
      });
    });

    it('handles empty drop payload gracefully', async () => {
      render(<AudioUploadSection />);

      const dropPayload = {
        paths: [],
        position: { x: 100, y: 200 },
      };

      (global as any).mockDragCallbacks.drop(dropPayload);

      // Should not crash or show error for empty drop
      expect(toast.error).not.toHaveBeenCalled();
    });

    it('handles malformed drag event payload', async () => {
      render(<AudioUploadSection />);

      // Drop with missing paths
      (global as any).mockDragCallbacks.drop({ position: { x: 100, y: 200 } });

      expect(toast.error).not.toHaveBeenCalled();
    });
  });

  describe('Performance and Memory', () => {
    it('cleans up event listeners on unmount', () => {
      const unlistenMocks = [vi.fn(), vi.fn(), vi.fn()];
      let callCount = 0;

      mockListen.mockImplementation(() =>
        Promise.resolve(unlistenMocks[callCount++])
      );

      const { unmount } = render(<AudioUploadSection />);
      unmount();

      // Event listeners should be cleaned up
      expect(mockListen).toHaveBeenCalledTimes(3);
    });

    it('handles rapid file selection changes', async () => {
      render(<AudioUploadSection />);

      // Rapidly select different files
      for (let i = 0; i < 5; i++) {
        mockOpen.mockResolvedValueOnce(`/path/to/file${i}.wav`);
        fireEvent.click(screen.getByText(i === 0 ? 'Select File' : 'Change'));

        await waitFor(() => {
          expect(screen.getByText(`file${i}.wav`)).toBeInTheDocument();
        });
      }
    });

    it('handles rapid transcription button clicks', async () => {
      mockOpen.mockResolvedValue('/path/to/test.wav');
      mockInvoke.mockImplementation(() => new Promise(resolve =>
        setTimeout(() => resolve('Result'), 100)
      ));

      render(<AudioUploadSection />);
      fireEvent.click(screen.getByText('Select File'));

      await waitFor(() => {
        const transcribeButton = screen.getByText('Transcribe');

        // Click multiple times rapidly
        fireEvent.click(transcribeButton);
        fireEvent.click(transcribeButton);
        fireEvent.click(transcribeButton);
      });

      // Should only invoke once due to disabled state during processing
      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledTimes(1);
      });
    });
  });

  describe('Accessibility', () => {
    it('has proper ARIA labels and roles', () => {
      render(<AudioUploadSection />);

      expect(screen.getByRole('button', { name: /Select File/i })).toBeInTheDocument();
      expect(screen.getByText('Important Information')).toBeInTheDocument();
    });

    it('provides keyboard navigation support', () => {
      render(<AudioUploadSection />);

      const selectButton = screen.getByRole('button', { name: /Select File/i });
      expect(selectButton).toBeVisible();
      expect(selectButton).not.toBeDisabled();
    });

    it('shows loading indicators with proper labels', async () => {
      mockOpen.mockResolvedValue('/path/to/test.wav');
      mockInvoke.mockImplementation(() => new Promise(() => {})); // Never resolves

      render(<AudioUploadSection />);
      fireEvent.click(screen.getByText('Select File'));

      await waitFor(() => {
        fireEvent.click(screen.getByText('Transcribe'));
      });

      await waitFor(() => {
        expect(screen.getByLabelText('Loading')).toBeInTheDocument();
      });
    });
  });

  describe('Integration with Settings', () => {
    it('respects different model settings', async () => {
      mockUseSettings.mockReturnValue({
        settings: {
          current_model: 'large-v3',
        },
      });

      mockOpen.mockResolvedValue('/path/to/test.wav');
      mockInvoke.mockResolvedValue('Transcription result');

      render(<AudioUploadSection />);
      fireEvent.click(screen.getByText('Select File'));

      await waitFor(() => {
        fireEvent.click(screen.getByText('Transcribe'));
      });

      expect(mockInvoke).toHaveBeenCalledWith('transcribe_audio_file', {
        filePath: '/path/to/test.wav',
        modelName: 'large-v3',
      });
    });

    it('handles missing settings gracefully', async () => {
      mockUseSettings.mockReturnValue({
        settings: null,
      });

      render(<AudioUploadSection />);

      // Should render without crashing
      expect(screen.getByText('Audio Upload')).toBeInTheDocument();
    });
  });
});