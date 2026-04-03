import { render, screen, waitFor, act } from '@testing-library/react';
import { vi, describe, it, expect, beforeEach, afterEach } from 'vitest';
import { RecordingsTab } from './RecordingsTab';

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

const registerEventMock = vi.fn((event: string, callback: any) => {
  (window as any).__testEventCallbacks = (window as any).__testEventCallbacks || {};
  (window as any).__testEventCallbacks[event] = callback;
  return vi.fn();
});

vi.mock('@/hooks/useEventCoordinator', () => ({
  useEventCoordinator: () => ({
    registerEvent: registerEventMock
  })
}));

// Mock Tauri core invoke so we don't depend on window.__TAURI_INTERNALS__
let invokeMock = vi.fn<(
  cmd: string,
  args?: Record<string, unknown>
) => Promise<any>>();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invokeMock(cmd, args),
}));

// Mock RecentRecordings component
vi.mock('@/components/sections/RecentRecordings', () => ({
  RecentRecordings: ({ history, onHistoryUpdate }: any) => (
    <div data-testid="recent-recordings">
      <div>History count: {history.length}</div>
      <button onClick={onHistoryUpdate}>Refresh</button>
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
    invokeMock = vi.fn(async (cmd: string) => {
      if (cmd === 'get_transcription_history') return [];
      return null;
    });
  });

  afterEach(() => {
    // no-op; mocks reset in beforeEach
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
    invokeMock = vi.fn(async (cmd: string) => {
      if (cmd === 'get_transcription_history') return mockHistory;
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
    invokeMock = vi.fn(async (cmd: string) => {
      if (cmd === 'get_transcription_history') throw new Error('Failed to load');
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
    invokeMock = vi.fn(async (cmd: string) => {
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
    
    const historyUpdated = (window as any).__testEventCallbacks?.['history-updated'];
    expect(historyUpdated).toBeInstanceOf(Function);

    // Fire the event (behavioral path)
    await act(async () => {
      await historyUpdated();
    });
    
    // Should reload and show updated history
    await waitFor(() => {
      expect(screen.getByText('History count: 1')).toBeInTheDocument();
    });
  });

  it('does not register a transcription-updated listener', async () => {
    render(<RecordingsTab />);

    await waitFor(() => {
      expect((window as any).__testEventCallbacks?.['history-updated']).toBeInstanceOf(Function);
    });

    expect((window as any).__testEventCallbacks?.['transcription-updated']).toBeUndefined();
  });
});
