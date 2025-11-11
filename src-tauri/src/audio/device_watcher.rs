use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::audio::recorder::AudioRecorder;
use crate::commands::settings::{get_settings, set_audio_device, update_tray_menu};
use crate::{get_recording_state, RecordingState};

/// Background watcher that monitors OS microphone devices and emits updates.
pub struct DeviceWatcher {
    stop: Arc<AtomicBool>,
    handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl DeviceWatcher {
    /// Start the watcher on a dedicated thread.
    pub fn start(app: AppHandle) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();
        let handle = thread::spawn(move || {
            let mut last_devices: Vec<String> = Vec::new();

            while !stop_flag.load(Ordering::Relaxed) {
                let devices = AudioRecorder::get_devices();

                if devices != last_devices {
                    log::info!("Audio devices changed: {:?}", devices);

                    if let Err(err) = app.emit("audio-devices-updated", &devices) {
                        log::warn!("Failed to emit audio-devices-updated: {}", err);
                    }

                    let app_for_tasks = app.clone();
                    let devices_for_tasks = devices.clone();

                    tauri::async_runtime::spawn(async move {
                        // Refresh tray regardless of selection outcome.
                        if let Err(err) = update_tray_menu(app_for_tasks.clone()).await {
                            log::warn!("Failed to update tray menu after device change: {}", err);
                        }

                        match get_settings(app_for_tasks.clone()).await {
                            Ok(settings) => {
                                if let Some(current) = settings.selected_microphone {
                                    if !devices_for_tasks.contains(&current) {
                                        let state = get_recording_state(&app_for_tasks);
                                        let is_recording = matches!(
                                            state,
                                            RecordingState::Starting
                                                | RecordingState::Recording
                                                | RecordingState::Stopping
                                                | RecordingState::Transcribing
                                        );

                                        if is_recording {
                                            log::info!(
                                                "Selected microphone '{}' removed but recording in progress; deferring auto-fallback",
                                                current
                                            );
                                        } else {
                                            log::info!(
                                                "Selected microphone '{}' removed; falling back to default",
                                                current
                                            );

                                            if let Err(err) =
                                                set_audio_device(app_for_tasks.clone(), None).await
                                            {
                                                log::warn!(
                                                    "Failed to reset audio device after removal: {}",
                                                    err
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            Err(err) => {
                                log::warn!("Failed to read settings after device change: {}", err);
                            }
                        }
                    });

                    last_devices = devices;
                }

                thread::sleep(Duration::from_millis(1500));
            }
        });

        Self {
            stop,
            handle: Mutex::new(Some(handle)),
        }
    }
}

impl Drop for DeviceWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);

        if let Ok(mut guard) = self.handle.lock() {
            if let Some(handle) = guard.take() {
                if let Err(err) = handle.join() {
                    log::debug!("DeviceWatcher thread join failed: {:?}", err);
                }
            }
        }
    }
}
