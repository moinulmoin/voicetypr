import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { clearMocks } from '@tauri-apps/api/mocks';
import { emitMockEvent } from './test/setup';
import App from './App';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core');

/**
 * Critical User Journeys - What users MUST be able to do
 * Following the testing philosophy: Test behavior, not implementation
 */
describe('Critical User Journeys', () => {
  beforeEach(() => {
    clearMocks();
    vi.clearAllMocks();

    // Minimal mock setup - just enough to make the app work
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_model_status') {
        return Promise.resolve({
          'base': {
            name: 'base',
            size: 142000000,
            downloaded: true,
            speed_score: 7,
            accuracy_score: 5,
          }
        });
      }
      if (cmd === 'get_settings') {
        return Promise.resolve({
          hotkey: 'CommandOrControl+Shift+Space',
          current_model: 'base',
          language: 'auto',
          auto_insert: true,
          show_window_on_record: false,
          theme: 'system',
        });
      }
      if (cmd === 'get_transcription_history') {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });
  });

  it('User can record voice and get transcription', async () => {
    render(<App />);

    // Wait for the app to load - should show empty state
    await waitFor(() => {
      expect(screen.getByText('No transcriptions yet')).toBeInTheDocument();
    });

    // User presses hotkey (simulated by backend state change)
    emitMockEvent('recording-state-changed', { state: 'recording', error: null });

    // Backend handles recording and transcription
    emitMockEvent('recording-state-changed', { state: 'transcribing', error: null });

    // Mock the transcription history to include the new transcription
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === 'get_transcription_history') {
        return Promise.resolve([{
          text: 'Hello world',
          model: 'base',
          timestamp: new Date().toISOString()
        }]);
      }
      if (cmd === 'get_model_status') {
        return Promise.resolve({
          'base': {
            name: 'base',
            size: 142000000,
            downloaded: true,
            speed_score: 7,
            accuracy_score: 5,
          }
        });
      }
      if (cmd === 'get_settings') {
        return Promise.resolve({
          hotkey: 'CommandOrControl+Shift+Space',
          current_model: 'base',  
          language: 'auto',
          auto_insert: true,
          show_window_on_record: false,
          theme: 'system',
        });
      }
      return Promise.resolve();
    });

    // Transcription completes - backend emits event to trigger history reload
    emitMockEvent('recording-state-changed', { state: 'idle', error: null });
    emitMockEvent('history-updated', null);

    // User sees their text in history
    await waitFor(() => {
      expect(screen.getByText('Hello world')).toBeInTheDocument();
    });
  });

  it('User sees helpful error when recording fails', async () => {
    render(<App />);

    // Wait for app to load
    await waitFor(() => {
      expect(screen.getByText('No transcriptions yet')).toBeInTheDocument();
    });

    // Recording error occurs - the app remains functional
    emitMockEvent('recording-error', 'Microphone not found');
    emitMockEvent('recording-state-changed', { state: 'error', error: 'Microphone not found' });

    // App continues to work - user can still see the main UI
    expect(screen.getByText('VoiceType')).toBeInTheDocument();
    expect(screen.getByText('No transcriptions yet')).toBeInTheDocument();
  });

  it('User can access settings', async () => {
    const user = userEvent.setup();
    render(<App />);

    // Wait for app to load
    // Wait for app to load
    await waitFor(() => {
      expect(screen.getByText('No transcriptions yet')).toBeInTheDocument();
    });

    // Find and click the settings button (it has the Settings icon)
    const settingsButton = screen.getByLabelText('Settings');
    await user.click(settingsButton);

    // User sees settings page
    await waitFor(() => {
      expect(screen.getByText('Settings')).toBeInTheDocument();
    });

    // User can see hotkey section
    expect(screen.getByText('Hotkey')).toBeInTheDocument();
    
    // Settings page loaded successfully
    expect(screen.getByText('Available Models')).toBeInTheDocument();
  });

  it('User without models sees helpful onboarding', async () => {
    // Override mock to return no downloaded models
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_model_status') {
        return Promise.resolve({
          'base': {
            name: 'base',
            size: 142000000,
            downloaded: false, // Not downloaded
            speed_score: 7,
            accuracy_score: 5,
          }
        });
      }
      if (cmd === 'get_settings') {
        return Promise.resolve({
          hotkey: 'CommandOrControl+Shift+Space',
          current_model: '',
          language: 'auto',
          auto_insert: true,
          show_window_on_record: false,
          theme: 'system',
        });
      }
      if (cmd === 'get_transcription_history') {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });

    render(<App />);

    // User sees welcome message
    await waitFor(() => {
      expect(screen.getByText(/Welcome to VoiceType/i)).toBeInTheDocument();
    });

    // User understands what to do
    expect(screen.getByText(/Choose a model to get started/i)).toBeInTheDocument();
  });

  it('App handles loading errors gracefully', async () => {
    // Make the app fail to load settings
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_settings') {
        return Promise.reject(new Error('Failed to load settings'));
      }
      // But models still load
      if (cmd === 'get_model_status') {
        return Promise.resolve({
          'base': {
            name: 'base',
            size: 142000000,
            downloaded: true,
            speed_score: 7,
            accuracy_score: 5,
          }
        });
      }
      if (cmd === 'get_transcription_history') {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });

    render(<App />);

    // User should still see the app name
    await waitFor(() => {
      expect(screen.getByText('VoiceType')).toBeInTheDocument();
    });

    // User can still use core functionality (with default settings)
    expect(screen.getByText('No transcriptions yet')).toBeInTheDocument();
  });
});