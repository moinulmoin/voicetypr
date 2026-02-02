//! Cross-platform media pause controller.
//!
//! Pauses system media when recording starts and resumes when recording stops.
//! Only resumes if WE paused it (not if user manually paused during recording).

use std::sync::atomic::{AtomicBool, Ordering};

/// Controller for pausing/resuming system media during voice recording.
pub struct MediaPauseController {
    /// Tracks if we paused the media (so we know whether to resume)
    was_playing_before_recording: AtomicBool,
}

impl Default for MediaPauseController {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaPauseController {
    pub fn new() -> Self {
        Self {
            was_playing_before_recording: AtomicBool::new(false),
        }
    }

    /// Pause media if currently playing. Call when recording starts.
    /// Returns true if media was paused.
    pub fn pause_if_playing(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.pause_if_playing_macos()
        }

        #[cfg(target_os = "windows")]
        {
            self.pause_if_playing_windows()
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            log::debug!("Media pause not supported on this platform");
            false
        }
    }

    /// Resume media if we paused it. Call when recording stops.
    /// Returns true if media was resumed.
    pub fn resume_if_we_paused(&self) -> bool {
        if self.was_playing_before_recording.swap(false, Ordering::SeqCst) {
            #[cfg(target_os = "macos")]
            {
                return self.resume_macos();
            }

            #[cfg(target_os = "windows")]
            {
                return self.resume_windows();
            }

            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                false
            }
        } else {
            false
        }
    }

    /// Reset state without resuming (e.g., if app is closing)
    pub fn reset(&self) {
        self.was_playing_before_recording.store(false, Ordering::SeqCst);
    }
}

// ============================================
// macOS Implementation (media-remote crate)
// ============================================
#[cfg(target_os = "macos")]
impl MediaPauseController {
    fn pause_if_playing_macos(&self) -> bool {
        use media_remote::{Controller, NowPlayingPerl};

        let now_playing = NowPlayingPerl::new();

        // Check if media is currently playing
        let is_playing = {
            let guard = now_playing.get_info();
            if let Some(info) = guard.as_ref() {
                info.is_playing == Some(true)
            } else {
                false
            }
        };

        if is_playing {
            log::info!("🎵 Media is playing, pausing for recording...");
            self.was_playing_before_recording.store(true, Ordering::SeqCst);

            if now_playing.pause() {
                log::info!("✅ Media paused successfully");
                true
            } else {
                log::warn!("⚠️ Failed to pause media");
                self.was_playing_before_recording.store(false, Ordering::SeqCst);
                false
            }
        } else {
            log::debug!("No media playing, nothing to pause");
            self.was_playing_before_recording.store(false, Ordering::SeqCst);
            false
        }
    }

    fn resume_macos(&self) -> bool {
        use media_remote::{Controller, NowPlayingPerl};

        log::info!("🎵 Resuming media playback...");
        let now_playing = NowPlayingPerl::new();

        if now_playing.play() {
            log::info!("✅ Media resumed successfully");
            true
        } else {
            log::warn!("⚠️ Failed to resume media");
            false
        }
    }
}

// ============================================
// Windows Implementation (GSMTC - Global System Media Transport Controls)
// ============================================
// Uses Windows.Media.Control APIs to properly detect playback state
// and use explicit pause/play (not toggle). Requires Windows 10 1809+.
#[cfg(target_os = "windows")]
impl MediaPauseController {
    fn pause_if_playing_windows(&self) -> bool {
        use windows::Media::Control::{
            GlobalSystemMediaTransportControlsSessionManager,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus,
        };

        // Get the session manager (blocking wait with .join())
        let manager = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
            Ok(op) => match op.join() {
                Ok(mgr) => mgr,
                Err(e) => {
                    log::warn!("Failed to get GSMTC session manager: {:?}", e);
                    return false;
                }
            },
            Err(e) => {
                log::warn!("Failed to request GSMTC session manager: {:?}", e);
                return false;
            }
        };

        // Get current session (the app currently controlling media)
        let session = match manager.GetCurrentSession() {
            Ok(s) => s,
            Err(_) => {
                log::debug!("No active media session found");
                return false;
            }
        };

        // Check playback status
        let is_playing = match session.GetPlaybackInfo() {
            Ok(info) => match info.PlaybackStatus() {
                Ok(status) => {
                    status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
                }
                Err(_) => false,
            },
            Err(_) => false,
        };

        if !is_playing {
            log::debug!("Media not playing, nothing to pause");
            self.was_playing_before_recording.store(false, Ordering::SeqCst);
            return false;
        }

        log::info!("Media is playing, pausing for recording...");
        self.was_playing_before_recording.store(true, Ordering::SeqCst);

        // Use explicit pause (not toggle!)
        match session.TryPauseAsync() {
            Ok(op) => match op.join() {
                Ok(success) => {
                    if success {
                        log::info!("Media paused successfully via GSMTC");
                        true
                    } else {
                        log::warn!("GSMTC TryPauseAsync returned false");
                        self.was_playing_before_recording.store(false, Ordering::SeqCst);
                        false
                    }
                }
                Err(e) => {
                    log::warn!("Failed to pause media: {:?}", e);
                    self.was_playing_before_recording.store(false, Ordering::SeqCst);
                    false
                }
            },
            Err(e) => {
                log::warn!("Failed to request pause: {:?}", e);
                self.was_playing_before_recording.store(false, Ordering::SeqCst);
                false
            }
        }
    }

    fn resume_windows(&self) -> bool {
        use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

        log::info!("Resuming media playback via GSMTC...");

        // Get the session manager (blocking wait with .join())
        let manager = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
            Ok(op) => match op.join() {
                Ok(mgr) => mgr,
                Err(e) => {
                    log::warn!("Failed to get GSMTC session manager for resume: {:?}", e);
                    return false;
                }
            },
            Err(e) => {
                log::warn!("Failed to request GSMTC session manager for resume: {:?}", e);
                return false;
            }
        };

        // Get current session
        let session = match manager.GetCurrentSession() {
            Ok(s) => s,
            Err(_) => {
                log::warn!("No active media session found for resume");
                return false;
            }
        };

        // Use explicit play (not toggle!)
        match session.TryPlayAsync() {
            Ok(op) => match op.join() {
                Ok(success) => {
                    if success {
                        log::info!("Media resumed successfully via GSMTC");
                        true
                    } else {
                        log::warn!("GSMTC TryPlayAsync returned false");
                        false
                    }
                }
                Err(e) => {
                    log::warn!("Failed to resume media: {:?}", e);
                    false
                }
            },
            Err(e) => {
                log::warn!("Failed to request play: {:?}", e);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controller_creation() {
        let controller = MediaPauseController::new();
        assert!(!controller.was_playing_before_recording.load(Ordering::SeqCst));
    }

    #[test]
    fn test_default_impl() {
        let controller = MediaPauseController::default();
        assert!(!controller.was_playing_before_recording.load(Ordering::SeqCst));
    }

    #[test]
    fn test_resume_without_pause_does_nothing() {
        let controller = MediaPauseController::new();
        // Should return false since we didn't pause anything
        assert!(!controller.resume_if_we_paused());
    }

    #[test]
    fn test_resume_clears_was_playing_flag() {
        let controller = MediaPauseController::new();
        // Manually set the flag to true
        controller.was_playing_before_recording.store(true, Ordering::SeqCst);
        
        // Resume should clear the flag (swap returns old value)
        // Note: actual resume behavior depends on platform APIs
        let _ = controller.resume_if_we_paused();
        
        // Flag should be cleared after resume attempt
        assert!(!controller.was_playing_before_recording.load(Ordering::SeqCst));
    }

    #[test]
    fn test_reset() {
        let controller = MediaPauseController::new();
        controller.was_playing_before_recording.store(true, Ordering::SeqCst);
        controller.reset();
        assert!(!controller.was_playing_before_recording.load(Ordering::SeqCst));
    }

    #[test]
    fn test_multiple_resets_are_safe() {
        let controller = MediaPauseController::new();
        controller.reset();
        controller.reset();
        controller.reset();
        assert!(!controller.was_playing_before_recording.load(Ordering::SeqCst));
    }

    #[test]
    fn test_was_playing_flag_is_atomic() {
        use std::sync::Arc;
        use std::thread;

        let controller = Arc::new(MediaPauseController::new());
        let mut handles = vec![];

        // Spawn multiple threads toggling the flag
        for i in 0..10 {
            let c = Arc::clone(&controller);
            handles.push(thread::spawn(move || {
                c.was_playing_before_recording.store(i % 2 == 0, Ordering::SeqCst);
                c.was_playing_before_recording.load(Ordering::SeqCst)
            }));
        }

        // All threads should complete without panic
        for handle in handles {
            let _ = handle.join().unwrap();
        }
    }
}
