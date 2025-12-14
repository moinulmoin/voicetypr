import '@testing-library/jest-dom';
import { expect, afterEach, vi, beforeEach } from 'vitest';
import { cleanup } from '@testing-library/react';
import * as matchers from '@testing-library/jest-dom/matchers';
import { mockIPC, mockWindows, clearMocks } from '@tauri-apps/api/mocks';

// Extend Vitest's expect with jest-dom matchers
expect.extend(matchers);

const defaultIpcHandler = (cmd: string) => {
  // Default mock responses for common commands
  switch (cmd) {
    case 'start_recording':
      return true;
    
    case 'stop_recording':
      return true;

    case 'get_current_recording_state':
      return { state: 'idle', error: null };

    case 'get_settings':
      return {
        hotkey: 'CommandOrControl+Shift+Space',
        language: 'en',
        theme: 'system',
        current_model: 'base.en',
        current_model_engine: 'whisper',
        transcription_cleanup_days: 30,
        onboarding_completed: true,
        auto_launch: true,
        microphone_device: null,
        ai_provider: 'groq',
        ai_enhancement_enabled: false,
        play_sound_on_recording: true
      };

    case 'save_settings':
      return true;

    case 'get_model_status':
      return {
        models: [
          {
            name: 'tiny.en',
            display_name: 'Tiny English',
            size: 39,
            url: '',
            sha256: '',
            downloaded: false,
            speed_score: 10,
            accuracy_score: 3,
            recommended: false,
            engine: 'whisper',
            kind: 'local',
            requires_setup: false,
          },
          {
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
            kind: 'local',
            requires_setup: false,
          },
          {
            name: 'soniox',
            display_name: 'Soniox (Cloud)',
            size: 0,
            url: '',
            sha256: '',
            downloaded: true,
            speed_score: 9,
            accuracy_score: 10,
            recommended: true,
            engine: 'soniox',
            kind: 'cloud',
            requires_setup: false,
          }
        ]
      };

    case 'download_model':
      return true;

    case 'delete_model':
      return true;

    case 'get_audio_devices':
      return ['Default Microphone', 'USB Microphone'];

    case 'cleanup_old_transcriptions':
      return true;

    case 'get_transcription_history':
      return [];

    case 'get_recording_state':
      return { state: 'idle', error: null };

    case 'get_ai_settings':
      return { enabled: false, provider: 'groq', model: '', hasApiKey: false };

    case 'get_enhancement_options':
      return { preset: 'Default', custom_vocabulary: [] };

    case 'init_cleanup_schedule':
      return true;

    case 'load_api_keys_to_cache':
      return true;

    case 'register_hotkey':
      return true;

    case 'check_permissions':
      return { microphone: true, accessibility: true };

    default:
      return null;
  }
};

// Setup and cleanup per test
beforeEach(() => {
  vi.clearAllMocks();
  // Re-initialize Tauri mocks for each test
  mockWindows('main');
  mockIPC(defaultIpcHandler);
});

afterEach(() => {
  cleanup();
  clearMocks();
});

// Mock window.crypto for Tauri in jsdom environment
if (!window.crypto) {
  Object.defineProperty(window, 'crypto', {
    value: {
      getRandomValues: (arr: any) => {
        for (let i = 0; i < arr.length; i++) {
          arr[i] = Math.floor(Math.random() * 256);
        }
        return arr;
      },
      randomUUID: () => {
        return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
          const r = (Math.random() * 16) | 0;
          const v = c === 'x' ? r : (r & 0x3) | 0x8;
          return v.toString(16);
        });
      },
    },
  });
}

// Mock event listeners
const eventListeners = new Map<string, Set<Function>>();

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((event: string, handler: Function) => {
    if (!eventListeners.has(event)) {
      eventListeners.set(event, new Set());
    }
    eventListeners.get(event)!.add(handler);
    
    // Return unsubscribe function
    return Promise.resolve(() => {
      eventListeners.get(event)?.delete(handler);
    });
  }),
  
  emit: vi.fn((event: string, payload?: any) => {
    const handlers = eventListeners.get(event);
    if (handlers) {
      handlers.forEach(handler => handler({ payload }));
    }
    return Promise.resolve();
  }),
}));

// Mock dialog plugin
vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: vi.fn(() => Promise.resolve(true)), // Default to confirming dialogs
}));

// Mock global shortcut plugin
vi.mock('@tauri-apps/plugin-global-shortcut', () => ({
  GlobalShortcutExt: vi.fn(),
  ShortcutState: {
    Pressed: 'pressed',
    Released: 'released',
  },
}));

// Mock OS plugin for platform detection
vi.mock('@tauri-apps/plugin-os', () => ({
  type: vi.fn(() => 'macos'), // Default to macOS for tests
}));

// Mock window.matchMedia
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: vi.fn().mockImplementation(query => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
});

// Mock IntersectionObserver
global.IntersectionObserver = vi.fn().mockImplementation(() => ({
  observe: vi.fn(),
  unobserve: vi.fn(),
  disconnect: vi.fn(),
}));

// Mock ResizeObserver
global.ResizeObserver = vi.fn().mockImplementation(() => ({
  observe: vi.fn(),
  unobserve: vi.fn(),
  disconnect: vi.fn(),
}));

// Export helper to emit mock events in tests
export const emitMockEvent = (event: string, payload: any) => {
  const handlers = eventListeners.get(event);
  if (handlers) {
    handlers.forEach(handler => handler({ payload }));
  }
};
