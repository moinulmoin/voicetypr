import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { ask } from '@tauri-apps/plugin-dialog';
import { sendNotification, isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import type { AppSettings } from '@/types';
import { createLogger } from "@/lib/logger";

const log = createLogger("update");

const UPDATE_CHECK_INTERVAL = 24 * 60 * 60 * 1000; // 24 hours in milliseconds
const LAST_UPDATE_CHECK_KEY = 'last_update_check';
const JUST_UPDATED_KEY = 'just_updated_version';

export class UpdateService {
  private static instance: UpdateService;
  private checkInProgress = false;
  private updateCheckTimer: number | null = null;
  private isSessionActive = false;
  private pendingRelaunch = false;
  private pendingUpdateVersion: string | null = null;

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
        log.debug('Backend still in active session, deferring relaunch');
        this.pendingRelaunch = true;
        return;
      }
    } catch (error) {
      log.error('Failed to check recording state, proceeding with relaunch:', error);
    }

    // Store the version we're updating to so the next launch can show it
    if (this.pendingUpdateVersion) {
      localStorage.setItem(JUST_UPDATED_KEY, this.pendingUpdateVersion);
    }

    try {
      await relaunch();
    } catch (error) {
      // Rollback: remove marker since relaunch failed
      localStorage.removeItem(JUST_UPDATED_KEY);
      log.error('Relaunch failed:', error);
      toast.error('Update installed. Please restart the app manually.', { 
        duration: 10000 
      });
    }
  }

  /**
   * Read and clear the just-updated version marker (one-shot).
   * Returns the version string if the app was just updated, then clears it.
   * Returns null if no marker exists.
   */
  getJustUpdatedVersion(): string | null {
    const version = localStorage.getItem(JUST_UPDATED_KEY);
    if (version) {
      localStorage.removeItem(JUST_UPDATED_KEY);
    }
    return version;
  }

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
    // Check if automatic update checks are enabled (default to true if not set)
    const autoUpdateEnabled = settings.check_updates_automatically ?? true;
    
    if (!autoUpdateEnabled) {
      log.debug('Automatic update checks are disabled');
      return;
    }

    // Check on startup and then daily. Background checks notify; they do not install.
    await this.checkForUpdatesInBackground();

    this.setupDailyUpdateCheck();
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
      log.error('Failed to request notification permission:', error);
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
      log.error('Failed to send system notification:', error);
    }
  }

  /**
   * Check for updates in the background.
   * Background checks notify the user; they never download or install automatically.
   * Runs on startup and daily when automatic update checks are enabled.
   */
  async checkForUpdatesInBackground(): Promise<void> {
    if (this.checkInProgress) {
      log.debug('Update check already in progress');
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
          log.debug('Skipping update check - too soon since last check');
          return;
        }
      }

      log.debug('Checking for updates in background...');
      const update = await check();
      
      // Update last check time
      localStorage.setItem(LAST_UPDATE_CHECK_KEY, Date.now().toString());
      
      if (update?.available) {
        await this.handleUpdateAvailable(update, true);
      }
    } catch (error) {
      log.error('Background update check failed:', error);
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
      log.error('Update check failed:', error);
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
      toast.info(`Update ${update.version} is available. Open Settings to install it.`);
      await this.sendSystemNotification(
        'Update Available',
        `VoiceTypr ${update.version} is ready to install from Settings.`
      );
      return;
    }

    await this.showUpdateDialog(update);
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
        this.pendingUpdateVersion = update.version;
        await this.performRelaunch();
      } catch (error) {
        log.error('Update installation failed:', error);
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

    log.debug('Daily update check scheduled');
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
