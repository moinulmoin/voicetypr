import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-shell';

export interface PermissionStatus {
  microphone: boolean;
  accessibility: boolean;
}

/**
 * Check all required permissions
 */
export async function checkAllPermissions(): Promise<PermissionStatus> {
  const [microphone, accessibility] = await Promise.all([
    checkMicrophonePermission(),
    checkAccessibilityPermission(),
  ]);
  
  return { microphone, accessibility };
}

/**
 * Check microphone permission
 */
export async function checkMicrophonePermission(): Promise<boolean> {
  try {
    return await invoke<boolean>('check_microphone_permission');
  } catch (error) {
    console.error('Failed to check microphone permission:', error);
    return false;
  }
}

/**
 * Check accessibility permission
 */
export async function checkAccessibilityPermission(): Promise<boolean> {
  try {
    return await invoke<boolean>('check_accessibility_permission');
  } catch (error) {
    console.error('Failed to check accessibility permission:', error);
    return false;
  }
}

/**
 * Request microphone permission
 */
export async function requestMicrophonePermission(): Promise<boolean> {
  try {
    return await invoke<boolean>('request_microphone_permission');
  } catch (error) {
    console.error('Failed to request microphone permission:', error);
    return false;
  }
}

/**
 * Open accessibility settings
 */
export async function openAccessibilitySettings(): Promise<void> {
  try {
    await open('x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility');
  } catch (error) {
    console.error('Failed to open accessibility settings:', error);
  }
}


/**
 * Get human-readable permission descriptions
 */
export function getPermissionDescription(permission: keyof PermissionStatus): string {
  const descriptions = {
    microphone: 'Required to record your voice for transcription',
    accessibility: 'Required to monitor keyboard shortcuts and insert text',
  };
  
  return descriptions[permission] || 'Required for app functionality';
}

/**
 * Get instructions for enabling a permission
 */
export function getPermissionInstructions(permission: keyof PermissionStatus): string {
  const instructions = {
    microphone: 'Go to System Settings → Privacy & Security → Microphone and enable VoiceTypr',
    accessibility: 'Go to System Settings → Privacy & Security → Accessibility and enable VoiceTypr',
  };
  
  return instructions[permission] || 'Check System Settings → Privacy & Security';
}