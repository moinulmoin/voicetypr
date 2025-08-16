// This setup runs before any module imports to ensure mocks are in place
// It's critical for mocking things that are called at module initialization time

// Mock Tauri OS plugin BEFORE any modules import it
(window as any).__TAURI_OS_PLUGIN_INTERNALS__ = {
  os_type: 'macos',
  arch: 'aarch64',
  exe_extension: '',
  family: 'unix',
  version: '14.0.0',
  platform: 'darwin'
};

// Mock Tauri internals for IPC
(window as any).__TAURI_INTERNALS__ = {
  invoke: () => Promise.resolve(),
  transformCallback: (cb: any) => cb,
  metadata: {}
};