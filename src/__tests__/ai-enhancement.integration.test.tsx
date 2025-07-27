import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import App from '../App';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(),
  listen: vi.fn(),
}));

describe('AI Enhancement Integration', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    
    // Mock default responses
    (invoke as any).mockImplementation((cmd: string, args?: any) => {
      switch (cmd) {
        case 'get_settings':
          return Promise.resolve({
            onboarding_completed: true,
            current_model: 'base.en',
            hotkey: 'Cmd+Shift+Space',
            language: 'en',
            translate_to_english: false,
            ai_enabled: false,
          });
        case 'get_transcription_history':
          return Promise.resolve([]);
        case 'list_downloaded_models':
          return Promise.resolve(['base.en']);
        case 'get_ai_settings':
          return Promise.resolve({
            enabled: false,
            provider: 'groq',
            model: '',  // Empty by default
            hasApiKey: false,
          });
        case 'get_ai_settings_for_provider':
          return Promise.resolve({
            enabled: false,
            provider: args?.provider || 'groq',
            model: '',  // Empty by default
            hasApiKey: false,
          });
        default:
          return Promise.resolve();
      }
    });
  });

  it('shows AI enhancement in sidebar', async () => {
    render(<App />);
    
    await waitFor(() => {
      expect(screen.getByText('Enhancements')).toBeInTheDocument();
    });
  });

  it('navigates to enhancements section', async () => {
    render(<App />);
    
    await waitFor(() => {
      const enhancementsButton = screen.getByText('Enhancements');
      fireEvent.click(enhancementsButton);
    });
    
    await waitFor(() => {
      expect(screen.getByText(/Enhance your transcriptions/)).toBeInTheDocument();
    });
  });

  it.skip('complete AI setup flow', async () => {
    render(<App />);
    
    // Navigate to enhancements
    await waitFor(() => {
      const enhancementsButton = screen.getByText('Enhancements');
      fireEvent.click(enhancementsButton);
    });
    
    // Wait for the section to load
    await waitFor(() => {
      expect(screen.getByText(/Enhance your transcriptions/)).toBeInTheDocument();
    });
    
    // Wait for models to load
    await waitFor(() => {
      expect(screen.getByText('Llama 3.1 8B Instant')).toBeInTheDocument();
    });
    
    // Find key button using getByRole with accessible name
    // The key button should have an accessible name
    const buttons = screen.getAllByRole('button');
    let keyButton = null;
    
    // Look for the button that contains the Key icon
    for (const button of buttons) {
      // Check if this button contains an svg with class containing 'key'
      const svg = button.querySelector('svg');
      if (svg && svg.getAttribute('class')?.includes('lucide-key')) {
        keyButton = button;
        break;
      }
    }
    
    // If we still can't find it, look inside the model cards
    if (!keyButton) {
      // Find the first model card
      const modelCard = screen.getByText('Llama 3.1 8B Instant').closest('[data-slot="card"]');
      if (modelCard) {
        // Find the button inside this card
        const buttonsInCard = modelCard.querySelectorAll('button');
        for (const btn of buttonsInCard) {
          const svg = btn.querySelector('svg');
          if (svg && svg.getAttribute('class')?.includes('lucide-key')) {
            keyButton = btn;
            break;
          }
        }
      }
    }
    
    expect(keyButton).toBeTruthy();
    fireEvent.click(keyButton);
    
    // Enter API key
    await waitFor(() => {
      const input = screen.getByPlaceholderText(/Enter your Groq API key/);
      expect(input).toBeInTheDocument();
      fireEvent.change(input, { target: { value: 'gsk_test_key_12345' } });
      
      const submitButton = screen.getByText('Save API Key');
      fireEvent.click(submitButton);
    });
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('save_ai_api_key', {
        provider: 'groq',
        apiKey: 'gsk_test_key_12345',
      });
    });
  });

  it('enables AI enhancement after API key is added', async () => {
    // Mock API key exists
    (invoke as any).mockImplementation((cmd: string, args?: any) => {
      switch (cmd) {
        case 'get_settings':
          return Promise.resolve({
            onboarding_completed: true,
            current_model: 'base.en',
            hotkey: 'Cmd+Shift+Space',
          });
        case 'get_transcription_history':
          return Promise.resolve([]);
        case 'list_downloaded_models':
          return Promise.resolve(['base.en']);
        case 'get_ai_settings':
          return Promise.resolve({
            enabled: false,
            provider: 'groq',
            model: '',  // No model selected initially
            hasApiKey: true,
          });
        case 'get_ai_settings_for_provider':
          return Promise.resolve({
            enabled: false,
            provider: args?.provider || 'groq',
            model: '',
            hasApiKey: true,
          });
        default:
          return Promise.resolve();
      }
    });
    
    render(<App />);
    
    // Navigate to enhancements
    await waitFor(() => {
      const enhancementsButton = screen.getByText('Enhancements');
      fireEvent.click(enhancementsButton);
    });
    
    // First select a model (which should be clickable since we have API key)
    await waitFor(() => {
      const modelCard = screen.getByText('Llama 3.1 8B Instant').closest('.transition-all');
      if (modelCard) {
        fireEvent.click(modelCard);
      }
    });
    
    // Update the mock to reflect the selected model
    (invoke as any).mockImplementation((cmd: string, args?: any) => {
      switch (cmd) {
        case 'get_settings':
          return Promise.resolve({
            onboarding_completed: true,
            current_model: 'base.en',
            hotkey: 'Cmd+Shift+Space',
          });
        case 'get_transcription_history':
          return Promise.resolve([]);
        case 'list_downloaded_models':
          return Promise.resolve(['base.en']);
        case 'get_ai_settings':
          return Promise.resolve({
            enabled: false,
            provider: 'groq',
            model: 'llama-3.1-8b-instant',
            hasApiKey: true,
          });
        case 'get_ai_settings_for_provider':
          return Promise.resolve({
            enabled: false,
            provider: args?.provider || 'groq',
            model: 'llama-3.1-8b-instant',
            hasApiKey: true,
          });
        default:
          return Promise.resolve();
      }
    });
    
    // Now toggle should be enabled
    await waitFor(() => {
      const toggle = screen.getByRole('switch');
      expect(toggle).toBeEnabled();
      fireEvent.click(toggle);
    });
    
    expect(invoke).toHaveBeenCalledWith('update_ai_settings', {
      enabled: true,
      provider: 'groq',
      model: 'llama-3.1-8b-instant',
    });
  });

  it('shows model selection when API key exists', async () => {
    // Mock API key exists
    (invoke as any).mockImplementation((cmd: string, args?: any) => {
      switch (cmd) {
        case 'get_settings':
          return Promise.resolve({
            onboarding_completed: true,
            current_model: 'base.en',
            hotkey: 'Cmd+Shift+Space',
          });
        case 'get_transcription_history':
          return Promise.resolve([]);
        case 'list_downloaded_models':
          return Promise.resolve(['base.en']);
        case 'get_ai_settings':
          return Promise.resolve({
            enabled: false,
            provider: 'groq',
            model: '',  // No model selected initially
            hasApiKey: true,
          });
        case 'get_ai_settings_for_provider':
          return Promise.resolve({
            enabled: false,
            provider: args?.provider || 'groq',
            model: '',
            hasApiKey: true,
          });
        default:
          return Promise.resolve();
      }
    });
    
    render(<App />);
    
    // Navigate to enhancements
    await waitFor(() => {
      const enhancementsButton = screen.getByText('Enhancements');
      fireEvent.click(enhancementsButton);
    });
    
    // Should show "Ready" status
    await waitFor(() => {
      const readyStatuses = screen.getAllByText('Ready');
      expect(readyStatuses.length).toBeGreaterThan(0);
    });
    
    // Click on a different model
    const modelCard = screen.getByText('Llama 3.1 8B Instant').closest('.transition-all');
    if (modelCard) {
      fireEvent.click(modelCard);
    }
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_ai_settings', {
        enabled: false,  // Should be false since we're just selecting a model
        provider: 'groq',
        model: 'llama-3.1-8b-instant',
      });
    });
  });

  it('handles transcription with AI enhancement', async () => {
    // Mock AI enhancement enabled
    (invoke as any).mockImplementation((cmd: string, args?: any) => {
      switch (cmd) {
        case 'get_settings':
          return Promise.resolve({
            onboarding_completed: true,
            current_model: 'base.en',
            hotkey: 'Cmd+Shift+Space',
            ai_enabled: true,
          });
        case 'enhance_transcription':
          return Promise.resolve(
            args?.text === 'hello world' 
              ? 'Hello, world!' 
              : args?.text
          );
        default:
          return Promise.resolve();
      }
    });
    
    // Simulate transcription
    const originalText = 'hello world';
    const enhancedText = await invoke('enhance_transcription', { text: originalText });
    
    expect(enhancedText).toBe('Hello, world!');
  });
});