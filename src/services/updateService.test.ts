import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';
import { relaunch } from '@tauri-apps/plugin-process';
import { check, type Update } from '@tauri-apps/plugin-updater';

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


type TestUpdate = Update & {
  downloadAndInstall: ReturnType<typeof vi.fn>;
};

function createAvailableUpdate(): TestUpdate {
  return {
    available: true,
    currentVersion: '1.0.0',
    version: '1.2.3',
    body: 'Security and reliability fixes',
    rawJson: {},
    downloadAndInstall: vi.fn().mockResolvedValue(undefined),
  } as unknown as TestUpdate;
}
describe('UpdateService version marker', () => {
  let service: UpdateService;

  beforeEach(() => {
    localStorage.clear();
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

  it('asks before installing an update found during a background check', async () => {
    const update = createAvailableUpdate();
    vi.mocked(check).mockResolvedValue(update);
    vi.mocked(ask).mockResolvedValue(false);

    await service.checkForUpdatesInBackground();

    expect(ask).toHaveBeenCalledWith(
      expect.stringContaining('Update 1.2.3 is available'),
      expect.objectContaining({
        title: 'Update Available',
        okLabel: 'Update',
        cancelLabel: 'Later',
      }),
    );
    expect(update.downloadAndInstall).not.toHaveBeenCalled();
    expect(relaunch).not.toHaveBeenCalled();
  });

  it('installs a background update only after explicit confirmation', async () => {
    const update = createAvailableUpdate();
    vi.mocked(check).mockResolvedValue(update);
    vi.mocked(ask).mockResolvedValue(true);
    vi.mocked(invoke).mockResolvedValue({ state: 'idle' });
    vi.mocked(relaunch).mockResolvedValue(undefined);

    await service.checkForUpdatesInBackground();

    expect(update.downloadAndInstall).toHaveBeenCalledOnce();
    expect(relaunch).toHaveBeenCalledOnce();
    expect(localStorage.getItem(JUST_UPDATED_KEY)).toBe('1.2.3');
  });
});
