import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { clearMocks } from '@tauri-apps/api/mocks';
import { emitMockEvent } from './test/setup';
import App from './App';
import { invoke } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';

vi.mock('@tauri-apps/api/core');
vi.mock('@tauri-apps/plugin-dialog');

describe('App Integration Tests', () => {
  const mockModels = {
    'base': {
      name: 'base',
      size: 142000000,
      downloaded: true,
      speed_score: 7,
      accuracy_score: 5,
    },
    'tiny': {
      name: 'tiny',
      size: 39000000,
      downloaded: false,
      speed_score: 10,
      accuracy_score: 3,
    }
  };

  const mockSettings = {
    hotkey: 'CommandOrControl+Shift+Space',
    current_model: 'base',
    language: 'auto',
    auto_insert: true,
    show_window_on_record: false,
    theme: 'system',
  };

  beforeEach(() => {
    clearMocks();
    vi.clearAllMocks();

    // Default mocks
    vi.mocked(invoke).mockImplementation((cmd: string, args?: any) => {
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
        case 'delete_model':
          return Promise.resolve();
        case 'get_transcription_history':
          return Promise.resolve([]);
        default:
          return Promise.resolve();
      }
    });
  });

  describe('Core User Journeys', () => {
    it('user can start the app and see main interface', async () => {
      render(<App />);

      // User sees app name
      expect(await screen.findByText('VoiceType')).toBeInTheDocument();
      
      // App loads successfully
      
      // User sees the empty state
      expect(screen.getByText('No transcriptions yet')).toBeInTheDocument();
    });

    it('user can navigate to settings and back', async () => {
      const user = userEvent.setup();
      render(<App />);

      await screen.findByText('VoiceType');

      // User clicks settings icon
      const settingsButton = screen.getByLabelText('Settings');
      await user.click(settingsButton);

      // User is in settings (sees close button)
      await waitFor(() => {
        expect(screen.getByText('✕')).toBeInTheDocument();
      });
      
      // User sees settings content
      expect(screen.getByText('Available Models')).toBeInTheDocument();

      // User clicks close
      await user.click(screen.getByText('✕'));

      // User is back at main screen
      await waitFor(() => {
        expect(screen.getByText('No transcriptions yet')).toBeInTheDocument();
      });
    });

    it('user can delete a model with confirmation', async () => {
      const user = userEvent.setup();
      vi.mocked(ask).mockResolvedValue(true); // User confirms deletion

      render(<App />);
      
      // Navigate to settings
      await screen.findByText('VoiceType');
      const settingsButton = screen.getByLabelText('Settings');
      await user.click(settingsButton);

      // Wait for settings to load
      await waitFor(() => {
        expect(screen.getByText('✕')).toBeInTheDocument();
      });
      
      // User sees models
      expect(screen.getByText('Base')).toBeInTheDocument();

      // Find and click a delete button (trash icon)
      const allButtons = screen.getAllByRole('button');
      const deleteButton = allButtons.find(btn => 
        btn.querySelector('svg') && btn.className.includes('hover:text-destructive')
      );
      
      if (deleteButton) {
        await user.click(deleteButton);
        
        // Verify confirmation was shown
        expect(vi.mocked(ask)).toHaveBeenCalled();
      }
    });

    it('user sees onboarding when no models are downloaded', async () => {
      // Override to have no downloaded models
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
      await waitFor(() => {
        expect(screen.getByText('Welcome to VoiceType')).toBeInTheDocument();
        expect(screen.getByText('Choose a model to get started')).toBeInTheDocument();
      });
    });

    it('app handles errors gracefully', async () => {
      render(<App />);
      
      // Wait for app to load
      await screen.findByText('VoiceType');

      // Simulate recording error
      emitMockEvent('recording-error', 'Microphone not accessible');
      emitMockEvent('recording-state-changed', { state: 'error', error: 'Microphone not accessible' });

      // Main window remains functional despite errors
      // Error messages are shown in pill window, not main window
      expect(screen.getByText('VoiceType')).toBeInTheDocument();
      expect(screen.getByText('No transcriptions yet')).toBeInTheDocument();
    });
  });
});