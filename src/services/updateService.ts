import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { ask } from '@tauri-apps/plugin-dialog';
import { toast } from 'sonner';
import type { AppSettings } from '@/types';

const UPDATE_CHECK_INTERVAL = 24 * 60 * 60 * 1000; // 24 hours in milliseconds
const LAST_UPDATE_CHECK_KEY = 'last_update_check';

export class UpdateService {
  private static instance: UpdateService;
  private checkInProgress = false;
  private updateCheckTimer: number | null = null;

  private constructor() {}

  static getInstance(): UpdateService {
    if (!UpdateService.instance) {
      UpdateService.instance = new UpdateService();
    }
    return UpdateService.instance;
  }

  /**
   * Initialize the update service
   * - Checks for updates on startup if enabled
   * - Sets up daily update checks
   */
  async initialize(settings: AppSettings): Promise<void> {
    // Check if automatic updates are enabled (default to true if not set)
    const autoUpdateEnabled = settings.check_updates_automatically ?? true;
    
    if (!autoUpdateEnabled) {
      console.log('Automatic updates are disabled');
      return;
    }

    // Check on startup
    await this.checkForUpdatesInBackground();

    // Set up daily checks
    this.setupDailyUpdateCheck();
  }

  /**
   * Check for updates in the background (silent check)
   * Only shows notification if an update is available
   */
  async checkForUpdatesInBackground(): Promise<void> {
    if (this.checkInProgress) {
      console.log('Update check already in progress');
      return;
    }

    try {
      this.checkInProgress = true;
      
      // Check if we should skip based on last check time
      const lastCheck = localStorage.getItem(LAST_UPDATE_CHECK_KEY);
      if (lastCheck) {
        const lastCheckTime = parseInt(lastCheck, 10);
        const now = Date.now();
        if (now - lastCheckTime < UPDATE_CHECK_INTERVAL) {
          console.log('Skipping update check - too soon since last check');
          return;
        }
      }

      console.log('Checking for updates in background...');
      const update = await check();
      
      // Update last check time
      localStorage.setItem(LAST_UPDATE_CHECK_KEY, Date.now().toString());
      
      if (update?.available) {
        await this.handleUpdateAvailable(update, true);
      }
    } catch (error) {
      console.error('Background update check failed:', error);
      // Don't show error toast for background checks
    } finally {
      this.checkInProgress = false;
    }
  }

  /**
   * Check for updates with user feedback (manual check)
   */
  async checkForUpdatesManually(): Promise<void> {
    if (this.checkInProgress) {
      toast.info('Update check already in progress');
      return;
    }

    try {
      this.checkInProgress = true;
      toast.info('Checking for updates...');
      
      const update = await check();
      
      // Update last check time
      localStorage.setItem(LAST_UPDATE_CHECK_KEY, Date.now().toString());
      
      if (update?.available) {
        await this.handleUpdateAvailable(update, false);
      } else {
        toast.success("You're on the latest version!");
      }
    } catch (error) {
      console.error('Update check failed:', error);
      toast.error('Failed to check for updates');
    } finally {
      this.checkInProgress = false;
    }
  }

  /**
   * Handle when an update is available
   */
  private async handleUpdateAvailable(update: Update, isBackgroundCheck: boolean): Promise<void> {
    // For background checks, use a toast notification first
    if (isBackgroundCheck) {
      toast.info(`Update ${update.version} is available!`, {
        duration: 10000,
      });
    } else {
      // For manual checks, show dialog immediately
      await this.showUpdateDialog(update);
    }
  }

  /**
   * Show update dialog and handle user response
   */
  private async showUpdateDialog(update: Update): Promise<void> {
    const yes = await ask(
      `Update ${update.version} is available!\n\nRelease notes:\n${update.body}\n\nDo you want to download and install it now?`,
      {
        title: 'Update Available',
        kind: 'info',
        okLabel: 'Update',
        cancelLabel: 'Later'
      }
    );
    
    if (yes) {
      toast.info('Downloading update...');
      
      try {
        // Download and install
        await update.downloadAndInstall();
        
        // Relaunch the app
        await relaunch();
      } catch (error) {
        console.error('Update installation failed:', error);
        toast.error('Failed to install update');
      }
    }
  }

  /**
   * Set up daily update checks
   */
  private setupDailyUpdateCheck(): void {
    // Clear any existing timer
    if (this.updateCheckTimer) {
      clearInterval(this.updateCheckTimer);
    }

    // Set up interval for daily checks
    this.updateCheckTimer = window.setInterval(() => {
      this.checkForUpdatesInBackground();
    }, UPDATE_CHECK_INTERVAL);

    console.log('Daily update check scheduled');
  }

  /**
   * Clean up resources
   */
  dispose(): void {
    if (this.updateCheckTimer) {
      clearInterval(this.updateCheckTimer);
      this.updateCheckTimer = null;
    }
  }
}

// Export singleton instance
export const updateService = UpdateService.getInstance();
