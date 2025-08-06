import { OsType, type } from '@tauri-apps/plugin-os';

export type Platform = 'darwin' | 'windows' | 'linux';

export function getPlatform(): OsType {
  return type();

}

export function isMacOS(): boolean {
  const platform = getPlatform();
  return platform === 'macos';
}

export function isWindows(): boolean {
  const platform = getPlatform();
  return platform === 'windows';
}

export function isLinux(): boolean {
  const platform = getPlatform();
  return platform === 'linux';
}