/**
 * Known system hotkey conflicts for different platforms
 * These hotkeys are reserved by the OS and may not work reliably
 */

interface ConflictInfo {
  hotkey: string;
  description: string;
  severity: 'error' | 'warning';
}

const WINDOWS_CONFLICTS: ConflictInfo[] = [
  // Windows system hotkeys
  { hotkey: 'Ctrl+Shift+Period', description: 'May conflict with Windows IME or Office shortcuts', severity: 'warning' },
  { hotkey: 'CommandOrControl+Shift+Period', description: 'May conflict with Windows IME or Office shortcuts', severity: 'warning' },
  { hotkey: 'Win+Period', description: 'Windows Emoji Picker', severity: 'error' },
  { hotkey: 'Win+Semicolon', description: 'Windows Emoji Picker', severity: 'error' },
  { hotkey: 'Win+Space', description: 'Windows Language/Keyboard switcher', severity: 'error' },
  { hotkey: 'Win+Tab', description: 'Windows Task View', severity: 'error' },
  { hotkey: 'Alt+Tab', description: 'Windows App Switcher', severity: 'error' },
  { hotkey: 'Ctrl+Alt+Delete', description: 'Windows Security Options', severity: 'error' },
  { hotkey: 'Win+L', description: 'Windows Lock Screen', severity: 'error' },
  { hotkey: 'Win+D', description: 'Windows Show Desktop', severity: 'error' },
  { hotkey: 'Alt+F4', description: 'Windows Close Application', severity: 'warning' },
  { hotkey: 'Ctrl+Shift+Escape', description: 'Windows Task Manager', severity: 'error' },
];

const MACOS_CONFLICTS: ConflictInfo[] = [
  // macOS system hotkeys
  { hotkey: 'CommandOrControl+Space', description: 'macOS Spotlight Search', severity: 'error' },
  { hotkey: 'CommandOrControl+Tab', description: 'macOS App Switcher', severity: 'error' },
  { hotkey: 'CommandOrControl+Shift+3', description: 'macOS Screenshot', severity: 'warning' },
  { hotkey: 'CommandOrControl+Shift+4', description: 'macOS Screenshot Selection', severity: 'warning' },
  { hotkey: 'CommandOrControl+Shift+5', description: 'macOS Screenshot/Recording', severity: 'warning' },
  { hotkey: 'CommandOrControl+Option+Escape', description: 'macOS Force Quit', severity: 'error' },
  { hotkey: 'CommandOrControl+Q', description: 'macOS Quit Application', severity: 'warning' },
  { hotkey: 'CommandOrControl+W', description: 'macOS Close Window', severity: 'warning' },
  { hotkey: 'CommandOrControl+M', description: 'macOS Minimize Window', severity: 'warning' },
  { hotkey: 'CommandOrControl+H', description: 'macOS Hide Application', severity: 'warning' },
  { hotkey: 'Control+CommandOrControl+Q', description: 'macOS Lock Screen', severity: 'error' },
  { hotkey: 'Control+CommandOrControl+Space', description: 'macOS Emoji Picker', severity: 'warning' },
];

const LINUX_CONFLICTS: ConflictInfo[] = [
  // Common Linux desktop environment hotkeys
  { hotkey: 'Alt+Tab', description: 'Linux App Switcher', severity: 'error' },
  { hotkey: 'Alt+F4', description: 'Linux Close Window', severity: 'warning' },
  { hotkey: 'Super+Space', description: 'Linux Application Launcher (varies by DE)', severity: 'warning' },
  { hotkey: 'Super+L', description: 'Linux Lock Screen (varies by DE)', severity: 'warning' },
  { hotkey: 'Ctrl+Alt+T', description: 'Linux Terminal (varies by DE)', severity: 'warning' },
  { hotkey: 'Ctrl+Alt+Delete', description: 'Linux System Monitor/Logout', severity: 'error' },
  { hotkey: 'Ctrl+Alt+F1', description: 'Linux TTY1', severity: 'error' },
  { hotkey: 'Ctrl+Alt+F2', description: 'Linux TTY2', severity: 'error' },
];

/**
 * Check if a hotkey conflicts with known system shortcuts
 * @param hotkey The normalized hotkey string to check
 * @param platform Optional platform override (defaults to current platform)
 * @returns Conflict information if found, null otherwise
 */
export function checkForSystemConflict(
  hotkey: string,
  platform?: 'windows' | 'macos' | 'linux'
): ConflictInfo | null {
  // Determine platform if not provided
  if (!platform) {
    if (typeof window !== 'undefined' && window.navigator) {
      const userAgent = window.navigator.userAgent.toLowerCase();
      if (userAgent.includes('win')) {
        platform = 'windows';
      } else if (userAgent.includes('mac')) {
        platform = 'macos';
      } else {
        platform = 'linux';
      }
    } else {
      // Default to windows if we can't detect
      platform = 'windows';
    }
  }

  // Get the appropriate conflict list
  let conflicts: ConflictInfo[];
  switch (platform) {
    case 'macos':
      conflicts = MACOS_CONFLICTS;
      break;
    case 'linux':
      conflicts = LINUX_CONFLICTS;
      break;
    case 'windows':
    default:
      conflicts = WINDOWS_CONFLICTS;
      break;
  }

  // Check for exact match (case-insensitive)
  const normalizedHotkey = hotkey.toLowerCase();
  const conflict = conflicts.find(c =>
    c.hotkey.toLowerCase() === normalizedHotkey
  );

  return conflict || null;
}

/**
 * Get all known conflicts for the current platform
 * @param platform Optional platform override
 * @returns Array of all known conflicts
 */
export function getAllConflicts(
  platform?: 'windows' | 'macos' | 'linux'
): ConflictInfo[] {
  if (!platform) {
    if (typeof window !== 'undefined' && window.navigator) {
      const userAgent = window.navigator.userAgent.toLowerCase();
      if (userAgent.includes('win')) {
        platform = 'windows';
      } else if (userAgent.includes('mac')) {
        platform = 'macos';
      } else {
        platform = 'linux';
      }
    } else {
      platform = 'windows';
    }
  }

  switch (platform) {
    case 'macos':
      return MACOS_CONFLICTS;
    case 'linux':
      return LINUX_CONFLICTS;
    case 'windows':
    default:
      return WINDOWS_CONFLICTS;
  }
}

/**
 * Format a conflict warning message
 * @param conflict The conflict information
 * @returns Formatted warning message
 */
export function formatConflictMessage(conflict: ConflictInfo): string {
  if (conflict.severity === 'error') {
    return `⚠️ This hotkey is reserved by the system: ${conflict.description}`;
  } else {
    return `ℹ️ This hotkey may conflict: ${conflict.description}`;
  }
}