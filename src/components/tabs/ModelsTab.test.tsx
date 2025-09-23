import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ModelsTab } from './ModelsTab';

// Mock sonner
vi.mock('sonner', () => ({
  toast: {
    info: vi.fn(),
    warning: vi.fn(),
    error: vi.fn(),
    success: vi.fn()
  }
}));

// Mock contexts
vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: {
      current_model: 'base.en',
      current_model_engine: 'whisper'
    }
  })
}));

// Mock hooks
let mockModels = {
  'base.en': {
    name: 'base.en',
    display_name: 'Base English',
    size: 74,
    url: '',
    sha256: '',
    downloaded: true,
    speed_score: 7,
    accuracy_score: 5,
    recommended: false,
    engine: 'whisper'
  },
  'small.en': {
    name: 'small.en',
    display_name: 'Small English',
    size: 244,
    url: '',
    sha256: '',
    downloaded: false,
    speed_score: 5,
    accuracy_score: 7,
    recommended: false,
    engine: 'whisper'
  }
};

// Mock the ModelManagementContext that ModelsTab actually imports
vi.mock('@/contexts/ModelManagementContext', () => ({
  useModelManagementContext: () => ({
    models: mockModels,
    downloadProgress: {},
    verifyingModels: new Set(),
    sortedModels: Object.entries(mockModels),
    downloadModel: vi.fn(),
    deleteModel: vi.fn(),
    cancelDownload: vi.fn(),
    retryDownload: vi.fn(),
    refreshModels: vi.fn(),
    preloadModel: vi.fn(),
    verifyModel: vi.fn()
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

  it('shows error toast on download failure', async () => {
    const { toast } = await import('sonner');
    render(<ModelsTab />);

    const callback = (window as any).__testEventCallbacks['download-error'];
    callback({ model: 'small.en', error: 'Network error' });

    expect(toast.error).toHaveBeenCalledWith(
      'Download Failed',
      expect.objectContaining({
        description: expect.stringContaining('small.en')
      })
    );
  });
});
