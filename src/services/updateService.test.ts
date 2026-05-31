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

import { check, type Update } from '@tauri-apps/plugin-updater';
import { ask } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { relaunch } from '@tauri-apps/plugin-process';
import { UpdateService } from './updateService';

const JUST_UPDATED_KEY = 'just_updated_version';

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
});

describe('UpdateService update consent', () => {
  let service: UpdateService;
  let update: Update;
  let downloadAndInstall: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    // @ts-expect-error accessing private static for test isolation
    UpdateService.instance = undefined;
    service = UpdateService.getInstance();
    downloadAndInstall = vi.fn(async () => undefined);
    update = {
      available: true,
      version: '1.12.4',
      currentVersion: '1.12.3',
      body: 'Bug fixes',
      date: null,
      rawJson: {},
      downloadAndInstall,
      close: vi.fn().mockResolvedValue(undefined),
    } as unknown as Update;
    vi.mocked(check).mockResolvedValue(update);
    vi.mocked(invoke).mockResolvedValue({ state: 'idle' });
  });

  it('prompts on startup but does not download when user declines', async () => {
    vi.mocked(ask).mockResolvedValue(false);

    await service.initialize({
      hotkey: 'CommandOrControl+Shift+Space',
      current_model: 'base.en',
      language: 'en',
      theme: 'system',
    });

    expect(check).toHaveBeenCalledOnce();
    expect(ask).toHaveBeenCalledWith(expect.stringContaining('Update 1.12.4 is available'), {
      title: 'Update Available',
      kind: 'info',
      okLabel: 'Update',
      cancelLabel: 'Later',
    });
    expect(downloadAndInstall).not.toHaveBeenCalled();
    expect(relaunch).not.toHaveBeenCalled();
  });

  it('auto-installs background updates only after explicit opt-in', async () => {
    await service.initialize({
      hotkey: 'CommandOrControl+Shift+Space',
      current_model: 'base.en',
      language: 'en',
      theme: 'system',
      install_updates_automatically: true,
    });

    expect(check).toHaveBeenCalledOnce();
    expect(ask).not.toHaveBeenCalled();
    expect(downloadAndInstall).toHaveBeenCalledOnce();
    expect(relaunch).toHaveBeenCalledOnce();
  });

  it('downloads and relaunches only after user accepts', async () => {
    vi.mocked(ask).mockResolvedValue(true);

    await service.checkForUpdatesManually();

    expect(check).toHaveBeenCalledOnce();
    expect(ask).toHaveBeenCalled();
    expect(downloadAndInstall).toHaveBeenCalledOnce();
    expect(relaunch).toHaveBeenCalledOnce();
  });
});
