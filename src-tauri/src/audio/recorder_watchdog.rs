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

                if state != RecordingState::Recording {
                    auto_stop_dispatched = false;
                } else if !auto_stop_dispatched {
                    let finished = app
                        .state::<RecorderState>()
                        .inner()
                        .0
                        .lock()
                        .map(|recorder| recorder.recording_thread_finished())
                        .unwrap_or(false);

                    if finished {
                        auto_stop_dispatched = true;
                        log::info!(
                            "RecorderWatchdog: recorder thread finished while state=Recording; driving stop flow"
                        );

                        let app_for_stop = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let recorder_state = app_for_stop.state::<RecorderState>();
                            if let Err(err) =
                                stop_recording(app_for_stop.clone(), recorder_state).await
                            {
                                log::error!("RecorderWatchdog auto-stop failed: {}", err);
                            }
                        });
                    }
                }

                thread::sleep(Duration::from_millis(250));
            }
        });

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
