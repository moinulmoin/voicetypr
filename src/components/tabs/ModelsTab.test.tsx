import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ModelsTab } from './ModelsTab';


const mockUpdateSettings = vi.fn();
const mockDeleteModel = vi.fn();


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


// Mock ModelsSection component
vi.mock('@/components/sections/ModelsSection', () => ({
  ModelsSection: ({ models, currentModel, onDelete }: any) => (
    <div data-testid="models-section">
      <div>Current Model: {currentModel}</div>
      <div>Models Count: {models.length}</div>
      <button onClick={() => onDelete(currentModel)}>Delete Current</button>
    </div>
  )
}));

describe('ModelsTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockDeleteModel.mockResolvedValue(true);
    mockUpdateSettings.mockResolvedValue(undefined);
  });

  it('displays current model and available models', () => {
    render(<ModelsTab />);
    expect(screen.getByText('Current Model: base.en')).toBeInTheDocument();
    expect(screen.getByText('Models Count: 2')).toBeInTheDocument();
  });

  it('keeps current model when delete confirmation is cancelled', async () => {
    mockDeleteModel.mockResolvedValue(false);

    render(<ModelsTab />);
    fireEvent.click(screen.getByRole('button', { name: /delete current/i }));

    await waitFor(() => {
      expect(mockDeleteModel).toHaveBeenCalledWith('base.en');
    });
    expect(mockUpdateSettings).not.toHaveBeenCalled();
  });

  it('clears current model only after successful deletion', async () => {
    render(<ModelsTab />);
    fireEvent.click(screen.getByRole('button', { name: /delete current/i }));

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith({
        current_model: '',
        current_model_engine: 'whisper'
      });
    });
  });

});
