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
// Windows Implementation (Media Key Simulation)
// ============================================
// Note: Windows implementation uses key simulation (VK_MEDIA_PLAY_PAUSE)
// which toggles playback. Unlike macOS, we can't detect if media was playing,
// so this may accidentally start paused media. This is a known limitation.
#[cfg(target_os = "windows")]
impl MediaPauseController {
    fn pause_if_playing_windows(&self) -> bool {
        log::info!("🎵 Sending media pause key (Windows)...");
        
        // On Windows, we can't reliably detect if media is playing without complex WinRT APIs
        // So we just send the play/pause key and hope for the best
        // We always mark as "was playing" so we'll toggle back on resume
        self.was_playing_before_recording.store(true, Ordering::SeqCst);
        
        if self.send_media_play_pause_key() {
            log::info!("✅ Media play/pause key sent");
            true
        } else {
            log::warn!("⚠️ Failed to send media key");
            self.was_playing_before_recording.store(false, Ordering::SeqCst);
            false
        }
    }

    fn resume_windows(&self) -> bool {
        log::info!("🎵 Sending media play key (Windows)...");
        
        if self.send_media_play_pause_key() {
            log::info!("✅ Media play/pause key sent");
            true
        } else {
            log::warn!("⚠️ Failed to send media key");
            false
        }
    }

    fn send_media_play_pause_key(&self) -> bool {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
            KEYEVENTF_KEYUP, VIRTUAL_KEY,
        };

        const VK_MEDIA_PLAY_PAUSE: u16 = 0xB3;

        let mut inputs = [
            // Key down
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(VK_MEDIA_PLAY_PAUSE),
                        wScan: 0,
                        dwFlags: KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // Key up
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(VK_MEDIA_PLAY_PAUSE),
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        let sent = unsafe {
            SendInput(&mut inputs, std::mem::size_of::<INPUT>() as i32)
        };

        sent == 2
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
    fn test_resume_without_pause_does_nothing() {
        let controller = MediaPauseController::new();
        // Should return false since we didn't pause anything
        assert!(!controller.resume_if_we_paused());
    }

    #[test]
    fn test_reset() {
        let controller = MediaPauseController::new();
        controller.was_playing_before_recording.store(true, Ordering::SeqCst);
        controller.reset();
        assert!(!controller.was_playing_before_recording.load(Ordering::SeqCst));
    }
}
