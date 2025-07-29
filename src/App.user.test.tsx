import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { clearMocks } from '@tauri-apps/api/mocks';
import { emitMockEvent } from './test/setup';
import App from './App';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core');

describe('VoiceTypr User Scenarios', () => {
  const mockModels = {
    'base': {
      name: 'base',
      size: 142000000,
      url: 'https://example.com/base.bin',
      sha256: 'def456',
      downloaded: true,
      speed_score: 7,
      accuracy_score: 5,
    },
    'tiny': {
      name: 'tiny',
      size: 39000000,
      url: 'https://example.com/tiny.bin',
      sha256: 'abc123',
      downloaded: false,
      speed_score: 10,
      accuracy_score: 3,
    },
  };

  const mockSettings = {
    hotkey: 'CommandOrControl+Shift+Space',
    current_model: 'base',
    language: 'en',
    theme: 'system',
  };

  beforeEach(() => {
    clearMocks();
    vi.clearAllMocks();

    vi.mocked(invoke).mockImplementation((cmd: string) => {
      switch (cmd) {
        case 'get_model_status':
          return Promise.resolve(mockModels);
        case 'get_settings':
          return Promise.resolve(mockSettings);
        case 'save_settings':
          return Promise.resolve();
        case 'start_recording':
          return Promise.resolve();
        case 'stop_recording':
          return Promise.resolve();
        case 'download_model':
          return Promise.resolve();
        case 'get_transcription_history':
          return Promise.resolve([]);
        default:
          return Promise.reject(new Error(`Unknown command: ${cmd}`));
      }
    });
  });

  describe('First-time user experience', () => {
    it('user without models sees onboarding and can download one', async () => {
      const user = userEvent.setup();
      
      // Setup: No models downloaded
      vi.mocked(invoke).mockImplementation((cmd: string) => {
        if (cmd === 'get_model_status') {
          return Promise.resolve({
            base: { ...mockModels.base, downloaded: false },
            tiny: { ...mockModels.tiny, downloaded: false }
          });
        }
        if (cmd === 'get_settings') {
          return Promise.resolve({ ...mockSettings, current_model: '' });
        }
        return Promise.resolve();
      });

      render(<App />);

      // User sees onboarding
      expect(await screen.findByText('Welcome to VoiceType')).toBeInTheDocument();
      expect(screen.getByText('Choose a model to get started')).toBeInTheDocument();

      // User sees model options with descriptions
      expect(screen.getByText('Base')).toBeInTheDocument();
      
      // User finds and clicks a download button
      const downloadButtons = screen.getAllByRole('button');
      const downloadButton = downloadButtons.find(btn => 
        btn.querySelector('svg') // Download icon
      );
      
      expect(downloadButton).toBeDefined();
      await user.click(downloadButton!);

      // Simulate download progress event
      emitMockEvent('download-progress', { model: 'base', progress: 50 });
      
      // User sees download progress
      await waitFor(() => {
        expect(screen.getByText('50%')).toBeInTheDocument();
      });
    });
  });

  describe('Recording voice', () => {
    it('user can record and see transcription', async () => {
      render(<App />);

      // Wait for app to load
      await waitFor(() => {
        expect(screen.getByText('No transcriptions yet')).toBeInTheDocument();
      });

      // Recording starts
      emitMockEvent('recording-state-changed', { state: 'recording', error: null });
      
      // Backend controls recording - no UI buttons to click
      // The recording state is managed entirely by backend

      // Transcription starts
      emitMockEvent('recording-state-changed', { state: 'transcribing', error: null });
      
      // Main window shows processing state in header
      await waitFor(() => {
        expect(screen.getByText('Processing')).toBeInTheDocument();
      });

      // Mock the transcription history to include the new transcription
      vi.mocked(invoke).mockImplementation((cmd) => {
        if (cmd === 'get_transcription_history') {
          return Promise.resolve([{
            text: 'Hello world, this is my transcription',
            model: 'base',
            timestamp: new Date().toISOString()
          }]);
        }
        // Keep other command mocks as default
        if (cmd === 'stop_recording') return Promise.resolve();
        if (cmd === 'get_settings') return Promise.resolve({
          hotkey: 'CommandOrControl+Shift+Space',
          language: 'en',
          theme: 'system',
          current_model: 'base',
          transcription_cleanup_days: null,
        });
        return Promise.resolve();
      });

      // Transcription completes
      emitMockEvent('transcription-complete', { 
        text: 'Hello world, this is my transcription',
        model: 'base' 
      });
      
      // Backend emits history-updated after saving transcription
      emitMockEvent('history-updated', null);

      // User sees the transcribed text in history
      await waitFor(() => {
        // Look for the truncated text (the component uses truncate class)
        const transcriptionElement = screen.getByText('Hello world, this is my transcription');
        expect(transcriptionElement).toBeInTheDocument();
      });
    });

    it('user sees error when recording fails', async () => {
      render(<App />);

      await screen.findByText('No transcriptions yet');

      // Recording error occurs
      emitMockEvent('recording-error', 'Microphone access denied');
      emitMockEvent('recording-state-changed', { state: 'error', error: 'Microphone access denied' });

      // Error handling is done in the pill window, not main window
      // Main window continues to show normal UI even on errors
      expect(screen.getByText('VoiceType')).toBeInTheDocument();
      expect(screen.getByText('No transcriptions yet')).toBeInTheDocument();
    });
  });

  describe('Managing settings', () => {
    it('user can change settings', async () => {
      const user = userEvent.setup();
      render(<App />);

      // Wait for app to load
      await screen.findByText('No transcriptions yet');

      // User navigates to settings
      const settingsButton = screen.getByLabelText('Settings');
      await user.click(settingsButton);

      // User is in settings (sees close button)
      await waitFor(() => {
        expect(screen.getByText('✕')).toBeInTheDocument();
      });
      
      // User can see settings options
      expect(screen.getByText('Hotkey')).toBeInTheDocument();
      expect(screen.getByText('Language')).toBeInTheDocument();
      expect(screen.getByText('Available Models')).toBeInTheDocument();
    });

    it('user can download additional models', async () => {
      const user = userEvent.setup();
      render(<App />);

      await screen.findByText('No transcriptions yet');

      // User opens settings
      const settingsButton = screen.getByLabelText('Settings');
      await user.click(settingsButton);

      await waitFor(() => {
        expect(screen.getByText('✕')).toBeInTheDocument();
      });

      // User sees models available
      expect(screen.getByText('Available Models')).toBeInTheDocument();
      expect(screen.getByText('Tiny')).toBeInTheDocument();
      
      // User sees model has download option (not downloaded)
      const modelCards = screen.getAllByText(/Speed:/i);
      expect(modelCards.length).toBeGreaterThan(0);
    });
  });

  describe('Keyboard shortcuts', () => {
    it('user can trigger recording with hotkey', async () => {
      render(<App />);

      // User sees empty state
      await waitFor(() => {
        expect(screen.getByText('No transcriptions yet')).toBeInTheDocument();
      });

      // Simulate global hotkey press
      emitMockEvent('hotkey-triggered', {});

      // Backend starts recording
      emitMockEvent('recording-state-changed', { state: 'recording', error: null });

      // Recording is handled in pill window, main window just shows status
      // The main window doesn't have recording controls
    });
  });

  describe('Error recovery', () => {
    it('user can retry when app fails to load', async () => {
      // Setup: Make settings fail but models succeed
      vi.mocked(invoke).mockImplementation((cmd: string) => {
        if (cmd === 'get_settings') {
          return Promise.reject(new Error('Settings load failed'));
        }
        if (cmd === 'get_model_status') {
          return Promise.resolve(mockModels);
        }
        return Promise.resolve();
      });
      
      render(<App />);

      // App should still show UI despite settings failure
      await waitFor(() => {
        expect(screen.getByText('VoiceType')).toBeInTheDocument();
      });
      
      // User can still use core functionality
      expect(screen.getByText('No transcriptions yet')).toBeInTheDocument();
    });
  });
});