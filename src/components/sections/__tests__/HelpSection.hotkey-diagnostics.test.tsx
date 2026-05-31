import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn() }));
vi.mock('@tauri-apps/plugin-os', () => ({
  platform: vi.fn(() => 'macos'),
  type: vi.fn(() => 'macos'),
  version: vi.fn(() => '15.0'),
}));
vi.mock('@tauri-apps/plugin-shell', () => ({ open: vi.fn() }));
import { formatHotkeyDiagnosticContext } from '../HelpSection';

describe('formatHotkeyDiagnosticContext', () => {
  it('formats hotkey registration and event fields for support reports', () => {
    const context = formatHotkeyDiagnosticContext({
      configuredHotkey: 'CommandOrControl+Shift+Space',
      normalizedHotkey: 'CommandOrControl+Shift+Space',
      recordingMode: 'toggle',
      useDifferentPttKey: false,
      pttHotkey: null,
      normalizedPttHotkey: null,
      registrationStatus: 'registered',
      registrationError: null,
      lastRegistrationAttemptAt: '2026-05-31T00:00:00Z',
      lastSuccessfulRegistrationAt: '2026-05-31T00:00:01Z',
      lastEventAt: '2026-05-31T00:00:02Z',
      lastEventKind: 'recording',
      lastEventState: 'pressed',
      eventCount: 3,
      currentRecordingState: 'idle',
      generatedAt: '2026-05-31T00:00:03Z',
      isRegistered: true,
    });

    expect(context).toContain('Configured Hotkey: CommandOrControl+Shift+Space');
    expect(context).toContain('Registration Status: registered');
    expect(context).toContain('Is Registered: true');
    expect(context).toContain('Last Event: recording (pressed) at 2026-05-31T00:00:02Z');
    expect(context).toContain('Event Count: 3');
  });
});
