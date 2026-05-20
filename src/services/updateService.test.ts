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
import { check } from '@tauri-apps/plugin-updater';
import { ask } from '@tauri-apps/plugin-dialog';
import { sendNotification, isPermissionGranted } from '@tauri-apps/plugin-notification';
import { toast } from 'sonner';
import type { AppSettings } from '@/types';

const JUST_UPDATED_KEY = 'just_updated_version';
const storage = new Map<string, string>();

const testSettings = (overrides: Partial<AppSettings> = {}): AppSettings => ({
  hotkey: 'CmdOrCtrl+Shift+Space',
  current_model: 'tiny.en',
  speech_language: 'en',
  theme: 'system',
  ...overrides,
});

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

describe('UpdateService update checks', () => {
  let service: UpdateService;

  beforeEach(() => {
    storage.clear();
    vi.clearAllMocks();
    // @ts-expect-error accessing private static for test isolation
    UpdateService.instance = undefined;
    service = UpdateService.getInstance();
  });

  it('initializes background checks by default without installing updates', async () => {
    vi.mocked(check).mockResolvedValue(null);
    await service.initialize(testSettings());

    expect(check).toHaveBeenCalledTimes(1);
  });

  it('background checks notify without installing updates', async () => {
    const downloadAndInstall = vi.fn();
    vi.mocked(isPermissionGranted).mockResolvedValue(true);
    vi.mocked(check).mockResolvedValue({
      available: true,
      version: '2.0.0',
      body: 'Release notes',
      downloadAndInstall,
    } as never);

    await service.initialize(testSettings({ check_updates_automatically: true }));

    expect(check).toHaveBeenCalledTimes(1);
    expect(downloadAndInstall).not.toHaveBeenCalled();
    expect(toast.info).toHaveBeenCalledWith(
      'Update 2.0.0 is available. Open Settings to install it.',
    );
    expect(sendNotification).toHaveBeenCalledWith({
      title: 'Update Available',
      body: 'VoiceTypr 2.0.0 is ready to install from Settings.',
    });
  });

  it('manual checks still ask before installing', async () => {
    const downloadAndInstall = vi.fn().mockResolvedValue(undefined);
    vi.mocked(check).mockResolvedValue({
      available: true,
      version: '2.0.0',
      body: 'Release notes',
      downloadAndInstall,
    } as never);
    vi.mocked(ask).mockResolvedValue(false);

    await service.checkForUpdatesManually();

    expect(ask).toHaveBeenCalled();
    expect(downloadAndInstall).not.toHaveBeenCalled();
  });
});
