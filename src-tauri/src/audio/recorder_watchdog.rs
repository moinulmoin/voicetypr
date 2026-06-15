use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::commands::audio::{stop_recording, RecorderState};
use crate::{get_recording_state, RecordingState};

/// Background watcher that recovers recorder workers that have exited while
/// app state is still `Recording`.
pub struct RecorderWatchdog {
    stop: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
    handle: Mutex<Option<thread::JoinHandle<()>>>,
    app: AppHandle,
}

impl RecorderWatchdog {
    pub fn new(app: AppHandle) -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            started: Arc::new(AtomicBool::new(false)),
            handle: Mutex::new(None),
            app,
        }
    }

    /// Start the watchdog on a dedicated thread.
    /// Safe to call multiple times - will no-op if already running.
    pub fn start(&self) {
        if self.started.swap(true, Ordering::SeqCst) {
            log::debug!("RecorderWatchdog already running, skipping start");
            return;
        }

        log::info!("Starting RecorderWatchdog");

        let stop_flag = self.stop.clone();
        let app = self.app.clone();

        let handle = thread::spawn(move || {
            let mut auto_stop_dispatched = false;

            while !stop_flag.load(Ordering::Relaxed) {
                let state = get_recording_state(&app);

                // Probe the recorder mutex only when a stop could be dispatched,
                // avoiding per-tick lock contention while not recording.
                let worker_finished = if state == RecordingState::Recording && !auto_stop_dispatched
                {
                    app.state::<RecorderState>()
                        .inner()
                        .0
                        .lock()
                        .map(|recorder| recorder.recording_thread_finished())
                        .unwrap_or(false)
                } else {
                    false
                };

                let (dispatch_stop, next_dispatched) =
                    watchdog_tick(state, worker_finished, auto_stop_dispatched);
                auto_stop_dispatched = next_dispatched;

                if dispatch_stop {
                    log::info!(
                        "RecorderWatchdog: recorder thread finished while state=Recording; driving stop flow"
                    );

                    let app_for_stop = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let recorder_state = app_for_stop.state::<RecorderState>();
                        if let Err(err) = stop_recording(app_for_stop.clone(), recorder_state).await
                        {
                            log::error!("RecorderWatchdog auto-stop failed: {}", err);
                        }
                    });
                }

                thread::sleep(Duration::from_millis(250));
            }
        });

        if let Ok(mut guard) = self.handle.lock() {
            *guard = Some(handle);
        }
    }
}

/// Per-tick decision for the recorder watchdog, extracted as a pure function so
/// the re-arm / once-per-session / state-gate behavior is unit-testable without
/// an `AppHandle`. Returns `(dispatch_stop, next_dispatched)`.
///
/// Leaving `Recording` re-arms the watchdog for the next session; within a
/// `Recording` session we drive `stop` at most once, and only after the worker
/// thread has actually finished.
fn watchdog_tick(state: RecordingState, worker_finished: bool, dispatched: bool) -> (bool, bool) {
    if state != RecordingState::Recording {
        return (false, false);
    }
    if dispatched {
        return (false, true);
    }
    if worker_finished {
        (true, true)
    } else {
        (false, false)
    }
}

impl Drop for RecorderWatchdog {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);

        if let Ok(mut guard) = self.handle.lock() {
            if let Some(handle) = guard.take() {
                if let Err(err) = handle.join() {
                    log::debug!("RecorderWatchdog thread join failed: {:?}", err);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::watchdog_tick;
    use crate::RecordingState;

    #[test]
    fn never_dispatches_outside_recording_and_clears_latch() {
        for state in [
            RecordingState::Idle,
            RecordingState::Starting,
            RecordingState::Stopping,
            RecordingState::Transcribing,
            RecordingState::Error,
        ] {
            // Even a carried-in finished+dispatched latch is reset, never fires.
            assert_eq!(watchdog_tick(state, true, true), (false, false));
            assert_eq!(watchdog_tick(state, false, false), (false, false));
        }
    }

    #[test]
    fn waits_while_recording_until_worker_finishes() {
        assert_eq!(
            watchdog_tick(RecordingState::Recording, false, false),
            (false, false)
        );
    }

    #[test]
    fn dispatches_once_when_worker_finishes_while_recording() {
        assert_eq!(
            watchdog_tick(RecordingState::Recording, true, false),
            (true, true)
        );
    }

    #[test]
    fn does_not_double_dispatch_within_a_recording_session() {
        assert_eq!(
            watchdog_tick(RecordingState::Recording, true, true),
            (false, true)
        );
    }

    #[test]
    fn re_arms_after_leaving_and_re_entering_recording() {
        // Session 1: worker finishes -> dispatch + latch.
        let (d1, latch) = watchdog_tick(RecordingState::Recording, true, false);
        assert!(d1 && latch);
        // Still recording, already dispatched -> no repeat, latch held.
        let (d2, latch) = watchdog_tick(RecordingState::Recording, true, latch);
        assert!(!d2 && latch);
        // Recording ended -> latch clears.
        let (d3, latch) = watchdog_tick(RecordingState::Idle, false, latch);
        assert!(!d3 && !latch);
        // Session 2: worker finishes -> dispatch again (recovery re-arms).
        let (d4, latch) = watchdog_tick(RecordingState::Recording, true, latch);
        assert!(d4 && latch);
    }
}
