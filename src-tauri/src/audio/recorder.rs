use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// Type-safe recording size limits
pub struct RecordingSize;

impl RecordingSize {
    const MAX_RECORDING_SIZE: u64 = 500 * 1024 * 1024; // 500MB max for recordings

    // TODO: Implement warning when approaching size limit
    #[allow(dead_code)]
    const WARNING_SIZE: u64 = 400 * 1024 * 1024; // 400MB warning threshold

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

    // TODO: Use this to show warning in UI when approaching limit
    #[allow(dead_code)]
    pub fn is_warning_threshold(size: u64) -> bool {
        size > Self::WARNING_SIZE
    }
}

pub struct AudioRecorder {
    recording_handle: Arc<Mutex<Option<RecordingHandle>>>,
    audio_level_receiver: Arc<Mutex<Option<mpsc::Receiver<f32>>>>,
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

// TODO: Track stop reasons for better user feedback
#[allow(dead_code)]
#[derive(Debug)]
enum StopReason {
    User,      // User manually stopped
    Silence,   // Auto-stopped due to silence
    SizeLimit, // Stopped due to size limit
}

// Configuration for silence detection
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct SilenceConfig {
    threshold: f32,           // RMS threshold for silence (0.01 = 1% of max)
    duration: Duration,       // How long silence before auto-stop (60 seconds)
    check_interval: Duration, // How often to check for silence
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            recording_handle: Arc::new(Mutex::new(None)),
            audio_level_receiver: Arc::new(Mutex::new(None)),
        }
    }

    pub fn start_recording(&mut self, output_path: &str) -> Result<(), String> {
        // Check if already recording
        if self
            .recording_handle
            .lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?
            .is_some()
        {
            return Err("Already recording".to_string());
        }

        let output_path = PathBuf::from(output_path);
        let (stop_tx, stop_rx) = mpsc::channel();
        let stop_tx_clone = stop_tx.clone();
        
        // Create audio level channel
        let (audio_level_tx, audio_level_rx) = mpsc::channel();

        // Default silence detection config
        let silence_config = SilenceConfig {
            threshold: 0.01,                        // 1% of max amplitude
            duration: Duration::from_secs(60),      // 60 seconds of silence
            check_interval: Duration::from_secs(1), // Check every second
        };

        // Spawn recording thread
        let thread_handle = thread::spawn(move || -> Result<String, String> {
            let host = cpal::default_host();
            let device = host
                .default_input_device()
                .ok_or("No input device available")?;

            let config = device.default_input_config().map_err(|e| e.to_string())?;

            log::info!(
                "Recording with config: sample_rate={}, channels={}",
                config.sample_rate().0,
                config.channels()
            );

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

            let writer_clone = writer.clone();
            let err_fn = |err| log::error!("Stream error: {}", err);
            let error_occurred = Arc::new(Mutex::new(None::<String>));

            // Shared state for silence detection and size tracking
            let last_sound_time = Arc::new(Mutex::new(Instant::now()));
            let bytes_written = Arc::new(Mutex::new(0u64));

            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    let error_clone = error_occurred.clone();
                    let last_sound_clone = last_sound_time.clone();
                    let bytes_clone = bytes_written.clone();
                    let silence_threshold = silence_config.threshold;
                    let stop_tx_for_size = stop_tx_clone.clone();
                    let audio_level_tx_clone = audio_level_tx.clone();

                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[f32], _: &_| {
                                // Calculate RMS for silence detection
                                let rms = (data.iter().map(|&x| x * x).sum::<f32>()
                                    / data.len() as f32)
                                    .sqrt();
                                
                                // Send audio level (ignore errors if receiver is dropped)
                                let _ = audio_level_tx_clone.send(rms);

                                // Update last sound time if above threshold
                                if rms > silence_threshold {
                                    if let Ok(mut time_guard) = last_sound_clone.lock() {
                                        *time_guard = Instant::now();
                                    }
                                }

                                // Check size before writing
                                let sample_bytes = data.len() * 2; // 2 bytes per i16 sample
                                if let Ok(mut bytes_guard) = bytes_clone.lock() {
                                    let new_total = *bytes_guard + sample_bytes as u64;
                                    if RecordingSize::check(new_total).is_err() {
                                        let _ = stop_tx_for_size.send(RecorderCommand::Stop);
                                        return;
                                    }
                                    *bytes_guard = new_total;
                                }

                                // Write audio data
                                if let Ok(mut guard) = writer_clone.try_lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        for &sample in data {
                                            let sample = (sample * i16::MAX as f32) as i16;
                                            if let Err(e) = writer.write_sample(sample) {
                                                if let Ok(mut error_guard) = error_clone.lock() {
                                                    *error_guard = Some(format!(
                                                        "Failed to write audio sample: {}",
                                                        e
                                                    ));
                                                }
                                                break;
                                            }
                                        }
                                    }
                                }
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| e.to_string())?
                }
                cpal::SampleFormat::I16 => {
                    let writer_clone = writer.clone();
                    let error_clone = error_occurred.clone();
                    let last_sound_clone = last_sound_time.clone();
                    let silence_threshold = (silence_config.threshold * i16::MAX as f32) as i16;
                    let audio_level_tx_clone = audio_level_tx.clone();

                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[i16], _: &_| {
                                // Calculate RMS for I16 samples
                                let rms = ((data.iter().map(|&x| (x as i32).pow(2)).sum::<i32>()
                                    as f32
                                    / data.len() as f32)
                                    .sqrt()) as i16;
                                
                                // Send normalized audio level (0.0 to 1.0)
                                let normalized_rms = (rms.abs() as f32) / (i16::MAX as f32);
                                let _ = audio_level_tx_clone.send(normalized_rms);

                                // Update last sound time if above threshold
                                if rms.abs() > silence_threshold {
                                    if let Ok(mut time_guard) = last_sound_clone.lock() {
                                        *time_guard = Instant::now();
                                    }
                                }

                                // Write audio data
                                if let Ok(mut guard) = writer_clone.try_lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        for &sample in data {
                                            if let Err(e) = writer.write_sample(sample) {
                                                if let Ok(mut error_guard) = error_clone.lock() {
                                                    *error_guard = Some(format!(
                                                        "Failed to write audio sample: {}",
                                                        e
                                                    ));
                                                }
                                                break;
                                            }
                                        }
                                    }
                                }
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| e.to_string())?
                }
                cpal::SampleFormat::U16 => {
                    let writer_clone = writer.clone();
                    let error_clone = error_occurred.clone();
                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[u16], _: &_| {
                                if let Ok(mut guard) = writer_clone.try_lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        for &sample in data {
                                            // Convert U16 to I16 for WAV format
                                            let sample = (sample as i32 - 32768) as i16;
                                            if let Err(e) = writer.write_sample(sample) {
                                                if let Ok(mut error_guard) = error_clone.lock() {
                                                    *error_guard = Some(format!(
                                                        "Failed to write audio sample: {}",
                                                        e
                                                    ));
                                                }
                                                break;
                                            }
                                        }
                                    }
                                }
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

            stream.play().map_err(|e| e.to_string())?;

            // Spawn silence detection thread
            let last_sound_clone = last_sound_time.clone();
            let silence_duration = silence_config.duration;
            let check_interval = silence_config.check_interval;
            let silence_thread = thread::spawn(move || loop {
                thread::sleep(check_interval);

                if let Ok(last_sound) = last_sound_clone.lock() {
                    if last_sound.elapsed() > silence_duration {
                        log::info!(
                            "Silence detected for {:?}, auto-stopping recording",
                            silence_duration
                        );
                        let _ = stop_tx_clone.send(RecorderCommand::StopSilence);
                        break;
                    }
                }
            });

            // Wait for stop signal
            let stop_reason = stop_rx.recv().ok();

            // Stop silence detection thread
            drop(silence_thread);

            // Stop and finalize
            drop(stream);

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

        *self
            .recording_handle
            .lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))? = Some(RecordingHandle {
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

            // Wait for thread to finish
            match handle.thread_handle.join() {
                Ok(Ok(msg)) => Ok(msg),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("Recording thread panicked".to_string()),
            }
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

    pub fn take_audio_level_receiver(&mut self) -> Option<mpsc::Receiver<f32>> {
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
