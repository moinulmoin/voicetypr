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
      return Promise.resolve();
    });
  });

  it('User can record voice and get transcription', async () => {
    const user = userEvent.setup();
    render(<App />);

    // User sees they can record
    expect(await screen.findByText(/Press.*to record/i)).toBeInTheDocument();
    
    // User clicks the record button
    const startButton = screen.getByText('Start Recording');
    await user.click(startButton);

    // Backend will handle the recording, emit the events
    emitMockEvent('recording-state-changed', { state: 'recording', error: null });

    // User sees recording UI
    await waitFor(() => {
      expect(screen.getByText('Stop Recording')).toBeInTheDocument();
      expect(screen.getByText('Recording...')).toBeInTheDocument();
    });

    // User stops recording
    await user.click(screen.getByText('Stop Recording'));
    
    // Backend processes
    emitMockEvent('recording-state-changed', { state: 'transcribing', error: null });

    // User sees processing message
    await waitFor(() => {
      expect(screen.getByText('Transcribing your speech...')).toBeInTheDocument();
    });

    // Transcription completes
    emitMockEvent('transcription-complete', { 
      text: 'Hello world',
      model: 'base' 
    });

    // User sees their text
    await waitFor(() => {
      expect(screen.getByText('Hello world')).toBeInTheDocument();
    });
  });

  it('User sees helpful error when recording fails', async () => {
    render(<App />);

    await screen.findByText(/Press.*to record/i);

    // Something goes wrong
    emitMockEvent('recording-error', 'Microphone not found');
    emitMockEvent('recording-state-changed', { state: 'error', error: 'Microphone not found' });

    // User sees what went wrong
    await waitFor(() => {
      expect(screen.getByText(/Microphone not found/i)).toBeInTheDocument();
    });

    // User can try again
    expect(screen.getByText('Try Again')).toBeInTheDocument();
  });

  it('User can access settings', async () => {
    const user = userEvent.setup();
    render(<App />);

    // Wait for app to load
    await screen.findByText(/Press.*to record/i);

    // Find and click the settings button (it has the Settings icon)
    const settingsButton = screen.getByLabelText('Settings');
    await user.click(settingsButton);
    
    // User should see they're in settings now
    await waitFor(() => {
      // The close button appears in settings view
      expect(screen.getByText('✕')).toBeInTheDocument();
    });
    
    // User can see settings content
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
      return Promise.resolve();
    });

    render(<App />);

    // User should still see the app name
    await waitFor(() => {
      expect(screen.getByText('VoiceType')).toBeInTheDocument();
    });
    
    // User should still be able to record (the main functionality)
    expect(screen.getByText('Start Recording')).toBeInTheDocument();
  });
});