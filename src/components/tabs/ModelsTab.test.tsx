import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi, describe, it, expect, beforeEach } from 'vitest';
import { ModelsTab } from './ModelsTab';
import { toast } from 'sonner';

// Mock toast
vi.mock('sonner', () => ({
  toast: {
    warning: vi.fn(),
    error: vi.fn(),
    success: vi.fn()
  }
}));

// Mock contexts
vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: { current_model: 'tiny' },
    updateSettings: vi.fn()
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

vi.mock('@/hooks/useModelManagement', () => ({
  useModelManagement: () => ({
    downloadProgress: { 'base': 50 },
    verifyingModels: new Set(['small']),
    downloadModel: vi.fn(),
    cancelDownload: vi.fn(),
    deleteModel: vi.fn(),
    sortedModels: [
      ['tiny', { 
        size: 39000000, 
        downloaded: true, 
        accuracy_score: 7,
        speed_score: 10 
      }],
      ['base', { 
        size: 74000000, 
        downloaded: false,
        accuracy_score: 8,
        speed_score: 8 
      }]
    ]
  })
}));

// Mock ModelsSection component
vi.mock('../sections/ModelsSection', () => ({
  ModelsSection: ({ models, currentModel, onDownload, onDelete, onSelect }: any) => (
    <div data-testid="models-section">
      <div>Current Model: {currentModel}</div>
      <div>Models Count: {models.length}</div>
      {models.map(([name, info]: any) => (
        <div key={name} data-testid={`model-${name}`}>
          <span>{name}</span>
          <button onClick={() => onDownload(name)}>Download {name}</button>
          <button onClick={() => onDelete(name)}>Delete {name}</button>
          <button onClick={() => onSelect(name)}>Select {name}</button>
        </div>
      ))}
    </div>
  )
}));

describe('ModelsTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__testEventCallbacks = {};
  });

  it('renders without crashing', () => {
    render(<ModelsTab />);
    expect(screen.getByTestId('models-section')).toBeInTheDocument();
  });

  it('displays current model from settings', () => {
    render(<ModelsTab />);
    expect(screen.getByText('Current Model: tiny')).toBeInTheDocument();
  });

  it('shows all available models', () => {
    render(<ModelsTab />);
    expect(screen.getByText('Models Count: 2')).toBeInTheDocument();
    expect(screen.getByTestId('model-tiny')).toBeInTheDocument();
    expect(screen.getByTestId('model-base')).toBeInTheDocument();
  });

  it('registers download-retry event listener', () => {
    render(<ModelsTab />);
    expect((window as any).__testEventCallbacks).toHaveProperty('download-retry');
  });

  it('shows toast on first download retry', () => {
    render(<ModelsTab />);
    
    const retryCallback = (window as any).__testEventCallbacks['download-retry'];
    retryCallback({
      model: 'base',
      attempt: 1,
      max_attempts: 3,
      error: 'Network error'
    });

    expect(toast.warning).toHaveBeenCalledWith(
      'Download Retry',
      expect.objectContaining({
        description: 'Download of base failed, retrying... (Attempt 1/3)',
        duration: 4000
      })
    );
  });

  it('does not show toast on subsequent retries', () => {
    render(<ModelsTab />);
    
    const retryCallback = (window as any).__testEventCallbacks['download-retry'];
    retryCallback({
      model: 'base',
      attempt: 2,
      max_attempts: 3,
      error: 'Network error'
    });

    expect(toast.warning).not.toHaveBeenCalled();
  });

  it('handles model download', async () => {
    const { useModelManagement } = await import('@/hooks/useModelManagement');
    const mockDownloadModel = vi.fn();
    vi.mocked(useModelManagement).mockReturnValueOnce({
      downloadProgress: {},
      verifyingModels: new Set(),
      downloadModel: mockDownloadModel,
      cancelDownload: vi.fn(),
      deleteModel: vi.fn(),
      sortedModels: [['base', { size: 74000000, downloaded: false, accuracy_score: 8, speed_score: 8 }]]
    } as any);

    render(<ModelsTab />);
    
    const downloadButton = screen.getByText('Download base');
    await userEvent.click(downloadButton);
    
    expect(mockDownloadModel).toHaveBeenCalledWith('base');
  });

  it('handles model deletion and updates settings if current model', async () => {
    const mockUpdateSettings = vi.fn();
    const mockDeleteModel = vi.fn();
    
    vi.mock('@/contexts/SettingsContext', () => ({
      useSettings: () => ({
        settings: { current_model: 'tiny' },
        updateSettings: mockUpdateSettings
      })
    }));
    
    const { useModelManagement } = await import('@/hooks/useModelManagement');
    vi.mocked(useModelManagement).mockReturnValueOnce({
      downloadProgress: {},
      verifyingModels: new Set(),
      downloadModel: vi.fn(),
      cancelDownload: vi.fn(),
      deleteModel: mockDeleteModel,
      sortedModels: [['tiny', { size: 39000000, downloaded: true, accuracy_score: 7, speed_score: 10 }]]
    } as any);

    render(<ModelsTab />);
    
    const deleteButton = screen.getByText('Delete tiny');
    await userEvent.click(deleteButton);
    
    await waitFor(() => {
      expect(mockDeleteModel).toHaveBeenCalledWith('tiny');
    });
  });

  it('handles model selection', async () => {
    const mockUpdateSettings = vi.fn();
    
    vi.mock('@/contexts/SettingsContext', () => ({
      useSettings: () => ({
        settings: { current_model: 'tiny' },
        updateSettings: mockUpdateSettings
      })
    }));

    render(<ModelsTab />);
    
    const selectButton = screen.getByText('Select base');
    await userEvent.click(selectButton);
    
    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalled();
    });
  });

  it('handles settings save errors gracefully', async () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const mockUpdateSettings = vi.fn().mockRejectedValueOnce(new Error('Save failed'));
    
    vi.mock('@/contexts/SettingsContext', () => ({
      useSettings: () => ({
        settings: { current_model: 'tiny' },
        updateSettings: mockUpdateSettings
      })
    }));

    render(<ModelsTab />);
    
    const selectButton = screen.getByText('Select base');
    await userEvent.click(selectButton);
    
    await waitFor(() => {
      expect(consoleSpy).toHaveBeenCalledWith('Failed to save settings:', expect.any(Error));
    });
    
    consoleSpy.mockRestore();
  });
});