import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { toast } from 'sonner';

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
import { sendNotification, isPermissionGranted } from '@tauri-apps/plugin-notification';
import type { DistributionInfo } from '@/types/distribution';
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

function mockDirectDistribution(): void {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === 'get_distribution_info') {
      return {
        channel: 'direct',
        is_store_install: false,
        package_family_name: null,
      };
    }

    if (command === 'get_current_recording_state') {
      return { state: 'idle' };
    }

    return undefined;
  });
}
describe('UpdateService version marker', () => {
  let service: UpdateService;

  beforeEach(() => {
    storage.clear();
    vi.clearAllMocks();
    mockDirectDistribution();
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
    mockDirectDistribution();
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
      close: vi.fn().mockResolvedValue(undefined),
    } as unknown as Update);

    await service.initialize(testSettings({ check_updates_automatically: true }));

    expect(check).toHaveBeenCalledTimes(1);
    expect(downloadAndInstall).not.toHaveBeenCalled();
    expect(toast.info).toHaveBeenCalledWith(
      'Update 2.0.0 is available. Open Settings to install it.',
    );
    expect(sendNotification).toHaveBeenCalledWith({
      title: 'Update Available',
      body: 'Voicetypr 2.0.0 is ready to install from Settings.',
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

  it('deduplicates concurrent distribution info requests', async () => {
    let resolveDistribution!: (info: DistributionInfo) => void;
    const distributionInfoPromise = new Promise<DistributionInfo>((resolve) => {
      resolveDistribution = resolve;
    });

    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'get_distribution_info') {
        return distributionInfoPromise;
      }

      return { state: 'idle' };
    });

    const settings: AppSettings = {
      hotkey: 'CommandOrControl+Shift+Space',
      current_model: 'base.en',
      speech_language: 'en',
      theme: 'system',
      check_updates_automatically: false,
    };
    const first = service.initialize(settings);
    const second = service.initialize(settings);

    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === 'get_distribution_info')).toHaveLength(1);

    resolveDistribution({
      channel: 'direct',
      is_store_install: false,
      package_family_name: null,
    });

    await Promise.all([first, second]);
  });

  it('holds update-check lock while distribution info is pending', async () => {
    let resolveDistribution!: (info: DistributionInfo) => void;
    const distributionInfoPromise = new Promise<DistributionInfo>((resolve) => {
      resolveDistribution = resolve;
    });

    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'get_distribution_info') {
        return distributionInfoPromise;
      }

      return { state: 'idle' };
    });
    vi.mocked(check).mockResolvedValue(null);

    const backgroundCheck = service.checkForUpdatesInBackground();
    const manualCheck = service.checkForUpdatesManually();

    expect(toast.info).toHaveBeenCalledWith('Update check already in progress');
    expect(check).not.toHaveBeenCalled();

    resolveDistribution({
      channel: 'direct',
      is_store_install: false,
      package_family_name: null,
    });

    await Promise.all([backgroundCheck, manualCheck]);

    expect(check).toHaveBeenCalledOnce();
  });

  it('skips direct updater checks for Microsoft Store installs', async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === 'get_distribution_info') {
        return {
          channel: 'store_msix',
          is_store_install: true,
          package_family_name: 'Ideaplexa.Voicetypr_12345',
        };
      }

      return { state: 'idle' };
    });

    await service.initialize({
      hotkey: 'CommandOrControl+Shift+Space',
      current_model: 'base.en',
      speech_language: 'en',
      theme: 'system',
    });

    expect(check).not.toHaveBeenCalled();

    await service.checkForUpdatesManually();

    expect(check).not.toHaveBeenCalled();
    expect(toast.info).toHaveBeenCalledWith('Updates are handled by Microsoft Store');

    localStorage.setItem(JUST_UPDATED_KEY, '1.12.5');
    expect(service.getJustUpdatedVersion()).toBeNull();
    expect(localStorage.getItem(JUST_UPDATED_KEY)).toBeNull();
  });
});
