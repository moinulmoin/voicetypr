import { type } from '@tauri-apps/plugin-os';

export type Platform = 'darwin' | 'windows' | 'linux';

export async function getPlatform(): Promise<Platform> {
  const osType = await type();
  
  if (osType === 'Darwin') {
    return 'darwin';
  } else if (osType === 'Windows_NT') {
    return 'windows';
  } else {
    return 'linux';
  }
}

export async function isMacOS(): Promise<boolean> {
  const platform = await getPlatform();
  return platform === 'darwin';
}

export async function isWindows(): Promise<boolean> {
  const platform = await getPlatform();
  return platform === 'windows';
}

export async function isLinux(): Promise<boolean> {
  const platform = await getPlatform();
  return platform === 'linux';
}