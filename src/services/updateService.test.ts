import { beforeEach, describe, expect, it, vi } from 'vitest';

// Mock Tauri plugins before importing the module under test
vi.mock('@tauri-apps/plugin-updater', () => ({
  check: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-notification', () => ({
  sendNotification: vi.fn(),
  isPermissionGranted: vi.fn(),
  requestPermission: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('sonner', () => ({
  toast: {
    info: vi.fn(),
    success: vi.fn(),
    error: vi.fn(),
    dismiss: vi.fn(),
  },
}));

import { UpdateService } from './updateService';

const JUST_UPDATED_KEY = 'just_updated_version';
const storage = new Map<string, string>();

Object.defineProperty(window, 'localStorage', {
  configurable: true,
  value: {
    getItem: vi.fn((key: string) => storage.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => {
      storage.set(key, value);
    }),
    removeItem: vi.fn((key: string) => {
      storage.delete(key);
    }),
    clear: vi.fn(() => {
      storage.clear();
    }),
  },
});

describe('UpdateService version marker', () => {
  let service: UpdateService;

  beforeEach(() => {
    storage.clear();
    vi.clearAllMocks();
    // Get a fresh instance per test to reset internal state
    // @ts-expect-error accessing private static for test isolation
    UpdateService.instance = undefined;
    service = UpdateService.getInstance();
  });

  it('stores version after update install', () => {
    const version = '1.12.1';
    localStorage.setItem(JUST_UPDATED_KEY, version);

    expect(localStorage.getItem(JUST_UPDATED_KEY)).toBe(version);
  });

  it('getJustUpdatedVersion returns and clears marker (one-shot)', () => {
    const version = '2.0.0';
    localStorage.setItem(JUST_UPDATED_KEY, version);

    const result = service.getJustUpdatedVersion();

    expect(result).toBe('2.0.0');
    expect(localStorage.getItem(JUST_UPDATED_KEY)).toBeNull();

    // Second call returns null — marker was consumed
    const result2 = service.getJustUpdatedVersion();
    expect(result2).toBeNull();
  });

  it('returns null when no update marker exists', () => {
    const result = service.getJustUpdatedVersion();

    expect(result).toBeNull();
  });

  it('multiple stores only keep the latest version', () => {
    localStorage.setItem(JUST_UPDATED_KEY, '1.0.0');
    localStorage.setItem(JUST_UPDATED_KEY, '1.12.1');
    localStorage.setItem(JUST_UPDATED_KEY, '2.0.0');

    const result = service.getJustUpdatedVersion();
    expect(result).toBe('2.0.0');
  });

  it('version marker survives simulated crash (persists in localStorage)', () => {
    const version = '1.12.1';
    localStorage.setItem(JUST_UPDATED_KEY, version);

    // Simulate app crash: create a fresh service instance
    // @ts-expect-error accessing private static for test isolation
    UpdateService.instance = undefined;
    const freshService = UpdateService.getInstance();

    const result = freshService.getJustUpdatedVersion();
    expect(result).toBe('1.12.1');
  });
});
