import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-shell';

export interface PermissionStatus {
  microphone: boolean;
  accessibility: boolean;
  automation: boolean;
}

/**
 * Check all required permissions
 */
export async function checkAllPermissions(): Promise<PermissionStatus> {
  const [microphone, accessibility, automation] = await Promise.all([
    checkMicrophonePermission(),
    checkAccessibilityPermission(),
    checkAutomationPermission(),
  ]);
  
  return { microphone, accessibility, automation };
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
 * Check automation permission
 */
export async function checkAutomationPermission(): Promise<boolean> {
  try {
    return await invoke<boolean>('test_automation_permission');
  } catch (error) {
    console.error('Failed to check automation permission:', error);
    return false;
  }
}

/**
 * Open automation settings
 */
export async function openAutomationSettings(): Promise<void> {
  try {
    await open('x-apple.systempreferences:com.apple.preference.security?Privacy_Automation');
  } catch (error) {
    console.error('Failed to open automation settings:', error);
  }
}

/**
 * Trigger automation permission dialog by attempting to use AppleScript
 * This will show the system dialog if permission hasn't been granted yet
 */
export async function triggerAutomationPermission(): Promise<boolean> {
  try {
    // This will trigger the automation permission dialog if not already granted
    const result = await invoke<boolean>('test_automation_permission');
    return result;
  } catch (error) {
    console.error('Failed to trigger automation permission:', error);
    return false;
  }
}

/**
 * Get human-readable permission descriptions
 */
export function getPermissionDescription(permission: keyof PermissionStatus): string {
  const descriptions = {
    microphone: 'Required to record your voice for transcription',
    accessibility: 'Required to monitor keyboard shortcuts and insert text',
    automation: 'Required to automatically paste transcribed text at cursor position',
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
    automation: 'Go to System Settings → Privacy & Security → Automation → VoiceTypr and enable System Events',
  };
  
  return instructions[permission] || 'Check System Settings → Privacy & Security';
}