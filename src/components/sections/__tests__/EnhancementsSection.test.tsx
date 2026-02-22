import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { EnhancementsSection } from '../EnhancementsSection';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
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

// Mock models returned by list_provider_models
const mockModels = {
  openai: [
    { id: 'gpt-5-nano', name: 'GPT-5 Nano', recommended: true },
    { id: 'gpt-5-mini', name: 'GPT-5 Mini', recommended: true },
  ],
  gemini: [
    { id: 'gemini-1.5-flash', name: 'Gemini 1.5 Flash', recommended: true },
    { id: 'gemini-2.0-flash-exp', name: 'Gemini 2.0 Flash', recommended: true },
  ],
};

describe('EnhancementsSection', () => {
  const mockAISettings = {
    enabled: false,
    provider: '',
    model: '',
    hasApiKey: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({
          preset: 'Default',
          custom_vocabulary: []
        });
      }
      if (cmd === 'get_ai_settings_for_provider') {
        const provider = (args as { provider?: string })?.provider || '';
        return Promise.resolve({
          ...mockAISettings,
          provider,
          hasApiKey: false,
        });
      }
      if (cmd === 'list_provider_models') {
        const provider = (args as { provider: string })?.provider;
        return Promise.resolve(mockModels[provider as keyof typeof mockModels] || []);
      }
      if (cmd === 'get_openai_config') {
        return Promise.resolve({ baseUrl: 'https://api.openai.com/v1' });
      }
      return Promise.resolve(mockAISettings);
    });
  });

  it('renders the enhancements section', async () => {
    render(<EnhancementsSection />);
    
    expect(screen.getByText('AI Formatting')).toBeInTheDocument();
    
    // Wait for providers to load
    await waitFor(() => {
      expect(screen.getByText('AI Providers')).toBeInTheDocument();
    });
  });

  it('displays all available providers', async () => {
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      expect(screen.getByText('AI Providers')).toBeInTheDocument();
      expect(screen.getByText('OpenAI')).toBeInTheDocument();
      expect(screen.getByText('Google Gemini')).toBeInTheDocument();
    }, { timeout: 5000 });
    
    // Custom Provider is in the same list
    await waitFor(() => {
      expect(screen.getByText('Custom (OpenAI-compatible)')).toBeInTheDocument();
    }, { timeout: 3000 });
  });

  it('shows Add Key button when no API key is set', async () => {
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      const addKeyButtons = screen.getAllByText('Add Key');
      expect(addKeyButtons.length).toBeGreaterThan(0);
    });
  });

  it('opens API key modal when Add Key is clicked', async () => {
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      const addKeyButtons = screen.getAllByText('Add Key');
      expect(addKeyButtons.length).toBeGreaterThan(0);
      fireEvent.click(addKeyButtons[0]);
    });
    
    await waitFor(() => {
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
    const { hasApiKey } = await import('@/utils/keyring');
    
    (hasApiKey as ReturnType<typeof vi.fn>).mockImplementation((provider: string) => {
      return Promise.resolve(provider === 'gemini');
    });
    
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, _args?: Record<string, unknown>) => {
      if (cmd === 'get_ai_settings') {
        return Promise.resolve({
          enabled: false,
          provider: 'gemini',
          model: 'gemini-1.5-flash',
          hasApiKey: true,
        });
      }
      if (cmd === 'list_provider_models') {
        return Promise.resolve(mockModels.gemini);
      }
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({ preset: 'Default', custom_vocabulary: [] });
      }
      if (cmd === 'get_openai_config') {
        return Promise.resolve({ baseUrl: 'https://api.openai.com/v1' });
      }
      return Promise.resolve();
    });
    
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      const toggle = screen.getByRole('switch');
      expect(toggle).toBeEnabled();
    });
  });

  it('enables enhancement toggle for custom no-auth config without keyring key', async () => {
    const { hasApiKey } = await import('@/utils/keyring');

    (hasApiKey as ReturnType<typeof vi.fn>).mockResolvedValue(false);

    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_ai_settings') {
        return Promise.resolve({
          enabled: false,
          provider: 'custom',
          model: 'local-model',
          hasApiKey: true,
        });
      }
      if (cmd === 'get_ai_settings_for_provider') {
        const provider = (args as { provider?: string })?.provider;
        return Promise.resolve({
          enabled: false,
          provider,
          model: 'local-model',
          hasApiKey: provider === 'custom',
        });
      }
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({ preset: 'Default', custom_vocabulary: [] });
      }
      if (cmd === 'get_openai_config') {
        return Promise.resolve({ baseUrl: 'https://custom.endpoint/v1' });
      }
      return Promise.resolve();
    });

    render(<EnhancementsSection />);

    await waitFor(() => {
      const toggle = screen.getByRole('switch');
      expect(toggle).toBeEnabled();
    });
  });

  it('enables enhancement toggle for legacy openai-compatible config without keyring key', async () => {
    const { hasApiKey } = await import('@/utils/keyring');

    (hasApiKey as ReturnType<typeof vi.fn>).mockResolvedValue(false);

    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_ai_settings') {
        return Promise.resolve({
          enabled: false,
          provider: 'openai',
          model: 'legacy-model',
          hasApiKey: true,
        });
      }
      if (cmd === 'get_ai_settings_for_provider') {
        const provider = (args as { provider?: string })?.provider;
        return Promise.resolve({
          enabled: false,
          provider,
          model: 'legacy-model',
          hasApiKey: provider === 'openai',
        });
      }
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({ preset: 'Default', custom_vocabulary: [] });
      }
      if (cmd === 'get_openai_config') {
        return Promise.resolve({ baseUrl: 'https://legacy.endpoint/v1' });
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
    const { hasApiKey } = await import('@/utils/keyring');
    
    (hasApiKey as ReturnType<typeof vi.fn>).mockImplementation((provider: string) => {
      return Promise.resolve(provider === 'gemini');
    });
    
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, _args?: Record<string, unknown>) => {
      if (cmd === 'get_ai_settings') {
        return Promise.resolve({
          enabled: false,
          provider: 'gemini',
          model: 'gemini-1.5-flash',
          hasApiKey: true,
        });
      }
      if (cmd === 'list_provider_models') {
        return Promise.resolve(mockModels.gemini);
      }
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({ preset: 'Default', custom_vocabulary: [] });
      }
      if (cmd === 'get_openai_config') {
        return Promise.resolve({ baseUrl: 'https://api.openai.com/v1' });
      }
      return Promise.resolve();
    });
    
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      expect(screen.getByText('AI Formatting')).toBeInTheDocument();
    });
    
    await waitFor(() => {
      const toggle = screen.getByRole('switch');
      expect(toggle).toBeEnabled();
      fireEvent.click(toggle);
    });
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_ai_settings', {
        enabled: true,
        provider: 'gemini',
        model: 'gemini-1.5-flash',
      });
      expect(toast.success).toHaveBeenCalledWith('AI formatting enabled');
    });
  });

  it('displays provider cards', async () => {
    const { hasApiKey } = await import('@/utils/keyring');
    (hasApiKey as ReturnType<typeof vi.fn>).mockImplementation((provider: string) => {
      return Promise.resolve(provider === 'gemini');
    });
    
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'get_ai_settings') {
        return Promise.resolve({
          enabled: false,
          provider: 'gemini',
          model: 'gemini-1.5-flash',
          hasApiKey: true,
        });
      }
      if (cmd === 'list_provider_models') {
        return Promise.resolve(mockModels.gemini);
      }
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({ preset: 'Default', custom_vocabulary: [] });
      }
      if (cmd === 'get_openai_config') {
        return Promise.resolve({ baseUrl: 'https://api.openai.com/v1' });
      }
      return Promise.resolve();
    });
    
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      expect(screen.getByText('AI Providers')).toBeInTheDocument();
      expect(screen.getByText('OpenAI')).toBeInTheDocument();
      expect(screen.getByText('Google Gemini')).toBeInTheDocument();
    });
  });

  it('handles API key submission', async () => {
    const { saveApiKey } = await import('@/utils/keyring');
    
    render(<EnhancementsSection />);
    
    await waitFor(() => {
      const addKeyButtons = screen.getAllByText('Add Key');
      expect(addKeyButtons.length).toBeGreaterThan(0);
      fireEvent.click(addKeyButtons[0]);
    });
    
    await waitFor(() => {
      const modalTitle = screen.getByText(/Add OpenAI API Key/);
      expect(modalTitle).toBeInTheDocument();
    });
    
    const input = screen.getByPlaceholderText(/Enter your OpenAI API key/);
    fireEvent.change(input, { target: { value: 'sk-test-api-key-12345' } });
    
    const submitButton = screen.getByText('Save API Key');
    fireEvent.click(submitButton);
    
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

  it('does not clear active OpenAI selection when custom key is removed', async () => {
    const { hasApiKey } = await import('@/utils/keyring');

    (hasApiKey as ReturnType<typeof vi.fn>).mockImplementation((provider: string) => {
      return Promise.resolve(provider === 'openai' || provider === 'custom');
    });

    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_ai_settings') {
        return Promise.resolve({
          enabled: true,
          provider: 'openai',
          model: 'gpt-5-nano',
          hasApiKey: true,
        });
      }
      if (cmd === 'list_provider_models') {
        const provider = (args as { provider: string })?.provider;
        return Promise.resolve(mockModels[provider as keyof typeof mockModels] || []);
      }
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({ preset: 'Default', custom_vocabulary: [] });
      }
      if (cmd === 'get_openai_config') {
        return Promise.resolve({ baseUrl: 'https://custom.endpoint/v1' });
      }
      if (cmd === 'get_ai_settings_for_provider') {
        const provider = (args as { provider?: string })?.provider;
        return Promise.resolve({
          enabled: true,
          provider,
          model: 'gpt-5-nano',
          hasApiKey: provider === 'custom',
        });
      }
      return Promise.resolve();
    });

    render(<EnhancementsSection />);

    await waitFor(() => {
      expect(screen.getByText('AI Providers')).toBeInTheDocument();
    });

    await emit('api-key-removed', { provider: 'custom' });

    await waitFor(() => {
      expect(invoke).not.toHaveBeenCalledWith('update_ai_settings', {
        enabled: false,
        provider: '',
        model: '',
      });
    });
  });

  it('does not clear active OpenAI selection when openai key is removed but legacy config exists', async () => {
    const { hasApiKey } = await import('@/utils/keyring');

    // No keyring key, but backend reports legacy config is usable.
    (hasApiKey as ReturnType<typeof vi.fn>).mockResolvedValue(false);

    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_ai_settings') {
        return Promise.resolve({
          enabled: true,
          provider: 'openai',
          model: 'legacy-model',
          hasApiKey: true,
        });
      }
      if (cmd === 'get_ai_settings_for_provider') {
        const provider = (args as { provider?: string })?.provider;
        return Promise.resolve({
          enabled: true,
          provider,
          model: 'legacy-model',
          hasApiKey: provider === 'openai',
        });
      }
      if (cmd === 'get_enhancement_options') {
        return Promise.resolve({ preset: 'Default', custom_vocabulary: [] });
      }
      if (cmd === 'get_openai_config') {
        return Promise.resolve({ baseUrl: 'https://legacy.endpoint/v1' });
      }
      if (cmd === 'list_provider_models') {
        const provider = (args as { provider: string })?.provider;
        return Promise.resolve(mockModels[provider as keyof typeof mockModels] || []);
      }
      return Promise.resolve();
    });

    render(<EnhancementsSection />);

    await waitFor(() => {
      expect(screen.getByText('AI Providers')).toBeInTheDocument();
    });

    await emit('api-key-removed', { provider: 'openai' });

    await waitFor(() => {
      expect(invoke).not.toHaveBeenCalledWith('update_ai_settings', {
        enabled: false,
        provider: '',
        model: '',
      });
    });
  });
});
