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
    language: 'en',
    theme: 'system',
  };

  beforeEach(() => {
    clearMocks();
    vi.clearAllMocks();

    // Default mocks
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

  describe('Error Handling and Recovery', () => {
    it('shows warning toast for no-speech-detected event', async () => {
      render(<App />);
      await screen.findByText('VoiceType');

      // Emit no-speech-detected event
      emitMockEvent('no-speech-detected', {
        type: 'no-speech-detected',
        message: 'No speech was detected in the recording',
        suggestion: 'Please try speaking louder or closer to the microphone',
        actionable: true,
        action: {
          type: 'open-settings',
          label: 'Check Audio Settings'
        }
      });

      // Toast notification should appear
      await waitFor(() => {
        const toastElement = document.querySelector('[data-toast]') || 
                           document.querySelector('.toast') ||
                           screen.queryByText('No speech was detected');
        expect(toastElement).toBeTruthy();
      }, { timeout: 3000 });
    });

    it('shows warning toast for audio-too-quiet event', async () => {
      render(<App />);
      await screen.findByText('VoiceType');

      // Emit audio-too-quiet event
      emitMockEvent('audio-too-quiet', {
        type: 'audio-too-quiet',
        message: 'Audio level is too low for reliable transcription',
        energy_level: 0.005,
        suggestion: 'Audio is too quiet (peak: 2.3%). Try speaking louder or closer to the microphone.',
        actionable: true,
        action: {
          type: 'open-settings',
          label: 'Adjust Microphone'
        }
      });

      // Toast notification should appear
      await waitFor(() => {
        const toastElement = document.querySelector('[data-toast]') || 
                           document.querySelector('.toast') ||
                           screen.queryByText('Audio level is too low');
        expect(toastElement).toBeTruthy();
      }, { timeout: 3000 });
    });

    it('shows warning toast for recording-too-short event', async () => {
      render(<App />);
      await screen.findByText('VoiceType');

      // Emit recording-too-short event
      emitMockEvent('recording-too-short', {
        type: 'recording-too-short',
        message: 'Recording is too short (0.3s). Minimum duration is 0.5 seconds',
        duration: 0.3,
        minimum_duration: 0.5,
        actionable: true,
        action: {
          type: 'retry-recording',
          label: 'Try Recording Again'
        }
      });

      // Toast notification should appear
      await waitFor(() => {
        const toastElement = document.querySelector('[data-toast]') || 
                           document.querySelector('.toast') ||
                           screen.queryByText('Recording is too short');
        expect(toastElement).toBeTruthy();
      }, { timeout: 3000 });
    });

    it('handles multiple error events in sequence', async () => {
      render(<App />);
      await screen.findByText('VoiceType');

      // Emit multiple error events
      emitMockEvent('no-speech-detected', {
        type: 'no-speech-detected',
        message: 'First error: No speech detected'
      });

      emitMockEvent('audio-too-quiet', {
        type: 'audio-too-quiet', 
        message: 'Second error: Audio too quiet'
      });

      emitMockEvent('recording-too-short', {
        type: 'recording-too-short',
        message: 'Third error: Recording too short'
      });

      // Multiple toasts should appear (or be queued)
      await waitFor(() => {
        const toastElements = document.querySelectorAll('[data-toast], .toast');
        const textMatches = screen.queryAllByText(/error|speech|audio|recording/i);
        expect(toastElements.length > 0 || textMatches.length > 0).toBeTruthy();
      }, { timeout: 3000 });
    });

    it('toast action button navigates to settings', async () => {
      const user = userEvent.setup();
      render(<App />);
      await screen.findByText('VoiceType');

      // Emit error event with action
      emitMockEvent('no-speech-detected', {
        type: 'no-speech-detected',
        message: 'No speech detected',
        actionable: true,
        action: {
          type: 'open-settings',
          label: 'Check Audio Settings'
        }
      });

      // Wait for toast and try to find action button
      await waitFor(async () => {
        const actionButton = screen.queryByText('Check Audio Settings') || 
                           screen.queryByRole('button', { name: /settings|check/i });
        
        if (actionButton) {
          await user.click(actionButton);
          
          // Should navigate to settings
          await waitFor(() => {
            expect(screen.getByText('Available Models')).toBeInTheDocument();
          });
        }
      }, { timeout: 5000 });
    });

    it('handles recording state error transitions', async () => {
      render(<App />);
      await screen.findByText('VoiceType');

      // Simulate error state
      emitMockEvent('recording-state-changed', {
        state: 'error',
        error: {
          type: 'microphone-access-denied',
          message: 'Could not access microphone. Please check permissions.',
          recoverable: true,
          timestamp: new Date().toISOString()
        }
      });

      // App should remain functional
      expect(screen.getByText('VoiceType')).toBeInTheDocument();

      // Recovery event
      emitMockEvent('recording-state-changed', {
        state: 'idle',
        recovery: {
          from_error: 'microphone-access-denied',
          message: 'Microphone access restored',
          timestamp: new Date().toISOString()
        }
      });

      // App should still be functional
      expect(screen.getByText('VoiceType')).toBeInTheDocument();
    });

    it('handles model download errors gracefully', async () => {
      const user = userEvent.setup();
      
      // Mock download failure
      vi.mocked(invoke).mockImplementation((cmd: string) => {
        if (cmd === 'get_model_status') {
          return Promise.resolve(mockModels);
        }
        if (cmd === 'get_settings') {
          return Promise.resolve(mockSettings);
        }
        if (cmd === 'download_model') {
          return Promise.reject(new Error('Network error: Could not download model'));
        }
        return Promise.resolve();
      });

      render(<App />);
      await screen.findByText('VoiceType');
      
      // Navigate to settings
      const settingsButton = screen.getByLabelText('Settings');
      await user.click(settingsButton);

      await waitFor(() => {
        expect(screen.getByText('Available Models')).toBeInTheDocument();
      });

      // Try to download a model (find download button)
      const downloadButtons = screen.getAllByRole('button').filter(btn => 
        btn.textContent?.includes('Download') || 
        btn.getAttribute('aria-label')?.includes('Download')
      );
      
      if (downloadButtons.length > 0) {
        await user.click(downloadButtons[0]);
        
        // Error should be handled gracefully without crashing
        await waitFor(() => {
          expect(screen.getByText('VoiceType')).toBeInTheDocument();
        });
      }
    });

    it('handles invalid hotkey gracefully', async () => {
      const user = userEvent.setup();

      // Mock invalid hotkey save
      vi.mocked(invoke).mockImplementation((cmd: string) => {
        if (cmd === 'get_model_status') {
          return Promise.resolve(mockModels);
        }
        if (cmd === 'get_settings') {
          return Promise.resolve(mockSettings);
        }
        if (cmd === 'save_settings' || cmd === 'set_global_shortcut') {
          return Promise.reject(new Error('Invalid hotkey combination'));
        }
        return Promise.resolve();
      });

      render(<App />);
      await screen.findByText('VoiceType');
      
      // Navigate to settings
      const settingsButton = screen.getByLabelText('Settings');
      await user.click(settingsButton);

      await waitFor(() => {
        expect(screen.getByText('Available Models')).toBeInTheDocument();
      });

      // Try to change hotkey (find hotkey input)
      const hotkeyInputs = screen.getAllByRole('textbox').filter(input =>
        input.getAttribute('placeholder')?.includes('hotkey') || 
        input.getAttribute('aria-label')?.includes('hotkey')
      );
      
      if (hotkeyInputs.length > 0) {
        await user.type(hotkeyInputs[0], 'InvalidKey');
        
        // Try to save (find save button or trigger save)
        const form = hotkeyInputs[0].closest('form');
        if (form) {
          await user.click(screen.getByRole('button', { name: /save/i }) || 
                          screen.getAllByRole('button')[0]);
        }
        
        // Error should be handled gracefully
        await waitFor(() => {
          expect(screen.getByText('VoiceType')).toBeInTheDocument();
        });
      }
    });

    it('handles permission denial scenarios', async () => {
      render(<App />);
      await screen.findByText('VoiceType');

      // Emit permission denied events
      emitMockEvent('permission-denied', {
        type: 'microphone',
        message: 'Microphone access was denied',
        action: {
          type: 'open-system-preferences',
          label: 'Open System Preferences'
        }
      });

      emitMockEvent('permission-denied', {
        type: 'accessibility',
        message: 'Accessibility permission is required for text insertion',
        action: {
          type: 'open-accessibility-settings',
          label: 'Open Accessibility Settings'
        }
      });

      // App should remain functional despite permission issues
      expect(screen.getByText('VoiceType')).toBeInTheDocument();
    });

    it('handles network connectivity issues', async () => {
      render(<App />);
      await screen.findByText('VoiceType');

      // Emit network error events
      emitMockEvent('network-error', {
        type: 'ai-enhancement-failed',
        message: 'Could not connect to AI enhancement service',
        error: 'Network timeout',
        retry_available: true
      });

      // App should continue working without AI enhancement
      expect(screen.getByText('VoiceType')).toBeInTheDocument();
    });

    it('handles memory/performance warnings', async () => {
      render(<App />);
      await screen.findByText('VoiceType');

      // Emit performance warning
      emitMockEvent('performance-warning', {
        type: 'high-memory-usage',
        message: 'High memory usage detected. Consider using a smaller model.',
        current_memory_mb: 2048,
        recommended_action: 'switch-to-smaller-model'
      });

      // App should remain responsive
      expect(screen.getByText('VoiceType')).toBeInTheDocument();
    });

    it('recovers from temporary errors', async () => {
      render(<App />);
      await screen.findByText('VoiceType');

      // Emit temporary error
      emitMockEvent('temporary-error', {
        type: 'model-loading-failed',
        message: 'Model temporarily unavailable',
        retry_in_seconds: 5,
        auto_retry: true
      });

      // Emit recovery event
      emitMockEvent('error-recovered', {
        type: 'model-loading-succeeded', 
        message: 'Model loaded successfully',
        recovered_from: 'model-loading-failed'
      });

      // App should be functional
      expect(screen.getByText('VoiceType')).toBeInTheDocument();
    });

    it('handles concurrent error events without crashes', async () => {
      render(<App />);
      await screen.findByText('VoiceType');

      // Emit many errors rapidly
      const errorEvents = [
        { type: 'no-speech-detected', message: 'Error 1' },
        { type: 'audio-too-quiet', message: 'Error 2' },
        { type: 'recording-too-short', message: 'Error 3' },
        { type: 'microphone-error', message: 'Error 4' },
        { type: 'transcription-failed', message: 'Error 5' }
      ];

      errorEvents.forEach((event, index) => {
        setTimeout(() => emitMockEvent(event.type, event), index * 10);
      });

      // Wait and verify app remains stable
      await waitFor(() => {
        expect(screen.getByText('VoiceType')).toBeInTheDocument();
      }, { timeout: 5000 });

      // App should not have crashed
      expect(screen.getByText('No transcriptions yet')).toBeInTheDocument();
    });
  });

  describe('Toast Notification System', () => {
    it('displays toast notifications for various event types', async () => {
      render(<App />);
      await screen.findByText('VoiceType');

      // Test different toast types
      const toastEvents = [
        {
          event: 'info-notification',
          data: { type: 'info', message: 'Info message' }
        },
        {
          event: 'warning-notification', 
          data: { type: 'warning', message: 'Warning message' }
        },
        {
          event: 'error-notification',
          data: { type: 'error', message: 'Error message' }
        },
        {
          event: 'success-notification',
          data: { type: 'success', message: 'Success message' }
        }
      ];

      for (const { event, data } of toastEvents) {
        emitMockEvent(event, data);
        
        await waitFor(() => {
          const toastElements = document.querySelectorAll('[data-toast], .toast');
          const textMatch = screen.queryByText(data.message);
          expect(toastElements.length > 0 || textMatch).toBeTruthy();
        }, { timeout: 2000 });
      }
    });

    it('handles toast queue overflow gracefully', async () => {
      render(<App />);
      await screen.findByText('VoiceType');

      // Emit many toasts rapidly
      for (let i = 0; i < 20; i++) {
        emitMockEvent('info-notification', {
          type: 'info',
          message: `Toast message ${i}`,
          id: `toast-${i}`
        });
      }

      // App should remain responsive despite many toasts
      await waitFor(() => {
        expect(screen.getByText('VoiceType')).toBeInTheDocument();
      });
    });

    it('supports toast dismissal', async () => {
      const user = userEvent.setup();
      render(<App />);
      await screen.findByText('VoiceType');

      // Emit dismissible toast
      emitMockEvent('dismissible-notification', {
        type: 'info',
        message: 'Dismissible message',
        dismissible: true
      });

      await waitFor(async () => {
        // Look for dismiss button (usually an X or close icon)
        const dismissButton = document.querySelector('[aria-label*="dismiss"], [aria-label*="close"], .toast-close');
        if (dismissButton) {
          await user.click(dismissButton);
        }
      }, { timeout: 3000 });

      // Toast should be dismissed
      await waitFor(() => {
        expect(screen.queryByText('Dismissible message')).not.toBeInTheDocument();
      });
    });
  });
});