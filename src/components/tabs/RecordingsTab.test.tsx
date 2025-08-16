import { render, screen, waitFor } from '@testing-library/react';
import { vi, describe, it, expect, beforeEach, afterEach } from 'vitest';
import { RecordingsTab } from './RecordingsTab';
import { mockIPC, clearMocks } from '@tauri-apps/api/mocks';
import { EventCallback } from '@tauri-apps/api/event';

// Mock sonner
vi.mock('sonner', () => ({
  toast: {
    error: vi.fn(),
    warning: vi.fn()
  }
}));

// Mock hooks
vi.mock('@/contexts/SettingsContext', () => ({
  useSettings: () => ({
    settings: { hotkey: 'Cmd+Shift+Space' }
  })
}));

// Track registered events
const registeredEvents: Record<string, any> = {};

vi.mock('@/hooks/useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn((event: string, callback: EventCallback<any>) => {
      // Store callbacks for testing
      (window as any).__testEventCallbacks = (window as any).__testEventCallbacks || {};
      (window as any).__testEventCallbacks[event] = callback;
      registeredEvents[event] = callback;
      return vi.fn(); // Return unregister function
    })
  })
}));

// Mock RecentRecordings component
vi.mock('@/components/sections/RecentRecordings', () => ({
  RecentRecordings: ({ history, onRefresh }: any) => (
    <div data-testid="recent-recordings">
      <div>History count: {history.length}</div>
      <button onClick={onRefresh}>Refresh</button>
      {history.map((item: any) => (
        <div key={item.id}>{item.text}</div>
      ))}
    </div>
  )
}));

describe('RecordingsTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__testEventCallbacks = {};
    Object.keys(registeredEvents).forEach(key => delete registeredEvents[key]);
    
    // Setup default Tauri IPC mock
    mockIPC((cmd) => {
      if (cmd === 'get_transcription_history') {
        return [];
      }
      return null;
    });
  });
  
  afterEach(() => {
    clearMocks();
  });


  it('loads history on mount', async () => {
    const mockHistory = [
      {
        timestamp: '2024-01-01T00:00:00Z',
        text: 'Test transcription',
        model: 'tiny'
      }
    ];
    
    // Setup mock for this test
    clearMocks();
    mockIPC((cmd) => {
      if (cmd === 'get_transcription_history') {
        return mockHistory;
      }
      return null;
    });
    
    render(<RecordingsTab />);
    
    await waitFor(() => {
      expect(screen.getByText('History count: 1')).toBeInTheDocument();
    });
  });

  it('displays empty state when no history', async () => {
    render(<RecordingsTab />);
    
    await waitFor(() => {
      expect(screen.getByText('History count: 0')).toBeInTheDocument();
    });
  });

  it('silently handles errors when loading history', async () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    
    // Mock to throw an error
    clearMocks();
    mockIPC((cmd) => {
      if (cmd === 'get_transcription_history') {
        throw new Error('Failed to load');
      }
      return null;
    });
    
    render(<RecordingsTab />);
    
    await waitFor(() => {
      expect(consoleSpy).toHaveBeenCalledWith('Failed to load transcription history:', expect.any(Error));
    });
    
    consoleSpy.mockRestore();
  });


  it('reloads history when history-updated event is fired', async () => {
    const mockHistory = [
      {
        timestamp: '2024-01-01T00:00:00Z',
        text: 'Updated transcription',
        model: 'base'
      }
    ];
    
    // Setup mock to return empty first, then updated history
    let callCount = 0;
    clearMocks();
    mockIPC((cmd) => {
      if (cmd === 'get_transcription_history') {
        callCount++;
        return callCount === 1 ? [] : mockHistory;
      }
      return null;
    });
    
    render(<RecordingsTab />);
    
    // Initially empty
    await waitFor(() => {
      expect(screen.getByText('History count: 0')).toBeInTheDocument();
    });
    
    // Fire the event
    const callback = (window as any).__testEventCallbacks['history-updated'];
    await callback();
    
    // Should reload and show updated history
    await waitFor(() => {
      expect(screen.getByText('History count: 1')).toBeInTheDocument();
    });
  });

  it('handles recording error event with toast', async () => {
    const { toast } = await import('sonner');
    
    render(<RecordingsTab />);
    
    // Wait for events to be registered
    await waitFor(() => {
      expect((window as any).__testEventCallbacks['recording-error']).toBeDefined();
    });
    
    const callback = (window as any).__testEventCallbacks['recording-error'];
    if (callback) {
      callback('Microphone not available');
    }
    
    expect(toast.error).toHaveBeenCalledWith(
      'Recording Failed',
      expect.objectContaining({
        description: 'Microphone not available'
      })
    );
  });

  it('handles transcription error event with toast', async () => {
    const { toast } = await import('sonner');
    
    render(<RecordingsTab />);
    
    // Wait for events to be registered
    await waitFor(() => {
      expect((window as any).__testEventCallbacks['transcription-error']).toBeDefined();
    });
    
    const callback = (window as any).__testEventCallbacks['transcription-error'];
    if (callback) {
      callback('Model not loaded');
    }
    
    expect(toast.error).toHaveBeenCalledWith(
      'Transcription Failed',
      expect.objectContaining({
        description: 'Model not loaded'
      })
    );
  });
});