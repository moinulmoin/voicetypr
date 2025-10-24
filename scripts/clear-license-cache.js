// Quick script to clear license cache using store-backed helper via Tauri command
import { invoke } from '@tauri-apps/api/core';

async function clearLicenseCache() {
  try {
    // We expose a lightweight command path: reuse backend invalidation
    await invoke('invalidate_license_cache');
    console.log('License cache cleared successfully');
  } catch (error) {
    console.error('Failed to clear cache:', error);
  }
}

clearLicenseCache();