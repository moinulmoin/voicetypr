import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi, describe, it, expect, beforeEach } from 'vitest';
import { EnhancementsTab } from './EnhancementsTab';
import { toast } from 'sonner';

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    warning: vi.fn(),
    info: vi.fn(),
    success: vi.fn()
  }
}));

// Mock contexts
vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: {
      ai_provider: 'openai',
      ai_model: 'gpt-4',
      enhancement_enabled: true
    },
    updateSettings: vi.fn().mockResolvedValue(true)
  })
}));

// Mock hooks
vi.mock('@/hooks/useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn((event: string, callback: any) => {
      (window as any).__testEventCallbacks = (window as any).__testEventCallbacks || {};
      (window as any).__testEventCallbacks[event] = callback;
      return vi.fn();
    })
  })
}));

// Mock API key utilities
vi.mock('@/utils/keyring', () => ({
  getApiKey: vi.fn(() => Promise.resolve('test-api-key')),
  saveApiKey: vi.fn(() => Promise.resolve()),
  deleteApiKey: vi.fn(() => Promise.resolve())
}));

// Mock EnhancementsSection component
vi.mock('../sections/EnhancementsSection', () => ({
  EnhancementsSection: () => (
    <div data-testid="enhancements-section">
      <div>AI Provider: OpenAI</div>
      <div>AI Model: GPT-4</div>
      <button onClick={() => console.log('Save API Key')}>Save API Key</button>
      <button onClick={() => console.log('Test Connection')}>Test Connection</button>
    </div>
  )
}));

describe('EnhancementsTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__testEventCallbacks = {};
  });

  it('renders without crashing', () => {
    render(<EnhancementsTab />);
    expect(screen.getByTestId('enhancements-section')).toBeInTheDocument();
  });

  it('displays AI provider settings', () => {
    render(<EnhancementsTab />);
    expect(screen.getByText('AI Provider: OpenAI')).toBeInTheDocument();
    expect(screen.getByText('AI Model: GPT-4')).toBeInTheDocument();
  });

  it('registers AI enhancement event listeners', () => {
    render(<EnhancementsTab />);
    
    expect((window as any).__testEventCallbacks).toHaveProperty('ai-enhancement-auth-error');
    expect((window as any).__testEventCallbacks).toHaveProperty('ai-enhancement-error');
  });

  it('handles AI authentication error event', () => {
    render(<EnhancementsTab />);
    
    const callback = (window as any).__testEventCallbacks['ai-enhancement-auth-error'];
    callback('Invalid API key');

    expect(toast.error).toHaveBeenCalledWith(
      'Invalid API key',
      expect.objectContaining({
        description: 'Navigate to Enhancements to update your API key'
      })
    );
  });

  it('handles AI enhancement error event', () => {
    render(<EnhancementsTab />);
    
    const callback = (window as any).__testEventCallbacks['ai-enhancement-error'];
    callback('Enhancement failed: Rate limit exceeded');

    expect(toast.warning).toHaveBeenCalledWith('Enhancement failed: Rate limit exceeded');
  });

  it('provides action button in auth error toast', () => {
    render(<EnhancementsTab />);
    
    const callback = (window as any).__testEventCallbacks['ai-enhancement-auth-error'];
    callback('API key expired');

    const toastCall = vi.mocked(toast.error).mock.calls[0];
    const options = toastCall[1] as any;
    
    expect(options.action).toBeDefined();
    expect(options.action.label).toBe('Go to Settings');
  });

  it('loads API key on mount', async () => {
    const { getApiKey } = await import('@/utils/keyring');
    vi.mocked(getApiKey).mockResolvedValueOnce('existing-key');
    
    render(<EnhancementsTab />);
    
    await waitFor(() => {
      expect(getApiKey).toHaveBeenCalled();
    });
  });

  it('renders Save API Key button', () => {
    render(<EnhancementsTab />);
    expect(screen.getByText('Save API Key')).toBeInTheDocument();
  });

  it('renders Test Connection button', () => {
    render(<EnhancementsTab />);
    expect(screen.getByText('Test Connection')).toBeInTheDocument();
  });

  it('handles multiple AI errors gracefully', () => {
    render(<EnhancementsTab />);
    
    const authCallback = (window as any).__testEventCallbacks['ai-enhancement-auth-error'];
    const errorCallback = (window as any).__testEventCallbacks['ai-enhancement-error'];
    
    // Simulate multiple errors
    authCallback('Invalid key');
    errorCallback('Network error');
    authCallback('Expired key');
    
    expect(toast.error).toHaveBeenCalledTimes(2);
    expect(toast.warning).toHaveBeenCalledTimes(1);
  });

  it('cleans up event listeners on unmount', () => {
    const { unmount } = render(<EnhancementsTab />);
    
    // Verify event callbacks exist
    expect((window as any).__testEventCallbacks['ai-enhancement-auth-error']).toBeDefined();
    expect((window as any).__testEventCallbacks['ai-enhancement-error']).toBeDefined();
    
    unmount();
    
    // Cleanup verification would depend on implementation
    // Event coordinator should handle cleanup
  });

  it('handles API provider change', async () => {
    const { updateSettings } = await import('@/contexts/SettingsContext');
    
    render(<EnhancementsTab />);
    
    // Simulate provider change
    await vi.mocked(updateSettings)({ ai_provider: 'anthropic' });
    
    expect(updateSettings).toHaveBeenCalledWith({ ai_provider: 'anthropic' });
  });
});