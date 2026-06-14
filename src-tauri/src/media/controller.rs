//! Cross-platform media pause controller.
//!
//! Pauses system media when recording starts and resumes when recording stops.
//! Only resumes if WE paused it (not if user manually paused during recording).

use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "windows")]
use parking_lot::Mutex;

#[cfg(target_os = "macos")]
use std::{
    io::Write,
    process::{Command as ProcessCommand, Stdio},
};

#[cfg(target_os = "macos")]
const NOW_PLAYING_JXA_SCRIPT: &str = r#"
function run() {
  const MediaRemote = $.NSBundle.bundleWithPath(
    "/System/Library/PrivateFrameworks/MediaRemote.framework/",
  );
  MediaRemote.load;

  const MRNowPlayingRequest = $.NSClassFromString("MRNowPlayingRequest");
  const client = MRNowPlayingRequest.localNowPlayingPlayerPath.client;
  const clientConverted = {
    bundleIdentifier: client.bundleIdentifier.js,
    parentApplicationBundleIdentifier:
      client.parentApplicationBundleIdentifier.js,
  };

  const infoDict = MRNowPlayingRequest.localNowPlayingItem.nowPlayingInfo;
  const infoConverted = {};
  for (const key in infoDict.js) {
    const value = infoDict.valueForKey(key).js;
    if (typeof value !== "object") {
      infoConverted[key] = value;
    } else if (value && typeof value.getTime === "function") {
      try {
        infoConverted[key] = value.getTime();
      } catch (e) {
        infoConverted[key] = value.toString();
      }
    } else {
      infoConverted[key] = value.toString();
    }
  }

  return JSON.stringify({
    isPlaying: MRNowPlayingRequest.localIsPlaying,
    client: clientConverted,
    info: infoConverted,
  });
}
"#;

#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
struct NowPlayingSnapshot {
    is_playing: Option<bool>,
}

#[cfg(target_os = "macos")]
fn now_playing_snapshot_via_osascript() -> Option<NowPlayingSnapshot> {
    let mut child = ProcessCommand::new("/usr/bin/osascript")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("-l")
        .arg("JavaScript")
        .spawn()
        .ok()?;

    {
        let stdin = child.stdin.as_mut()?;
        stdin.write_all(NOW_PLAYING_JXA_SCRIPT.as_bytes()).ok()?;
    }

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        if log::log_enabled!(log::Level::Debug) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            let stderr = stderr.trim();
            let stdout = stdout.trim();

            let stderr_trunc: String = stderr.chars().take(400).collect();
            let stdout_trunc: String = stdout.chars().take(400).collect();

            log::debug!(
                "osascript now playing query failed | status={:?} stdout={:?} stderr={:?}",
                output.status,
                stdout_trunc,
                stderr_trunc
            );
        }
        return None;
    }

    let raw: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(err) => {
            if log::log_enabled!(log::Level::Debug) {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);

                let stderr = stderr.trim();
                let stdout = stdout.trim();

                let stderr_trunc: String = stderr.chars().take(400).collect();
                let stdout_trunc: String = stdout.chars().take(400).collect();

                log::debug!(
                    "osascript now playing JSON parse failed | error={:?} stdout={:?} stderr={:?}",
                    err,
                    stdout_trunc,
                    stderr_trunc
                );
            }

            return None;
        }
    };
    let is_playing = raw.get("isPlaying").and_then(|v| v.as_bool());

    Some(NowPlayingSnapshot { is_playing })
}

/// Controller for pausing/resuming system media during voice recording.
pub struct MediaPauseController {
    /// Tracks if we paused the media (so we know whether to resume)
    was_playing_before_recording: AtomicBool,

    /// On Windows, track which media session we paused so we only resume the same session.
    #[cfg(target_os = "windows")]
    paused_session_source_app_user_model_id: Mutex<Option<String>>,
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
            #[cfg(target_os = "windows")]
            paused_session_source_app_user_model_id: Mutex::new(None),
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
        if self
            .was_playing_before_recording
            .swap(false, Ordering::SeqCst)
        {
            #[cfg(target_os = "macos")]
            {
                self.resume_macos()
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
    #[allow(dead_code)]
    pub fn reset(&self) {
        self.was_playing_before_recording
            .store(false, Ordering::SeqCst);

        #[cfg(target_os = "windows")]
        {
            *self.paused_session_source_app_user_model_id.lock() = None;
        }
    }
}

// ============================================
#[cfg(target_os = "macos")]
impl MediaPauseController {
    fn pause_if_playing_macos(&self) -> bool {
        let snapshot = now_playing_snapshot_via_osascript();
        let is_playing = snapshot
            .as_ref()
            .and_then(|s| s.is_playing)
            .unwrap_or(false);

        if !is_playing {
            log::debug!("No media playing, nothing to pause");
            self.was_playing_before_recording
                .store(false, Ordering::SeqCst);
            return false;
        }

        log::info!("🎵 Media is playing, pausing for recording...");

        if toggle_media_playback_via_osascript() {
            log::info!("✅ Media paused successfully");
            self.was_playing_before_recording
                .store(true, Ordering::SeqCst);
            true
        } else {
            log::warn!("⚠️ Failed to pause media");
            self.was_playing_before_recording
                .store(false, Ordering::SeqCst);
            false
        }
    }

    fn resume_macos(&self) -> bool {
        if now_playing_snapshot_via_osascript()
            .and_then(|s| s.is_playing)
            .unwrap_or(false)
        {
            log::debug!("Media already playing (osascript), skipping resume");
            return false;
        }

        log::info!("🎵 Resuming media playback...");
        if toggle_media_playback_via_osascript() {
            log::info!("✅ Media resumed successfully");
            true
        } else {
            log::warn!("⚠️ Failed to resume media");
            false
        }
    }
}

#[cfg(target_os = "macos")]
fn toggle_media_playback_via_osascript() -> bool {
    let output = ProcessCommand::new("/usr/bin/osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to key code 100")
        .output();

    match output {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            log::warn!(
                "osascript media toggle failed | status={:?} stdout={:?} stderr={:?}",
                output.status,
                stdout,
                stderr
            );
            false
        }
        Err(err) => {
            log::warn!("Failed to execute osascript media toggle: {}", err);
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
    fn try_pause_session(
        &self,
        session: &windows::Media::Control::GlobalSystemMediaTransportControlsSession,
    ) -> bool {
        let source_app_id = session.SourceAppUserModelId().ok().map(|id| id.to_string());
        match session.TryPauseAsync() {
            Ok(op) => match op.join() {
                Ok(true) => {
                    log::info!("Media paused successfully via GSMTC");
                    self.was_playing_before_recording
                        .store(true, Ordering::SeqCst);
                    *self.paused_session_source_app_user_model_id.lock() = source_app_id;
                    true
                }
                Ok(false) => {
                    log::info!("GSMTC TryPauseAsync returned false; trying next candidate");
                    false
                }
                Err(e) => {
                    log::warn!("Failed to pause media: {:?}; trying next candidate", e);
                    false
                }
            },
            Err(e) => {
                log::warn!("Failed to request pause: {:?}; trying next candidate", e);
                false
            }
        }
    }

    fn pause_if_playing_windows(&self) -> bool {
        use std::{thread, time::Duration};
        use windows::Media::Control::{
            GlobalSystemMediaTransportControlsSession,
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

        fn is_playing(session: &GlobalSystemMediaTransportControlsSession) -> bool {
            let playback_info = match session.GetPlaybackInfo() {
                Ok(info) => info,
                Err(_) => return false,
            };

            let status = match playback_info.PlaybackStatus() {
                Ok(status) => status,
                Err(_) => return false,
            };

            status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
        }

        fn timeline_position_ticks(
            session: &GlobalSystemMediaTransportControlsSession,
        ) -> Option<i64> {
            let timeline = session.GetTimelineProperties().ok()?;
            Some(timeline.Position().ok()?.Duration)
        }

        let mut current_session = manager.GetCurrentSession().ok();
        let current_id = current_session
            .as_ref()
            .and_then(|s| s.SourceAppUserModelId().ok().map(|id| id.to_string()));

        let mut all: Vec<(String, GlobalSystemMediaTransportControlsSession)> = Vec::new();
        if let Ok(sessions) = manager.GetSessions() {
            if let Ok(size) = sessions.Size() {
                for i in 0..size {
                    let session = match sessions.GetAt(i) {
                        Ok(session) => session,
                        Err(_) => continue,
                    };

                    // Keep the session even if its app id is unreadable (id is only
                    // used for dedup/ordering; resume bookkeeping is re-read on pause),
                    // so a playing-but-id-less session is still attempted.
                    let id = session
                        .SourceAppUserModelId()
                        .ok()
                        .map(|id| id.to_string())
                        .unwrap_or_default();

                    all.push((id, session));
                }
            }
        }

        if let Some(cid) = current_id.clone() {
            if !all.iter().any(|(id, _)| id == &cid) {
                if let Some(session) = current_session.take() {
                    all.push((cid, session));
                }
            }
        }

        let mut attempted: Vec<usize> = Vec::new();
        let mut had_reported_candidates = false;

        let mut phase1_order: Vec<usize> = Vec::new();
        if let Some(ref cid) = current_id {
            if let Some(idx) = all.iter().position(|(id, _)| id == cid) {
                phase1_order.push(idx);
            }
        }
        for (idx, (id, _)) in all.iter().enumerate() {
            if current_id.as_ref() != Some(id) {
                phase1_order.push(idx);
            }
        }

        for &idx in &phase1_order {
            let session = &all[idx].1;
            if !is_playing(session) {
                continue;
            }
            if !had_reported_candidates {
                had_reported_candidates = true;
                log::info!("Media is playing, pausing for recording...");
            }
            attempted.push(idx);
            if self.try_pause_session(session) {
                return true;
            }
        }

        // Fallback: some sessions occasionally report non-Playing states even while audio is
        // progressing. If we can observe timeline position advancing over a short interval,
        // treat it as playing and pause it.
        let mut timeline_probe: Vec<(usize, i64)> = Vec::new();
        for (idx, (_, session)) in all.iter().enumerate() {
            if attempted.contains(&idx) {
                continue;
            }
            let pos = timeline_position_ticks(session).unwrap_or(0);
            timeline_probe.push((idx, pos));
        }

        let mut timeline_candidates: Vec<usize> = Vec::new();
        let had_timeline_probe_entries = !timeline_probe.is_empty();

        if had_timeline_probe_entries {
            if let Some(ref current_session_id) = current_id {
                if let Some(pos) = timeline_probe
                    .iter()
                    .position(|(idx, _)| all[*idx].0 == *current_session_id)
                {
                    let current = timeline_probe.remove(pos);
                    timeline_probe.insert(0, current);
                }
            }

            thread::sleep(Duration::from_millis(120));

            // 1 tick = 100ns, so 50ms = 500_000 ticks.
            const DELTA_THRESHOLD_TICKS: i64 = 50 * 10_000;

            for (idx, before) in timeline_probe {
                let (id, session) = &all[idx];
                let after = timeline_position_ticks(session).unwrap_or(before);
                let delta = after.saturating_sub(before);

                if delta > DELTA_THRESHOLD_TICKS {
                    log::info!(
                        "Inferred playing session via timeline movement | source_app_id={} delta_ms={}",
                        id,
                        delta / 10_000
                    );
                    timeline_candidates.push(idx);
                }
            }
        }
        let had_timeline_candidates = !timeline_candidates.is_empty();
        for idx in timeline_candidates {
            let session = &all[idx].1;
            if self.try_pause_session(session) {
                return true;
            }
        }

        self.was_playing_before_recording
            .store(false, Ordering::SeqCst);
        *self.paused_session_source_app_user_model_id.lock() = None;

        if !had_reported_candidates && !had_timeline_candidates {
            log::info!("No playing media session found");
        } else {
            log::info!("No media session could be paused");
        }
        false
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
                log::warn!(
                    "Failed to request GSMTC session manager for resume: {:?}",
                    e
                );
                return false;
            }
        };

        let paused_id = self.paused_session_source_app_user_model_id.lock().take();

        let session = if let Some(paused_id) = paused_id {
            let sessions = match manager.GetSessions() {
                Ok(sessions) => sessions,
                Err(e) => {
                    log::warn!("Failed to enumerate GSMTC sessions for resume: {:?}", e);
                    return false;
                }
            };

            let size = match sessions.Size() {
                Ok(size) => size,
                Err(e) => {
                    log::warn!("Failed to read GSMTC sessions size for resume: {:?}", e);
                    return false;
                }
            };

            let mut found = None;
            for i in 0..size {
                let session = match sessions.GetAt(i) {
                    Ok(session) => session,
                    Err(_) => continue,
                };

                let session_id = match session.SourceAppUserModelId() {
                    Ok(id) => id.to_string(),
                    Err(_) => continue,
                };

                if session_id == paused_id {
                    found = Some(session);
                    break;
                }
            }

            match found {
                Some(session) => session,
                None => {
                    log::info!("Paused media session is no longer available; skipping resume");
                    return false;
                }
            }
        } else {
            match manager.GetCurrentSession() {
                Ok(session) => session,
                Err(_) => {
                    log::warn!("No active media session found for resume");
                    return false;
                }
            }
        };

        // If it is already playing, don't send play.
        if let Ok(playback_info) = session.GetPlaybackInfo() {
            if let Ok(status) = playback_info.PlaybackStatus() {
                use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus;
                if status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing {
                    log::info!("Media already playing, skipping resume");
                    return false;
                }
            }
        }

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
        assert!(!controller
            .was_playing_before_recording
            .load(Ordering::SeqCst));
    }

    #[test]
    fn test_default_impl() {
        let controller = MediaPauseController::default();
        assert!(!controller
            .was_playing_before_recording
            .load(Ordering::SeqCst));
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
        controller
            .was_playing_before_recording
            .store(true, Ordering::SeqCst);

        // Resume should clear the flag (swap returns old value)
        // Note: actual resume behavior depends on platform APIs
        let _ = controller.resume_if_we_paused();

        // Flag should be cleared after resume attempt
        assert!(!controller
            .was_playing_before_recording
            .load(Ordering::SeqCst));
    }

    #[test]
    fn test_reset() {
        let controller = MediaPauseController::new();
        controller
            .was_playing_before_recording
            .store(true, Ordering::SeqCst);
        controller.reset();
        assert!(!controller
            .was_playing_before_recording
            .load(Ordering::SeqCst));
    }

    #[test]
    fn test_multiple_resets_are_safe() {
        let controller = MediaPauseController::new();
        controller.reset();
        controller.reset();
        controller.reset();
        assert!(!controller
            .was_playing_before_recording
            .load(Ordering::SeqCst));
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
                c.was_playing_before_recording
                    .store(i % 2 == 0, Ordering::SeqCst);
                c.was_playing_before_recording.load(Ordering::SeqCst)
            }));
        }

        // All threads should complete without panic
        for handle in handles {
            let _ = handle.join().unwrap();
        }
    }

    #[test]
    fn test_parking_lot_mutex_survives_panic() {
        use parking_lot::Mutex;
        use std::sync::Arc;
        use std::thread;

        let mutex = Arc::new(Mutex::new(Some(String::from("test-value"))));
        let mutex_clone = Arc::clone(&mutex);

        // Spawn a thread that modifies the value, then panics while holding the lock.
        // If the mutex poisoned, the write would be lost and lock() would fail.
        let handle = thread::spawn(move || {
            let mut guard = mutex_clone.lock();
            *guard = Some(String::from("modified-before-panic"));
            panic!("intentional test panic");
        });

        // The thread should have panicked
        let result = handle.join();
        assert!(result.is_err(), "Thread should have panicked");

        // parking_lot::Mutex should NOT be poisoned — we can still lock it,
        // and the write from the panicking thread should be visible.
        let value = mutex.lock().clone();
        assert_eq!(value, Some(String::from("modified-before-panic")));
    }
}
