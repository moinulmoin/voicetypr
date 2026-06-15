use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::commands::audio::{stop_recording, RecorderState};
use crate::{get_recording_state, RecordingState};

/// Background watcher that detects when the audio recorder worker self-terminates
/// while app state remains `Recording`, and drives the normal stop/transcribe flow.
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
    /// Safe to call multiple times — will no-op if already running.
    pub fn start(&self) {
        if self.started.swap(true, Ordering::SeqCst) {
            log::debug!("RecorderWatchdog already running, skipping start");
            return;
        }

        log::info!("Starting RecorderWatchdog");

        let stop_flag = self.stop.clone();
        let app = self.app.clone();

        let handle = thread::spawn(move || {
            // One-shot per autonomous termination: dispatch the stop flow once, then
            // wait until state leaves Recording before re-arming. stop_recording is
            // idempotent (stop_in_flight) and resets to Idle even on a recorder error,
            // so a dispatched stop reliably moves state off Recording and re-arms us.
            let mut auto_stop_dispatched = false;

            while !stop_flag.load(Ordering::Relaxed) {
                let state = get_recording_state(&app);

                if state != RecordingState::Recording {
                    auto_stop_dispatched = false;
                } else if !auto_stop_dispatched {
                    let finished = app
                        .state::<RecorderState>()
                        .inner()
                        .0
                        .lock()
                        .map(|r| r.recording_thread_finished())
                        .unwrap_or(false);

                    if finished {
                        auto_stop_dispatched = true;
                        log::info!(
                            "RecorderWatchdog: recorder thread self-terminated while state=Recording; driving stop/transcribe flow"
                        );
                        let app2 = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let st = app2.state::<RecorderState>();
                            if let Err(e) = stop_recording(app2.clone(), st).await {
                                log::error!("RecorderWatchdog auto-stop failed: {}", e);
                            }
                        });
                    }
                }

                thread::sleep(Duration::from_millis(250));
            }
        });

        // `started` is set before this handle is stored, so a Drop racing an instant
        // start() may skip the join. Benign: the thread observes the stop flag and
        // exits within one poll interval; start() runs once at setup, Drop at shutdown.
        if let Ok(mut guard) = self.handle.lock() {
            *guard = Some(handle);
        }
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
