import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { ask } from '@tauri-apps/plugin-dialog';
import { sendNotification, isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import type { AppSettings } from '@/types';

const UPDATE_CHECK_INTERVAL = 24 * 60 * 60 * 1000; // 24 hours in milliseconds
const LAST_UPDATE_CHECK_KEY = 'last_update_check';
const JUST_UPDATED_KEY = 'just_updated_version';

export class UpdateService {
  private static instance: UpdateService;
  private checkInProgress = false;
  private updateCheckTimer: number | null = null;
  private isSessionActive = false;
  private pendingRelaunch = false;

  private constructor() {}

  /**
   * Set whether user is in an active session (recording/transcribing)
   * Auto-updates are skipped when session is active
   * When session ends, pending relaunch will be triggered
   */
  setSessionActive(active: boolean): void {
    this.isSessionActive = active;
    
    // If session ended and we have a pending relaunch, do it now
    if (!active && this.pendingRelaunch) {
      this.pendingRelaunch = false;
      this.performRelaunch();
    }
  }

  /**
   * Perform relaunch with backend verification and error handling
   */
  private async performRelaunch(): Promise<void> {
    // Final safety check: verify with backend that no session is active
    try {
      const currentState = await invoke<{ state: string }>('get_current_recording_state');
      const isActive = currentState.state !== 'idle' && currentState.state !== 'error';
      if (isActive) {
        console.log('Backend still in active session, deferring relaunch');
        this.pendingRelaunch = true;
        return;
      }
    } catch (error) {
      console.error('Failed to check recording state, proceeding with relaunch:', error);
    }

    // Mark that we're about to relaunch after update
    localStorage.setItem(JUST_UPDATED_KEY, 'true');

    try {
      await relaunch();
    } catch (error) {
      // Rollback: remove flag since relaunch failed
      localStorage.removeItem(JUST_UPDATED_KEY);
      console.error('Relaunch failed:', error);
      toast.error('Update installed. Please restart the app manually.', { 
        duration: 10000 
      });
    }
  }

  static getInstance(): UpdateService {
    if (!UpdateService.instance) {
      UpdateService.instance = new UpdateService();
    }
    return UpdateService.instance;
  }

  /**
   * Initialize the update service
   * - Shows "Updated" toast if app was just updated
   * - Checks for updates on startup if enabled
   * - Sets up daily update checks
   */
  async initialize(settings: AppSettings): Promise<void> {
    // Check if app was just updated and show toast
    await this.showJustUpdatedToast();

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
   * Show toast if app was just updated, with window focus
   */
  private async showJustUpdatedToast(): Promise<void> {
    const justUpdated = localStorage.getItem(JUST_UPDATED_KEY);
    if (justUpdated) {
      localStorage.removeItem(JUST_UPDATED_KEY);
      
      try {
        await invoke('focus_main_window');
        // Small delay to ensure window is focused before toast appears
        setTimeout(() => {
          toast.success('Updated to latest version', { duration: 5000 });
        }, 200);
      } catch {
        // If focus fails, still show the toast
        toast.success('Updated to latest version', { duration: 5000 });
      }
    }
  }

  /**
   * Request notification permission (call after onboarding)
   */
  async requestNotificationPermission(): Promise<boolean> {
    try {
      let permitted = await isPermissionGranted();
      if (!permitted) {
        const permission = await requestPermission();
        permitted = permission === 'granted';
      }
      // Send a welcome notification so user sees what they're allowing
      if (permitted) {
        sendNotification({ 
          title: 'Notifications Enabled', 
          body: "You'll be notified about app updates" 
        });
      }
      return permitted;
    } catch (error) {
      console.error('Failed to request notification permission:', error);
      return false;
    }
  }

  /**
   * Send a system notification (macOS notification center)
   */
  private async sendSystemNotification(title: string, body: string): Promise<void> {
    try {
      let permitted = await isPermissionGranted();
      if (!permitted) {
        const permission = await requestPermission();
        permitted = permission === 'granted';
      }
      if (permitted) {
        sendNotification({ title, body });
      }
    } catch (error) {
      console.error('Failed to send system notification:', error);
    }
  }

  /**
   * Check for updates in the background and auto-install if available.
   * Shows progress toasts during download/install, then relaunches the app.
   * Runs on startup and daily when automatic updates are enabled.
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
    if (isBackgroundCheck) {
      // For background checks, auto-install silently
      await this.autoInstallUpdate(update);
    } else {
      // For manual checks, show dialog to let user decide
      await this.showUpdateDialog(update);
    }
  }

  /**
   * Auto-install update silently with progress feedback
   */
  private async autoInstallUpdate(update: Update, retryCount = 0): Promise<void> {
    const MAX_RETRIES = 3;
    const RETRY_DELAY = 30000; // 30 seconds

    // Defer auto-update if user is in active session (recording/transcribing)
    if (this.isSessionActive) {
      if (retryCount < MAX_RETRIES) {
        console.log(`Deferring auto-update - session active (retry ${retryCount + 1}/${MAX_RETRIES} in 30s)`);
        setTimeout(() => this.autoInstallUpdate(update, retryCount + 1), RETRY_DELAY);
        return;
      }
      console.log('Skipping auto-update - session still active after max retries');
      return;
    }

    const toastId = 'update-progress';
    
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          toast.info(`Downloading update ${update.version}...`, { 
            id: toastId,
            duration: Infinity 
          });
        } else if (event.event === 'Finished') {
          toast.success('Update ready, restarting...', { 
            id: toastId,
            duration: Infinity 
          });
        }
      });
    } catch (error) {
      console.error('Auto-update failed:', error);
      toast.error('Update failed. You can try again from Settings > About.', { 
        id: toastId,
        duration: 5000 
      });
      return;
    }

    // Re-check session state before relaunch (user may have started recording during download)
    if (this.isSessionActive) {
      console.log('Update downloaded but session active - will relaunch when session ends');
      this.pendingRelaunch = true;
      toast.dismiss(toastId);
      await this.sendSystemNotification('Update Ready', 'VoiceTypr will restart when recording ends');
      return;
    }

    await this.performRelaunch();
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
        await update.downloadAndInstall();
        // Notify if relaunch will be deferred due to active session
        if (this.isSessionActive) {
          await this.sendSystemNotification('Update Ready', 'VoiceTypr will restart when recording ends');
        }
        await this.performRelaunch();
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
