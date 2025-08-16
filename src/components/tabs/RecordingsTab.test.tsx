import { render, screen, waitFor } from '@testing-library/react';
import { vi, describe, it, expect, beforeEach } from 'vitest';
import { RecordingsTab } from './RecordingsTab';
import { invoke } from '@tauri-apps/api/core';
import { EventCallback } from '@tauri-apps/api/event';

// Mock Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn()
}));

// Mock hooks
vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: { hotkey: 'Cmd+Shift+Space' }
  })
}));

vi.mock('@/hooks/useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn((event: string, callback: EventCallback<any>) => {
      // Store callbacks for testing
      (window as any).__testEventCallbacks = (window as any).__testEventCallbacks || {};
      (window as any).__testEventCallbacks[event] = callback;
      return vi.fn(); // Return unregister function
    })
  })
}));

// Mock RecentRecordings component
vi.mock('../sections/RecentRecordings', () => ({
  RecentRecordings: ({ history, hotkey, onHistoryUpdate }: any) => (
    <div data-testid="recent-recordings">
      <div>Hotkey: {hotkey}</div>
      <div>History Count: {history.length}</div>
      <button onClick={onHistoryUpdate}>Update History</button>
    </div>
  )
}));

describe('RecordingsTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__testEventCallbacks = {};
  });

  it('renders without crashing', () => {
    render(<RecordingsTab />);
    expect(screen.getByTestId('recent-recordings')).toBeInTheDocument();
  });

  it('loads history on mount', async () => {
    const mockHistory = [
      {
        timestamp: '2024-01-01T00:00:00Z',
        text: 'Test transcription',
        model: 'tiny'
      }
    ];
    
    vi.mocked(invoke).mockResolvedValueOnce(mockHistory);
    
    render(<RecordingsTab />);
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('get_transcription_history', { limit: 50 });
    });
    
    await waitFor(() => {
      expect(screen.getByText('History Count: 1')).toBeInTheDocument();
    });
  });

  it('handles history load failure gracefully', async () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.mocked(invoke).mockRejectedValueOnce(new Error('Failed to load'));
    
    render(<RecordingsTab />);
    
    await waitFor(() => {
      expect(consoleSpy).toHaveBeenCalledWith(
        'Failed to load transcription history:',
        expect.any(Error)
      );
    });
    
    // Should still render with empty history
    expect(screen.getByText('History Count: 0')).toBeInTheDocument();
    
    consoleSpy.mockRestore();
  });

  it('registers event listeners on mount', () => {
    render(<RecordingsTab />);
    
    // Check that all required events are registered
    expect((window as any).__testEventCallbacks).toHaveProperty('history-updated');
    expect((window as any).__testEventCallbacks).toHaveProperty('recording-error');
    expect((window as any).__testEventCallbacks).toHaveProperty('transcription-error');
    expect((window as any).__testEventCallbacks).toHaveProperty('no-speech-detected');
    expect((window as any).__testEventCallbacks).toHaveProperty('transcription-empty');
    expect((window as any).__testEventCallbacks).toHaveProperty('no-models-error');
    expect((window as any).__testEventCallbacks).toHaveProperty('model-incomplete-error');
  });

  it('reloads history when history-updated event is fired', async () => {
    const mockHistory = [
      {
        timestamp: '2024-01-01T00:00:00Z',
        text: 'Updated transcription',
        model: 'base'
      }
    ];
    
    vi.mocked(invoke).mockResolvedValueOnce([]).mockResolvedValueOnce(mockHistory);
    
    render(<RecordingsTab />);
    
    // Initial load
    await waitFor(() => {
      expect(screen.getByText('History Count: 0')).toBeInTheDocument();
    });
    
    // Trigger history-updated event
    const historyUpdatedCallback = (window as any).__testEventCallbacks['history-updated'];
    await historyUpdatedCallback({});
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledTimes(2);
      expect(screen.getByText('History Count: 1')).toBeInTheDocument();
    });
  });

  it('displays correct hotkey from settings', () => {
    render(<RecordingsTab />);
    expect(screen.getByText('Hotkey: Cmd+Shift+Space')).toBeInTheDocument();
  });

  it('handles manual history update', async () => {
    const mockHistory = [
      {
        timestamp: '2024-01-01T00:00:00Z',
        text: 'Manual update',
        model: 'small'
      }
    ];
    
    vi.mocked(invoke).mockResolvedValueOnce([]).mockResolvedValueOnce(mockHistory);
    
    render(<RecordingsTab />);
    
    const updateButton = screen.getByText('Update History');
    updateButton.click();
    
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledTimes(2);
      expect(screen.getByText('History Count: 1')).toBeInTheDocument();
    });
  });
});