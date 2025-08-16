import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ModelsTab } from './ModelsTab';

// Mock sonner
vi.mock('sonner', () => ({
  toast: {
    info: vi.fn(),
    warning: vi.fn()
  }
}));

// Mock contexts
vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: {
      current_model: 'base.en'
    }
  })
}));

// Mock hooks
let mockModels = {
  'base.en': { id: 'base.en', name: 'Base English', size: 74, downloaded: true },
  'small.en': { id: 'small.en', name: 'Small English', size: 244, downloaded: false }
};

vi.mock('@/hooks/useModelManagement', () => ({
  useModelManagement: () => ({
    models: mockModels,
    downloadProgress: {},
    verifyingModels: new Set(),
    sortedModels: Object.values(mockModels),
    downloadModel: vi.fn(),
    deleteModel: vi.fn(),
    selectModel: vi.fn(),
    retryDownload: vi.fn()
  })
}));

vi.mock('@/hooks/useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn((event: string, callback: any) => {
      (window as any).__testEventCallbacks = (window as any).__testEventCallbacks || {};
      (window as any).__testEventCallbacks[event] = callback;
      return vi.fn();
    })
  })
}));

// Mock ModelsSection component
vi.mock('@/components/sections/ModelsSection', () => ({
  ModelsSection: ({ models, currentModel }: any) => (
    <div data-testid="models-section">
      <div>Current Model: {currentModel}</div>
      <div>Models Count: {models.length}</div>
    </div>
  )
}));

describe('ModelsTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__testEventCallbacks = {};
  });

  it('displays current model and available models', () => {
    render(<ModelsTab />);
    expect(screen.getByText('Current Model: base.en')).toBeInTheDocument();
    expect(screen.getByText('Models Count: 2')).toBeInTheDocument();
  });

  it('shows toast on download retry', async () => {
    const { toast } = await import('sonner');
    render(<ModelsTab />);
    
    const callback = (window as any).__testEventCallbacks['download-retry'];
    callback({ model: 'small.en', attempt: 1, max_attempts: 3 });

    expect(toast.warning).toHaveBeenCalledWith(
      'Download Retry',
      expect.objectContaining({
        description: expect.stringContaining('small.en')
      })
    );
  });
});