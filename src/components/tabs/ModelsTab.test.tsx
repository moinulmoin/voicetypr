import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ModelsTab } from './ModelsTab';
const mockDeleteModel = vi.fn();
const mockUpdateSettings = vi.fn();
let capturedOnDelete: (name: string) => Promise<void> = () => Promise.resolve();


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
    },
    updateSettings: mockUpdateSettings
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
    engine: 'whisper',
    kind: 'local' as const,
    requires_setup: false
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
    engine: 'whisper',
    kind: 'local' as const,
    requires_setup: false
  }
};

// Mock the ModelManagementContext that ModelsTab actually imports
vi.mock('@/contexts/ModelManagementContext', () => ({
  useModelManagementContext: () => ({
    models: mockModels,
    downloadProgress: {},
    verifyingModels: new Set(),
    downloadPhases: {},
    downloadErrors: { 'small.en': 'Network error' },
    isLoading: true,
    sortedModels: Object.entries(mockModels),
    downloadModel: vi.fn(),
    deleteModel: mockDeleteModel,
    cancelDownload: vi.fn(),
    retryDownload: vi.fn(),
    refreshModels: vi.fn(),
    loadModels: vi.fn(),
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
  ModelsSection: ({ models, currentModel, downloadErrors, isLoading, onDelete }: any) => {
    capturedOnDelete = onDelete;
    return (
      <div data-testid="models-section">
        <div>Current Model: {currentModel}</div>
        <div>Models Count: {models.length}</div>
        <div>Small Error: {downloadErrors['small.en']}</div>
        <div>Loading: {String(isLoading)}</div>
      </div>
    );
  }
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

  it('passes hook-owned errors and loading state to ModelsSection', () => {
    render(<ModelsTab />);

    expect(screen.getByText('Small Error: Network error')).toBeInTheDocument();
    expect(screen.getByText('Loading: true')).toBeInTheDocument();
  });

  it('swallows a deleteModel rejection without clearing the model selection', async () => {
    mockDeleteModel.mockRejectedValueOnce(new Error('delete failed'));

    render(<ModelsTab />);
    // mock ModelsSection mounted and captured the handler
    expect(screen.getByTestId('models-section')).toBeInTheDocument();

    // ModelCard calls onDelete(name) fire-and-forget; a failing delete_model
    // must not escape as an unhandled rejection, and selection stays unchanged.
    await expect(capturedOnDelete('base.en')).resolves.toBeUndefined();

    expect(mockDeleteModel).toHaveBeenCalledWith('base.en');
    expect(mockUpdateSettings).not.toHaveBeenCalled();
  });
});
