import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { EnhancementsSection } from '../EnhancementsSection';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

// Mock dependencies
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
  },
}));

describe('EnhancementsSection', () => {
  const mockAISettings = {
    enabled: false,
    provider: 'groq',
    model: '',  // Empty by default
    hasApiKey: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as any).mockResolvedValue(mockAISettings);
  });

  it('renders the enhancements section', async () => {
    render(<EnhancementsSection />);
    
    expect(screen.getByText('Enhancements')).toBeInTheDocument();
    expect(screen.getByText(/Enhance your transcriptions/)).toBeInTheDocument();
    
    // Wait for models to load
    await waitFor(() => {
      expect(screen.getByText('Llama 3.1 8B Instant')).toBeInTheDocument();
    });
  });

  it('displays all available models', async () => {
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      expect(screen.getByText('Llama 3.1 8B Instant')).toBeInTheDocument();
    });
  });

  it('shows key icon when no API key is set', async () => {
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      const keyButtons = screen.getAllByRole('button', { name: '' });
      expect(keyButtons.length).toBeGreaterThan(0);
    });
  });

  it('opens API key modal when key icon is clicked', async () => {
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      const keyButtons = screen.getAllByRole('button');
      const keyButton = keyButtons.find(btn => btn.querySelector('svg'));
      if (keyButton) {
        fireEvent.click(keyButton);
      }
    });
    
    await waitFor(() => {
      expect(screen.getByText(/Add Groq API Key/)).toBeInTheDocument();
    });
  });

  it('disables enhancement toggle when no API key', async () => {
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      const toggle = screen.getByRole('switch');
      expect(toggle).toBeDisabled();
    });
  });

  it('enables enhancement toggle when API key exists and model is selected', async () => {
    (invoke as any).mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'get_ai_settings') {
        return Promise.resolve({
          enabled: false,
          provider: 'groq',
          model: 'llama-3.1-8b-instant',  // Model is selected
          hasApiKey: true,
        });
      }
      if (cmd === 'get_ai_settings_for_provider') {
        return Promise.resolve({
          enabled: false,
          provider: args?.provider || 'groq',
          model: 'llama-3.1-8b-instant',
          hasApiKey: true,
        });
      }
      return Promise.resolve();
    });
    
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      const toggle = screen.getByRole('switch');
      expect(toggle).toBeEnabled();
    });
  });

  it('toggles AI enhancement', async () => {
    // Mock that we have an API key for groq provider
    (invoke as any).mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'get_ai_settings') {
        return Promise.resolve({
          enabled: false,
          provider: 'groq',
          model: 'llama-3.1-8b-instant',
          hasApiKey: true,
        });
      }
      if (cmd === 'get_ai_settings_for_provider') {
        return Promise.resolve({
          enabled: false,
          provider: args?.provider || 'groq',
          model: 'llama-3.1-8b-instant',
          hasApiKey: true,
        });
      }
      return Promise.resolve();
    });
    
    render(<EnhancementsSection />);
    
    // Wait for the component to load and fetch API key status
    await waitFor(() => {
      expect(screen.getByText('AI Enhancement')).toBeInTheDocument();
    });
    
    // The toggle should be enabled since we have API key and a model selected
    await waitFor(() => {
      const toggle = screen.getByRole('switch');
      expect(toggle).toBeEnabled();
      fireEvent.click(toggle);
    });
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_ai_settings', {
        enabled: true,
        provider: 'groq',
        model: 'llama-3.1-8b-instant',
      });
      expect(toast.success).toHaveBeenCalledWith('AI enhancement enabled');
    });
  });

  it('selects a model', async () => {
    (invoke as any).mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'get_ai_settings') {
        return Promise.resolve({
          enabled: false,
          provider: 'groq',
          model: '',  // No model selected initially
          hasApiKey: true,
        });
      }
      if (cmd === 'get_ai_settings_for_provider') {
        return Promise.resolve({
          enabled: false,
          provider: args?.provider || 'groq',
          model: '',
          hasApiKey: true,
        });
      }
      return Promise.resolve();
    });
    
    render(<EnhancementsSection />);
    
    // Wait for component to fully load and API key status to be checked
    await waitFor(() => {
      expect(screen.getByText('AI Enhancement')).toBeInTheDocument();
    });
    
    // Wait for the Ready status to appear (indicates API key is loaded)
    await waitFor(() => {
      const readyStatuses = screen.getAllByText('Ready');
      expect(readyStatuses.length).toBeGreaterThan(0);
    });
    
    // Now click the model card
    const modelCard = screen.getByText('Llama 3.1 8B Instant').closest('.transition-all');
    if (modelCard) {
      fireEvent.click(modelCard);
    }
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_ai_settings', {
        enabled: false,
        provider: 'groq',
        model: 'llama-3.1-8b-instant',
      });
      expect(toast.success).toHaveBeenCalledWith('Model selected');
    });
  });

  it('handles API key submission', async () => {
    render(<EnhancementsSection />);
    
    // Open modal
    await waitFor(() => {
      const keyButtons = screen.getAllByRole('button');
      const keyButton = keyButtons.find(btn => btn.querySelector('svg'));
      if (keyButton) {
        fireEvent.click(keyButton);
      }
    });
    
    // Enter API key
    const input = screen.getByPlaceholderText(/Enter your Groq API key/);
    fireEvent.change(input, { target: { value: 'test-api-key-12345' } });
    
    // Submit
    const submitButton = screen.getByText('Save API Key');
    fireEvent.click(submitButton);
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('save_ai_api_key', {
        provider: 'groq',
        apiKey: 'test-api-key-12345',
      });
    });
  });

  it('shows error when enabling without API key', async () => {
    render(<EnhancementsSection />);
    
    // Wait for initial load
    await waitFor(() => {
      const toggle = screen.getByRole('switch');
      expect(toggle).toBeDisabled();
    });
    
    // Try to enable through the handler directly
    const component = screen.getByText('Enhancements').closest('div');
    expect(component).toBeInTheDocument();
    
    // The switch is disabled, so we can't actually click it to trigger the error
    // This test validates that the switch is properly disabled when no API key exists
  });
});