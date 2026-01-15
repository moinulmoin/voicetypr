use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::level_meter::AudioLevelMeter;
use super::silence_detector::SilenceDetector;

// Type-safe recording size limits
pub struct RecordingSize;

impl RecordingSize {
    const MAX_RECORDING_SIZE: u64 = 500 * 1024 * 1024; // 500MB max for recordings

    pub fn check(size: u64) -> Result<(), String> {
        if size > Self::MAX_RECORDING_SIZE {
            return Err(format!(
                "Recording size {} bytes ({:.1}MB) exceeds maximum of 500MB",
                size,
                size as f64 / 1024.0 / 1024.0
            ));
        }
        Ok(())
    }
}

pub struct AudioRecorder {
    recording_handle: Arc<Mutex<Option<RecordingHandle>>>,
    audio_level_receiver: Arc<Mutex<Option<mpsc::Receiver<f64>>>>,
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        // Ensure cleanup on drop
        if let Ok(mut handle_guard) = self.recording_handle.lock() {
            if let Some(handle) = handle_guard.take() {
                // Send stop signal
                if let Err(e) = handle.stop_tx.send(RecorderCommand::Stop) {
                    log::warn!("Failed to send stop signal during drop: {:?}", e);
                }
                // Don't wait for thread in Drop - let it clean up in background
            }
        } else {
            log::error!("Failed to acquire recording handle lock during drop");
        }

        // Clear audio level receiver
        if let Ok(mut receiver_guard) = self.audio_level_receiver.lock() {
            receiver_guard.take();
        } else {
            log::error!("Failed to acquire audio level receiver lock during drop");
        }
    }
}

struct RecordingHandle {
    stop_tx: mpsc::Sender<RecorderCommand>,
    thread_handle: thread::JoinHandle<Result<String, String>>,
}

#[derive(Debug)]
enum RecorderCommand {
    Stop,
    StopSilence,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            recording_handle: Arc::new(Mutex::new(None)),
            audio_level_receiver: Arc::new(Mutex::new(None)),
        }
    }

    pub fn start_recording(
        &mut self,
        output_path: &str,
        device_name: Option<String>,
    ) -> Result<(), String> {
        log::info!(
            "AudioRecorder::start_recording called with path: {}",
            output_path
        );

        // Acquire lock once and hold it through the entire initialization
        let mut handle_guard = self
            .recording_handle
            .lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?;

        // Check if already recording
        if handle_guard.is_some() {
            return Err("Already recording".to_string());
        }

        // Clear any leftover audio level receiver from previous recordings
        if let Ok(mut guard) = self.audio_level_receiver.lock() {
            guard.take();
        }

        let output_path = PathBuf::from(output_path);
        let (stop_tx, stop_rx) = mpsc::channel();
        let stop_tx_clone = stop_tx.clone();

        // Create audio level channel (f64 for EBU R128 loudness values)
        let (audio_level_tx, audio_level_rx) = mpsc::channel::<f64>();

        // Silence detection config for VAD
        let silence_duration = Duration::from_secs(10); // 10 seconds of continuous silence

        // Spawn recording thread
        let thread_handle = thread::spawn(move || -> Result<String, String> {
            let host = cpal::default_host();
            let device = if let Some(device_name) = device_name {
                // Try to find the specified device
                host.input_devices()
                    .map_err(|e| format!("Failed to enumerate input devices: {}", e))?
                    .find(|d| d.name().map(|n| n == device_name).unwrap_or(false))
                    .ok_or_else(|| {
                        log::warn!(
                            "Specified device '{}' not found, falling back to default",
                            device_name
                        );
                        format!("Device '{}' not found", device_name)
                    })
                    .or_else(|_| {
                        // Fallback to default device if specified device not found
                        host.default_input_device()
                            .ok_or("No input device available".to_string())
                    })?
            } else {
                // Use default device
                host.default_input_device()
                    .ok_or("No input device available")?
            };

            let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
            log::info!("======================================");
            log::info!("🎤 AUDIO DEVICE SELECTED: {}", device_name);
            log::info!("======================================");

            let config = device.default_input_config().map_err(|e| e.to_string())?;

            log::info!(
                "Audio config: sample_rate={} Hz, channels={}, format={:?}",
                config.sample_rate().0,
                config.channels(),
                config.sample_format()
            );

            // List all available input devices for debugging
            log::info!("Available input devices:");
            if let Ok(devices) = host.input_devices() {
                for (idx, dev) in devices.enumerate() {
                    if let Ok(name) = dev.name() {
                        log::info!("  {}. {}", idx + 1, name);
                    }
                }
            }

            // Initialize silence detector and level meter
            let silence_detector = Arc::new(Mutex::new(SilenceDetector::new(silence_duration)));

            let level_meter = Arc::new(Mutex::new(
                AudioLevelMeter::new(
                    config.sample_rate().0,
                    config.channels() as u32,
                    audio_level_tx.clone(),
                )
                .map_err(|e| format!("Failed to create level meter: {}", e))?,
            ));

            // Record with native settings, Whisper will handle resampling
            let spec = hound::WavSpec {
                channels: config.channels(),
                sample_rate: config.sample_rate().0,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };

            let writer = Arc::new(Mutex::new(Some(
                hound::WavWriter::create(&output_path, spec).map_err(|e| e.to_string())?,
            )));
            // Error callback that triggers stop on device errors (e.g., disconnection)
            let stop_tx_for_error = stop_tx_clone.clone();
            let error_occurred = Arc::new(Mutex::new(None::<String>));
            let error_occurred_for_callback = error_occurred.clone();
            let err_fn = move |err: cpal::StreamError| {
                // Detailed logging for audio device errors
                log::error!("═══════════════════════════════════════════════════════");
                log::error!("🔴 AUDIO DEVICE ERROR DETECTED");
                log::error!("═══════════════════════════════════════════════════════");
                log::error!("Error type: {:?}", err);
                log::error!("Error message: {}", err);
                log::error!("Action: Triggering graceful recording stop");
                log::error!("═══════════════════════════════════════════════════════");

                // Store the error
                if let Ok(mut guard) = error_occurred_for_callback.lock() {
                    *guard = Some(format!("Audio device error: {}", err));
                }
                // Signal the recording thread to stop
                let _ = stop_tx_for_error.send(RecorderCommand::Stop);
            };

            // Shared state for size tracking
            let bytes_written = Arc::new(Mutex::new(0u64));

            // Common audio processing closure
            let process_audio = {
                let writer_clone = writer.clone();
                let error_clone = error_occurred.clone();
                let bytes_clone = bytes_written.clone();
                let stop_tx_for_size = stop_tx_clone.clone();
                let stop_tx_for_silence = stop_tx_clone.clone();
                let silence_detector_clone = silence_detector.clone();
                let level_meter_clone = level_meter.clone();

                move |f32_samples: &[f32], i16_samples: &[i16]| {
                    // Calculate RMS for both level meter and silence detection
                    let sum: f32 = f32_samples.iter().map(|x| x * x).sum();
                    let rms = (sum / f32_samples.len() as f32).sqrt();

                    // Process with level meter
                    if let Ok(mut meter) = level_meter_clone.try_lock() {
                        let _ = meter.process_samples(f32_samples);
                    }

                    // Check for silence
                    if let Ok(mut detector) = silence_detector_clone.try_lock() {
                        if detector.update(rms) {
                            // Silence duration exceeded, stop recording
                            let _ = stop_tx_for_silence.send(RecorderCommand::StopSilence);
                        }
                    }

                    // Check size before writing
                    let sample_bytes = i16_samples.len() * 2; // 2 bytes per i16 sample
                    if let Ok(mut bytes_guard) = bytes_clone.lock() {
                        let new_total = *bytes_guard + sample_bytes as u64;
                        if RecordingSize::check(new_total).is_err() {
                            let _ = stop_tx_for_size.send(RecorderCommand::Stop);
                            return;
                        }
                        *bytes_guard = new_total;
                    }

                    // Write audio data (i16 format)
                    if let Ok(mut guard) = writer_clone.try_lock() {
                        if let Some(writer) = guard.as_mut() {
                            for &sample in i16_samples {
                                if let Err(e) = writer.write_sample(sample) {
                                    if let Ok(mut error_guard) = error_clone.lock() {
                                        *error_guard =
                                            Some(format!("Failed to write audio sample: {}", e));
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            };

            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    let process_clone = process_audio.clone();
                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[f32], _: &_| {
                                // Convert F32 to I16 with proper clamping to avoid distortion
                                let i16_samples: Vec<i16> = data
                                    .iter()
                                    .map(|&sample| {
                                        // Clamp to avoid overflow and use 32767.0 for symmetric conversion
                                        let clamped = sample.clamp(-1.0, 1.0);
                                        (clamped * 32767.0) as i16
                                    })
                                    .collect();

                                // Process audio
                                process_clone(data, &i16_samples);
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| e.to_string())?
                }
                cpal::SampleFormat::I16 => {
                    let process_clone = process_audio.clone();
                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[i16], _: &_| {
                                // Convert I16 to F32 for processing
                                let f32_samples: Vec<f32> =
                                    data.iter().map(|&x| x as f32 / i16::MAX as f32).collect();

                                // Process audio
                                process_clone(&f32_samples, data);
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| e.to_string())?
                }
                cpal::SampleFormat::U16 => {
                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[u16], _: &_| {
                                // Convert U16 to F32 for processing
                                let f32_samples: Vec<f32> = data
                                    .iter()
                                    .map(|&x| (x as f32 - 32768.0) / 32768.0)
                                    .collect();

                                // Convert U16 to I16 for writing
                                let i16_samples: Vec<i16> =
                                    data.iter().map(|&x| (x as i32 - 32768) as i16).collect();

                                // Process audio
                                process_audio(&f32_samples, &i16_samples);
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| e.to_string())?
                }
                _ => {
                    return Err(format!(
                        "Unsupported sample format: {:?}",
                        config.sample_format()
                    ))
                }
            };

            stream.play().map_err(|e| {
                log::error!("Failed to start audio stream: {}", e);
                e.to_string()
            })?;

            log::info!("Audio stream started successfully");

            // Wait for stop signal
            let stop_reason = stop_rx.recv().ok();

            // Stop and finalize - platform-specific handling for audio stream cleanup
            // On Windows, some audio devices (especially USB/wireless like Astro A50) can hang
            // indefinitely during stream cleanup. On macOS, cpal::Stream isn't Send-safe
            // so we can't use threaded timeout - but macOS CoreAudio is generally more reliable.
            #[cfg(target_os = "windows")]
            {
                // Windows: Use timeout to prevent hanging on problematic audio devices
                let stream_drop_timeout = Duration::from_secs(1);
                let stream_drop_handle = std::thread::spawn(move || {
                    drop(stream);
                });

                // Wait for stream drop with timeout
                let drop_start = std::time::Instant::now();
                while drop_start.elapsed() < stream_drop_timeout {
                    if stream_drop_handle.is_finished() {
                        let _ = stream_drop_handle.join();
                        log::info!("Audio stream dropped successfully");
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }

                if drop_start.elapsed() >= stream_drop_timeout {
                    // Stream drop timed out - abandon the thread and continue
                    // The OS will clean up the resources when the process exits
                    log::warn!("═══════════════════════════════════════════════════════");
                    log::warn!("⚠️ AUDIO STREAM DROP TIMEOUT");
                    log::warn!("═══════════════════════════════════════════════════════");
                    log::warn!("Timeout: {}ms", stream_drop_timeout.as_millis());
                    log::warn!("Elapsed: {}ms", drop_start.elapsed().as_millis());
                    log::warn!("Cause: Audio device driver not responding to cleanup");
                    log::warn!("Impact: Recording will continue, orphaned thread abandoned");
                    log::warn!("Resolution: Device may need to be unplugged/replugged");
                    log::warn!("═══════════════════════════════════════════════════════");
                }
            }

            #[cfg(not(target_os = "windows"))]
            {
                // macOS/Linux: Drop stream directly - CoreAudio/ALSA are generally reliable
                // and cpal::Stream isn't Send-safe on macOS anyway
                drop(stream);
                log::info!("Audio stream dropped successfully");
            }

            // Check if any errors occurred during recording
            if let Ok(guard) = error_occurred.lock() {
                if let Some(error) = &*guard {
                    return Err(error.clone());
                }
            }

            // Take the writer out of the mutex to finalize it
            if let Ok(mut guard) = writer.lock() {
                if let Some(w) = guard.take() {
                    w.finalize().map_err(|e| e.to_string())?;
                }
            }

            // Return appropriate message based on stop reason
            match stop_reason {
                Some(RecorderCommand::StopSilence) => {
                    Ok("Recording stopped due to silence".to_string())
                }
                Some(RecorderCommand::Stop) => Ok("Recording stopped by user".to_string()),
                None => Ok("Recording stopped".to_string()),
            }
        });

        // Set the handle using the guard we already have
        *handle_guard = Some(RecordingHandle {
            stop_tx,
            thread_handle,
        });

        // Store the audio level receiver
        *self
            .audio_level_receiver
            .lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))? = Some(audio_level_rx);

        Ok(())
    }

    pub fn stop_recording(&mut self) -> Result<String, String> {
        let handle = self
            .recording_handle
            .lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?
            .take();

        // Also clear the audio level receiver
        if let Ok(mut guard) = self.audio_level_receiver.lock() {
            guard.take();
        }

        if let Some(handle) = handle {
            // Send stop signal
            handle.stop_tx.send(RecorderCommand::Stop).ok();

            // Wait for thread to finish with timeout
            let thread_handle = handle.thread_handle;
            let timeout = Duration::from_secs(5); // Reasonable timeout for normal operation
            let start = std::time::Instant::now();

            // Try to join thread with timeout by checking if it's finished
            while start.elapsed() < timeout {
                if thread_handle.is_finished() {
                    match thread_handle.join() {
                        Ok(Ok(msg)) => return Ok(msg),
                        Ok(Err(e)) => return Err(e),
                        Err(_) => return Err("Recording thread panicked".to_string()),
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }

            // If we get here, the thread didn't finish within timeout
            Err("Recording thread failed to stop within timeout".to_string())
        } else {
            Err("Not recording".to_string())
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording_handle
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    pub fn take_audio_level_receiver(&mut self) -> Option<mpsc::Receiver<f64>> {
        self.audio_level_receiver
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    pub fn get_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.input_devices()
            .map(|devices| devices.filter_map(|device| device.name().ok()).collect())
            .unwrap_or_else(|_| Vec::new())
    }
}
