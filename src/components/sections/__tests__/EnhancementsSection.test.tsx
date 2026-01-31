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

vi.mock('@/utils/keyring', () => ({
  saveApiKey: vi.fn().mockResolvedValue(undefined),
  hasApiKey: vi.fn().mockResolvedValue(false),
  removeApiKey: vi.fn().mockResolvedValue(undefined),
  getApiKey: vi.fn().mockResolvedValue(null),
  keyringSet: vi.fn().mockResolvedValue(undefined),
}));

describe('EnhancementsSection', () => {
  const mockAISettings = {
    enabled: false,
    provider: '',
    model: '',
    hasApiKey: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({
          preset: 'Default',
          custom_vocabulary: []
        });
      }
      return Promise.resolve(mockAISettings);
    });
  });

  it('renders the enhancements section', async () => {
    render(<EnhancementsSection />);
    
    expect(screen.getByText('AI Formatting')).toBeInTheDocument();
    
    // Wait for providers to load - now we have provider cards
    await waitFor(() => {
      expect(screen.getByText('AI Providers')).toBeInTheDocument();
    });
  });

  it('displays all available providers', async () => {
    render(<EnhancementsSection />);
    
    // Wait for providers section to load - all providers should render together
    await waitFor(() => {
      expect(screen.getByText('AI Providers')).toBeInTheDocument();
      expect(screen.getByText('OpenAI')).toBeInTheDocument();
      expect(screen.getByText('Anthropic')).toBeInTheDocument();
      expect(screen.getByText('Google Gemini')).toBeInTheDocument();
    }, { timeout: 5000 });
    
    // Custom Provider is now in the same list
    await waitFor(() => {
      expect(screen.getByText('Custom (OpenAI-compatible)')).toBeInTheDocument();
    }, { timeout: 3000 });
  });

  it('shows Add Key button when no API key is set', async () => {
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      // Look for "Add Key" buttons
      const addKeyButtons = screen.getAllByText('Add Key');
      expect(addKeyButtons.length).toBeGreaterThan(0);
    });
  });

  it('opens API key modal when Add Key is clicked', async () => {
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      const addKeyButtons = screen.getAllByText('Add Key');
      expect(addKeyButtons.length).toBeGreaterThan(0);
      fireEvent.click(addKeyButtons[0]); // Click first "Add Key" button (OpenAI)
    });
    
    await waitFor(() => {
      // The modal title will show the provider name
      const modalTitle = screen.getByText(/Add OpenAI API Key/);
      expect(modalTitle).toBeInTheDocument();
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
    // Import the mocked hasApiKey function
    const { hasApiKey } = await import('@/utils/keyring');
    
    // Mock hasApiKey to return true for gemini provider
    (hasApiKey as any).mockImplementation((provider: string) => {
      return Promise.resolve(provider === 'gemini');
    });
    
    (invoke as any).mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'get_ai_settings') {
        return Promise.resolve({
          enabled: false,
          provider: 'gemini',
          model: 'gemini-2.0-flash',
          hasApiKey: true,
        });
      }
      if (cmd === 'get_ai_settings_for_provider') {
        return Promise.resolve({
          enabled: false,
          provider: args?.provider || 'gemini',
          model: 'gemini-2.0-flash',
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
    // Import the mocked hasApiKey function
    const { hasApiKey } = await import('@/utils/keyring');
    
    // Mock hasApiKey to return true for gemini provider
    (hasApiKey as any).mockImplementation((provider: string) => {
      return Promise.resolve(provider === 'gemini');
    });
    
    // Mock that we have an API key for gemini provider
    (invoke as any).mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'get_ai_settings') {
        return Promise.resolve({
          enabled: false,
          provider: 'gemini',
          model: 'gemini-2.0-flash',
          hasApiKey: true,
        });
      }
      if (cmd === 'get_ai_settings_for_provider') {
        return Promise.resolve({
          enabled: false,
          provider: args?.provider || 'gemini',
          model: 'gemini-2.0-flash',
          hasApiKey: true,
        });
      }
      return Promise.resolve();
    });
    
    render(<EnhancementsSection />);
    
    // Wait for the component to load
    await waitFor(() => {
      expect(screen.getByText('AI Formatting')).toBeInTheDocument();
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
        provider: 'gemini',
        model: 'gemini-2.0-flash',
      });
      expect(toast.success).toHaveBeenCalledWith('AI formatting enabled');
    });
  });

  it('displays provider cards with model selection', async () => {
    // Setup: User has an API key for Anthropic
    const { hasApiKey } = await import('@/utils/keyring');
    (hasApiKey as any).mockImplementation((provider: string) => {
      return Promise.resolve(provider === 'anthropic');
    });
    
    (invoke as any).mockImplementation((cmd: string) => {
      if (cmd === 'get_ai_settings') {
        return Promise.resolve({
          enabled: false,
          provider: 'anthropic',
          model: 'claude-haiku-4-5',
          hasApiKey: true,
        });
      }
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({
          preset: 'Default',
          custom_vocabulary: [],
        });
      }
      return Promise.resolve();
    });
    
    render(<EnhancementsSection />);
    
    // User should see the AI Providers section
    await waitFor(() => {
      expect(screen.getByText('AI Providers')).toBeInTheDocument();
    });
    
    // User should see all provider cards
    await waitFor(() => {
      expect(screen.getByText('OpenAI')).toBeInTheDocument();
      expect(screen.getByText('Anthropic')).toBeInTheDocument();
      expect(screen.getByText('Google Gemini')).toBeInTheDocument();
    });
  });

  it('handles API key submission', async () => {
    // Import the mocked saveApiKey function
    const { saveApiKey } = await import('@/utils/keyring');
    
    render(<EnhancementsSection />);
    
    // Open modal by clicking the first Add Key button
    await waitFor(() => {
      const addKeyButtons = screen.getAllByText('Add Key');
      expect(addKeyButtons.length).toBeGreaterThan(0);
      fireEvent.click(addKeyButtons[0]); // Click OpenAI's Add Key
    });
    
    // Wait for modal to open
    await waitFor(() => {
      const modalTitle = screen.getByText(/Add OpenAI API Key/);
      expect(modalTitle).toBeInTheDocument();
    });
    
    // Enter API key
    const input = screen.getByPlaceholderText(/Enter your OpenAI API key/);
    fireEvent.change(input, { target: { value: 'sk-test-api-key-12345' } });
    
    // Submit
    const submitButton = screen.getByText('Save API Key');
    fireEvent.click(submitButton);
    
    // Verify saveApiKey was called
    await waitFor(() => {
      expect(saveApiKey).toHaveBeenCalled();
    });
  });

  it('shows Quick Setup guide when AI is disabled', async () => {
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      expect(screen.getByText('Quick Setup')).toBeInTheDocument();
      expect(screen.getByText(/Choose a provider above/)).toBeInTheDocument();
    });
  });

  it('shows Formatting Options section', async () => {
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      expect(screen.getByText('Formatting Options')).toBeInTheDocument();
    });
  });
});
