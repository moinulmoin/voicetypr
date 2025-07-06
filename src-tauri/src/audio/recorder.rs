use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::thread;
use std::path::PathBuf;

pub struct AudioRecorder {
    recording_handle: Arc<Mutex<Option<RecordingHandle>>>,
}

struct RecordingHandle {
    stop_tx: mpsc::Sender<()>,
    thread_handle: thread::JoinHandle<Result<(), String>>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            recording_handle: Arc::new(Mutex::new(None)),
        }
    }

    pub fn start_recording(&mut self, output_path: &str) -> Result<(), String> {
        // Check if already recording
        if self.recording_handle.lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?
            .is_some() {
            return Err("Already recording".to_string());
        }

        let output_path = PathBuf::from(output_path);
        let (stop_tx, stop_rx) = mpsc::channel();

        // Spawn recording thread
        let thread_handle = thread::spawn(move || -> Result<(), String> {
            let host = cpal::default_host();
            let device = host.default_input_device()
                .ok_or("No input device available")?;

            let config = device.default_input_config()
                .map_err(|e| e.to_string())?;
            
            println!("Recording with config: sample_rate={}, channels={}", 
                     config.sample_rate().0, config.channels());

            // Record with native settings, Whisper will handle resampling
            let spec = hound::WavSpec {
                channels: config.channels(),
                sample_rate: config.sample_rate().0,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };

            let writer = Arc::new(Mutex::new(Some(
                hound::WavWriter::create(&output_path, spec)
                    .map_err(|e| e.to_string())?
            )));

            let writer_clone = writer.clone();
            let err_fn = |err| eprintln!("Stream error: {}", err);
            let error_occurred = Arc::new(Mutex::new(None::<String>));

            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    let error_clone = error_occurred.clone();
                    device.build_input_stream(
                        &config.config(),
                        move |data: &[f32], _: &_| {
                            if let Ok(mut guard) = writer_clone.try_lock() {
                                if let Some(writer) = guard.as_mut() {
                                    for &sample in data {
                                        let sample = (sample * i16::MAX as f32) as i16;
                                        if let Err(e) = writer.write_sample(sample) {
                                            if let Ok(mut error_guard) = error_clone.lock() {
                                                *error_guard = Some(format!("Failed to write audio sample: {}", e));
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        },
                        err_fn,
                        None
                    ).map_err(|e| e.to_string())?
                }
                cpal::SampleFormat::I16 => {
                    let writer_clone = writer.clone();
                    let error_clone = error_occurred.clone();
                    device.build_input_stream(
                        &config.config(),
                        move |data: &[i16], _: &_| {
                            if let Ok(mut guard) = writer_clone.try_lock() {
                                if let Some(writer) = guard.as_mut() {
                                    for &sample in data {
                                        if let Err(e) = writer.write_sample(sample) {
                                            if let Ok(mut error_guard) = error_clone.lock() {
                                                *error_guard = Some(format!("Failed to write audio sample: {}", e));
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        },
                        err_fn,
                        None
                    ).map_err(|e| e.to_string())?
                }
                cpal::SampleFormat::U16 => {
                    let writer_clone = writer.clone();
                    let error_clone = error_occurred.clone();
                    device.build_input_stream(
                        &config.config(),
                        move |data: &[u16], _: &_| {
                            if let Ok(mut guard) = writer_clone.try_lock() {
                                if let Some(writer) = guard.as_mut() {
                                    for &sample in data {
                                        // Convert U16 to I16 for WAV format
                                        let sample = (sample as i32 - 32768) as i16;
                                        if let Err(e) = writer.write_sample(sample) {
                                            if let Ok(mut error_guard) = error_clone.lock() {
                                                *error_guard = Some(format!("Failed to write audio sample: {}", e));
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        },
                        err_fn,
                        None
                    ).map_err(|e| e.to_string())?
                }
                _ => return Err(format!("Unsupported sample format: {:?}", config.sample_format())),
            };

            stream.play().map_err(|e| e.to_string())?;

            // Wait for stop signal
            stop_rx.recv().ok();

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

            Ok(())
        });

        *self.recording_handle.lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))? = Some(RecordingHandle {
            stop_tx,
            thread_handle,
        });

        Ok(())
    }

    pub fn stop_recording(&mut self) -> Result<String, String> {
        let handle = self.recording_handle.lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?
            .take();

        if let Some(handle) = handle {
            // Send stop signal
            handle.stop_tx.send(()).ok();

            // Wait for thread to finish
            match handle.thread_handle.join() {
                Ok(Ok(())) => Ok("Recording stopped".to_string()),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("Recording thread panicked".to_string()),
            }
        } else {
            Err("Not recording".to_string())
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording_handle.lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    pub fn get_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.input_devices()
            .map(|devices| {
                devices.filter_map(|device| device.name().ok()).collect()
            })
            .unwrap_or_else(|_| Vec::new())
    }
}