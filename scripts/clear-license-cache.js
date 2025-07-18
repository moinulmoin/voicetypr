// Quick script to clear license cache
import { remove } from 'tauri-plugin-cache-api';

async function clearLicenseCache() {
  try {
    await remove('license_status');
    console.log('License cache cleared successfully');
  } catch (error) {
    console.error('Failed to clear cache:', error);
  }
}

clearLicenseCache();