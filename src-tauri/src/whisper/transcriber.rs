use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use whisper_rs::{
    convert_integer_to_float_audio, convert_stereo_to_mono_audio, print_system_info, FullParams,
    SamplingStrategy, WhisperContext, WhisperContextParameters,
};

use crate::utils::logger::*;
#[cfg(debug_assertions)]
use crate::utils::system_monitor;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TranscriptionBackend {
    Cpu,
    Metal,
}

impl TranscriptionBackend {
    fn is_cpu(self) -> bool {
        matches!(self, Self::Cpu)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Metal => "Metal GPU",
        }
    }
}

pub struct Transcriber {
    context: WhisperContext,
    backend: TranscriptionBackend,
}

impl Transcriber {
    pub fn new(model_path: &Path) -> Result<Self, String> {
        let init_start = Instant::now();
        let model_path_str = model_path
            .to_str()
            .ok_or_else(|| format!("Model path contains invalid UTF-8: {:?}", model_path))?;

        log_start("TRANSCRIBER_INIT");
        log_with_context(
            log::Level::Debug,
            "Initializing transcriber",
            &[
                ("model_path", model_path_str),
                ("platform", std::env::consts::OS),
            ],
        );

        // Log model file info
        if let Ok(metadata) = std::fs::metadata(model_path) {
            let size_mb = metadata.len() / 1024 / 1024;
            log_file_operation(
                "MODEL_LOAD",
                model_path_str,
                true,
                Some(metadata.len()),
                None,
            );
            log::info!("🤖 Model file size: {}MB", size_mb);
        }

        log::info!("🤖 whisper.cpp system info: {}", print_system_info());

        // Configure GPU usage based on platform and features
        let mut ctx_params = WhisperContextParameters::default();

        // macOS: Try Metal first, fallback to CPU if it fails
        // Note: Metal GPU acceleration only works on Apple Silicon (aarch64), not Intel Macs
        #[cfg(target_os = "macos")]
        {
            let is_apple_silicon = std::env::consts::ARCH == "aarch64";

            if !is_apple_silicon {
                log_with_context(
                    log::Level::Info,
                    "🎮 METAL_SKIP",
                    &[
                        ("reason", "Intel Mac detected"),
                        ("arch", std::env::consts::ARCH),
                        ("fallback_to", "CPU"),
                    ],
                );
                log::info!("⚠️ Intel Mac detected - using CPU-only mode (Metal GPU not supported)");
                ctx_params.use_gpu(false);
            } else {
                ctx_params.use_gpu(true);
            }

            let metal_start = Instant::now();

            log_with_context(
                log::Level::Info,
                "🎮 METAL_INIT",
                &[
                    ("backend", if is_apple_silicon { "Metal" } else { "CPU" }),
                    ("platform", "macOS"),
                    ("arch", std::env::consts::ARCH),
                    (
                        "attempt",
                        if is_apple_silicon {
                            "gpu_first"
                        } else {
                            "cpu_only"
                        },
                    ),
                ],
            );

            match WhisperContext::new_with_params(model_path_str, ctx_params) {
                Ok(ctx) => {
                    let backend = if is_apple_silicon {
                        TranscriptionBackend::Metal
                    } else {
                        TranscriptionBackend::Cpu
                    };
                    let init_time = metal_start.elapsed().as_millis();
                    let acceleration = if is_apple_silicon {
                        "gpu_acceleration_enabled"
                    } else {
                        "cpu_only"
                    };

                    log_performance("METAL_INIT", init_time as u64, Some(acceleration));
                    log_with_context(
                        log::Level::Info,
                        if is_apple_silicon {
                            "🎮 METAL_SUCCESS"
                        } else {
                            "🎮 CPU_INIT"
                        },
                        &[
                            ("init_time_ms", init_time.to_string().as_str()),
                            (
                                "acceleration",
                                if is_apple_silicon {
                                    "enabled"
                                } else {
                                    "disabled"
                                },
                            ),
                        ],
                    );

                    log_complete("TRANSCRIBER_INIT", init_start.elapsed().as_millis() as u64);
                    log_with_context(
                        log::Level::Debug,
                        "Transcriber initialized",
                        &[("backend", backend.label()), ("model_path", model_path_str)],
                    );

                    return Ok(Self {
                        context: ctx,
                        backend,
                    });
                }
                Err(gpu_err) => {
                    log_with_context(
                        log::Level::Info,
                        "🎮 METAL_FALLBACK",
                        &[
                            ("error", gpu_err.to_string().as_str()),
                            ("fallback_to", "CPU"),
                            (
                                "attempt_time_ms",
                                metal_start.elapsed().as_millis().to_string().as_str(),
                            ),
                        ],
                    );

                    ctx_params = WhisperContextParameters::default();
                    ctx_params.use_gpu(false);
                    log::info!("🔄 Attempting CPU-only initialization...");
                }
            }
        }

        // Windows main app is CPU-only. Optional Vulkan acceleration runs in a
        // separate sidecar process so Vulkan loader failures cannot prevent the
        // Tauri app from starting or falling back to CPU transcription.
        #[cfg(target_os = "windows")]
        {
            log::info!("Windows main transcriber: CPU-only mode");
            log_with_context(
                log::Level::Info,
                "🎮 CPU_ONLY_BUILD",
                &[
                    ("reason", "vulkan isolated in optional sidecar"),
                    ("arch", std::env::consts::ARCH),
                    ("fallback_to", "CPU"),
                ],
            );
            ctx_params.use_gpu(false);
        }

        // Create context (for Windows CPU fallback or other platforms)
        let cpu_start = Instant::now();
        let ctx = WhisperContext::new_with_params(model_path_str, ctx_params).map_err(|e| {
            log_failed("TRANSCRIBER_INIT", &e.to_string());
            log_with_context(
                log::Level::Debug,
                "CPU fallback failed",
                &[("model_path", model_path_str), ("backend", "CPU_FALLBACK")],
            );
            format!("Failed to load model: {}", e)
        })?;

        let backend = TranscriptionBackend::Cpu;
        let backend_type = backend.label();

        let cpu_time = cpu_start.elapsed().as_millis();

        log_with_context(
            log::Level::Info,
            "🎮 WHISPER_BACKEND",
            &[
                ("backend", backend_type),
                ("gpu_used", "false"),
                ("init_time_ms", cpu_time.to_string().as_str()),
            ],
        );

        log_complete("TRANSCRIBER_INIT", init_start.elapsed().as_millis() as u64);
        log_with_context(
            log::Level::Debug,
            "Transcriber initialization complete",
            &[
                ("backend", backend_type),
                ("model_path", model_path_str),
                ("gpu_acceleration", "false"),
            ],
        );

        log::info!(
            "✅ Whisper initialized successfully using {} backend",
            backend_type
        );

        // Log model capabilities and validation
        log_with_context(
            log::Level::Debug,
            "Model initialization validation",
            &[
                ("model_loaded", "true"),
                ("backend", backend_type),
                ("supports_multilingual", "true"), // Whisper models are multilingual
                (
                    "model_size_mb",
                    (std::fs::metadata(model_path)
                        .map(|m| m.len() / 1024 / 1024)
                        .unwrap_or(0)
                        .to_string())
                    .as_str(),
                ),
            ],
        );

        Ok(Self {
            context: ctx,
            backend,
        })
    }

    pub fn transcribe_with_cancellation(
        &self,
        audio_path: &Path,
        language: Option<&str>,
        translate: bool,
        cancel_flag: Arc<AtomicBool>,
    ) -> Result<String, String> {
        let transcription_start = Instant::now();
        let audio_path_str = format!("{:?}", audio_path);

        // Monitor system resources before transcription (only in debug builds)
        #[cfg(debug_assertions)]
        system_monitor::log_resources_before_operation("TRANSCRIPTION");

        log_start("TRANSCRIPTION");
        log_with_context(
            log::Level::Debug,
            "Starting transcription",
            &[
                ("audio_path", &audio_path_str),
                ("language", language.unwrap_or("auto")),
                ("translate", translate.to_string().as_str()),
                ("timestamp", chrono::Utc::now().to_rfc3339().as_str()),
            ],
        );

        // Check if file exists and is readable
        if !audio_path.exists() {
            let error = format!("Audio file does not exist: {:?}", audio_path);
            log_failed("TRANSCRIPTION", &error);
            log_with_context(
                log::Level::Debug,
                "File validation failed",
                &[
                    ("stage", "file_validation"),
                    ("audio_path", &audio_path_str),
                ],
            );
            return Err(error);
        }

        if is_cancelled(&cancel_flag) {
            log::info!("[TRANSCRIPTION_DEBUG] Transcription cancelled before starting");
            return Err("Transcription cancelled".to_string());
        }

        let file_size = std::fs::metadata(audio_path)
            .map_err(|e| {
                let error = format!("Cannot read file metadata: {}", e);
                log_failed("TRANSCRIPTION", &error);
                log_with_context(
                    log::Level::Debug,
                    "Metadata read failed",
                    &[("stage", "metadata_read"), ("audio_path", &audio_path_str)],
                );
                error
            })?
            .len();

        log_file_operation(
            "TRANSCRIPTION_INPUT",
            &audio_path_str,
            true,
            Some(file_size),
            None,
        );

        if file_size == 0 {
            let error = "Audio file is empty (0 bytes)";
            log_failed("TRANSCRIPTION", error);
            log_with_context(
                log::Level::Debug,
                "File size check failed",
                &[("stage", "file_size_check"), ("file_size", "0")],
            );
            return Err(error.to_string());
        }

        // Read WAV file
        let audio_read_start = Instant::now();
        let mut reader = hound::WavReader::open(audio_path).map_err(|e| {
            let error = format!("Failed to open WAV file: {}", e);
            log::error!("[TRANSCRIPTION_DEBUG] {}", error);

            error
        })?;

        let spec = reader.spec();
        log::info!(
            "[TRANSCRIPTION_DEBUG] WAV spec: channels={}, sample_rate={}, bits={}",
            spec.channels,
            spec.sample_rate,
            spec.bits_per_sample
        );

        /* ----------------------------------------------
        1) read raw i16 pcm
        ---------------------------------------------- */
        let samples_i16: Vec<i16> = reader
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read audio samples: {}", e))?;

        if is_cancelled(&cancel_flag) {
            log::info!("[TRANSCRIPTION_DEBUG] Transcription cancelled after reading samples");
            return Err("Transcription cancelled".to_string());
        }

        /* ----------------------------------------------
        2) i16 → f32  (range -1.0 … 1.0)
        ---------------------------------------------- */
        let mut audio: Vec<f32> = vec![0.0; samples_i16.len()];
        convert_integer_to_float_audio(&samples_i16, &mut audio).map_err(|e| e.to_string())?;

        if is_cancelled(&cancel_flag) {
            log::info!("[TRANSCRIPTION_DEBUG] Transcription cancelled after audio conversion");
            return Err("Transcription cancelled".to_string());
        }

        /* ----------------------------------------------
        3) multi-channel → mono  (Whisper needs mono)
        ---------------------------------------------- */
        if spec.channels == 2 {
            let mut mono_audio = vec![0.0; audio.len() / 2];
            convert_stereo_to_mono_audio(&audio, &mut mono_audio).map_err(|e| e.to_string())?;
            audio = mono_audio;
        } else if spec.channels > 2 {
            // Handle multi-channel audio (3, 4, 5.1, 7.1, etc.)
            log::info!(
                "[TRANSCRIPTION_DEBUG] Converting {}-channel audio to mono",
                spec.channels
            );
            audio = convert_multichannel_to_mono(&audio, spec.channels as usize)?;
        } else if spec.channels != 1 {
            return Err(format!("Invalid channel count: {}", spec.channels));
        }

        // Store original audio length before the move
        let _original_audio_length = audio.len();

        /* ----------------------------------------------
        4) Resample to 16kHz using high-quality resampler
        ---------------------------------------------- */
        // Use rubato for high-quality resampling to 16kHz
        let resampled_audio = if spec.sample_rate != 16_000 {
            use crate::audio::resampler::resample_to_16khz;

            log::info!(
                "[TRANSCRIPTION_DEBUG] Resampling audio from {} Hz to 16000 Hz",
                spec.sample_rate
            );

            resample_to_16khz(&audio, spec.sample_rate)?
        } else {
            log::info!("[TRANSCRIPTION_DEBUG] Audio already at 16kHz, no resampling needed");
            audio
        };

        // Log audio preprocessing performance
        let preprocessing_time = audio_read_start.elapsed().as_millis() as u64;
        log_performance(
            "AUDIO_PREPROCESSING",
            preprocessing_time,
            Some(&format!("samples={}", resampled_audio.len())),
        );
        log_with_context(
            log::Level::Debug,
            "Audio preprocessing complete",
            &[
                (
                    "preprocessing_time_ms",
                    preprocessing_time.to_string().as_str(),
                ),
                ("sample_rate", "16000"),
                ("channels", "1"),
                ("samples", resampled_audio.len().to_string().as_str()),
            ],
        );

        if is_cancelled(&cancel_flag) {
            log::info!("[TRANSCRIPTION_DEBUG] Transcription cancelled after resampling");
            return Err("Transcription cancelled".to_string());
        }

        log::debug!(
            "Audio ready for Whisper: {} samples at 16kHz ({:.2}s)",
            resampled_audio.len(),
            resampled_audio.len() as f32 / 16_000_f32
        );

        let cpu_profile = self.backend.is_cpu();
        let mut params = if cpu_profile {
            log::info!("[PERFORMANCE] Using CPU fast transcription profile");
            FullParams::new(SamplingStrategy::Greedy { best_of: 1 })
        } else {
            FullParams::new(SamplingStrategy::BeamSearch {
                beam_size: 5,
                patience: -1.0,
            })
        };

        // Set language - use centralized validation
        log::info!("[LANGUAGE] Received language: {:?}", language);

        let final_lang = if let Some(lang) = language {
            if lang == "auto" {
                // Auto-detect removed due to 30-second requirement, default to English
                log::info!("[LANGUAGE] Auto-detection no longer supported, defaulting to English");
                Some("en")
            } else {
                let validated = super::languages::validate_language(Some(lang));
                log::info!("[LANGUAGE] Using language: {}", validated);
                Some(validated)
            }
        } else {
            log::info!("[LANGUAGE] No language specified, using English");
            Some("en")
        };

        if let Some(lang) = final_lang {
            log::info!("[LANGUAGE] Final language set to: {}", lang);
            params.set_language(Some(lang));
        }

        // Set translate mode
        if translate {
            log::info!("[LANGUAGE] Translation mode enabled - will translate to English");
            params.set_translate(true);
        } else {
            log::info!("[LANGUAGE] Transcription mode - will transcribe in original language");
            params.set_translate(false);
        }

        // Use most cores but leave one free to keep UI responsive
        let hw = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);

        // ARM big.LITTLE optimization: On ARM64 Windows (Qualcomm Snapdragon),
        // using all cores is slower because efficiency cores drag down performance.
        // Limit to ~4 threads (performance cores only) for better speed.
        #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
        let threads = {
            let t = std::cmp::min(4, std::cmp::max(1, hw.saturating_sub(1))) as i32;
            log::info!(
                "[PERFORMANCE] ARM64: Using {} threads (limiting to perf cores, {} total available)",
                t,
                hw
            );
            t
        };

        #[cfg(not(all(target_os = "windows", target_arch = "aarch64")))]
        let threads = {
            let t = std::cmp::max(1, hw.saturating_sub(1)) as i32;
            log::info!("[PERFORMANCE] Using {} threads for transcription", t);
            t
        };

        params.set_n_threads(threads);

        params.set_no_context(cpu_profile);
        params.set_no_timestamps(cpu_profile);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Suppress blank outputs to avoid empty transcriptions
        params.set_suppress_blank(true);

        // Don't suppress non-speech tokens - they help with timing and context
        params.set_suppress_nst(true);

        // Adjust speech detection threshold
        params.set_no_speech_thold(0.6); // Default value, as higher values can cause issues

        // Quality thresholds - use more lenient values to avoid rejecting valid speech
        // Default entropy threshold is 2.4, we'll keep it default to avoid over-filtering
        params.set_entropy_thold(2.4); // Default value - prevents filtering out valid but uncertain speech

        // Use default log probability threshold to avoid being too strict
        params.set_logprob_thold(-1.0); // Default value - balanced probability requirements

        // Set initial prompt to help with context
        params.set_initial_prompt(""); // Empty prompt to avoid biasing the model

        params.set_temperature(if cpu_profile { 0.0 } else { 0.2 });
        params.set_temperature_inc(0.2); // Increase by 0.2 on fallback (default)
        params.set_max_initial_ts(1.0); // Limit initial timestamp search

        // Limit segment length to prevent runaway hallucinations
        params.set_max_len(0); // 0 means no limit
        params.set_length_penalty(-1.0); // Default penalty

        let samples_count = resampled_audio.len();
        let duration_seconds = samples_count as f32 / 16_000_f32;

        // Check minimum duration (0.5 seconds)
        if duration_seconds < 0.5 {
            let error = "Recording too short".to_string();
            log::warn!("[TRANSCRIPTION_DEBUG] {}", error);
            return Err(error);
        }

        let audio_metrics = calculate_audio_metrics(&resampled_audio);
        log_audio_metrics(
            "WHISPER_INPUT",
            audio_metrics.rms,
            audio_metrics.peak,
            duration_seconds,
            None,
        );

        if is_likely_silence(audio_metrics) {
            log::info!(
                "[TRANSCRIPTION_DEBUG] Skipping Whisper inference because audio is below speech threshold: rms={:.6}, peak={:.6}",
                audio_metrics.rms,
                audio_metrics.peak
            );
            return Ok(String::new());
        }

        let abort_flag = cancel_flag.clone();
        let abort_callback: Box<dyn FnMut() -> bool> = Box::new(move || is_cancelled(&abort_flag));
        params
            .set_abort_callback_safe::<Option<Box<dyn FnMut() -> bool>>, Box<dyn FnMut() -> bool>>(
                Some(abort_callback),
            );
        let progress_callback: Box<dyn FnMut(i32)> = Box::new(|progress| {
            log::debug!("[TRANSCRIPTION_DEBUG] Whisper progress: {}%", progress);
        });
        params.set_progress_callback_safe::<Option<Box<dyn FnMut(i32)>>, Box<dyn FnMut(i32)>>(
            Some(progress_callback),
        );

        // Run transcription
        log::info!("[TRANSCRIPTION_DEBUG] Creating Whisper state...");
        let mut state = self.context.create_state().map_err(|e| {
            let error = format!("Failed to create Whisper state: {}", e);
            log::error!("[TRANSCRIPTION_DEBUG] {}", error);

            error
        })?;

        let inference_start = Instant::now();
        log_start("WHISPER_INFERENCE");
        log_with_context(
            log::Level::Debug,
            "Starting Whisper inference",
            &[
                ("samples", samples_count.to_string().as_str()),
                (
                    "duration_seconds",
                    format!("{:.2}", duration_seconds).as_str(),
                ),
                ("language", language.unwrap_or("auto")),
                ("translate", translate.to_string().as_str()),
            ],
        );

        match state.full(params, &resampled_audio) {
            Ok(_) => {
                let inference_time = inference_start.elapsed();
                let inference_ms = inference_time.as_millis();

                log_performance(
                    "WHISPER_INFERENCE",
                    inference_ms as u64,
                    Some(&format!(
                        "audio_duration={:.2}s, samples={}",
                        duration_seconds, samples_count
                    )),
                );

                log::info!(
                    "✅ Whisper inference completed in {:.2}s",
                    inference_time.as_secs_f32()
                );
            }
            Err(e) => {
                if is_cancelled(&cancel_flag) {
                    log::info!("[TRANSCRIPTION_DEBUG] Whisper inference aborted by cancellation");
                    return Err("Transcription cancelled".to_string());
                }

                let error = format!("Whisper inference failed: {}", e);
                log_failed("WHISPER_INFERENCE", &error);
                log_with_context(
                    log::Level::Debug,
                    "Inference failed",
                    &[
                        ("samples", samples_count.to_string().as_str()),
                        (
                            "duration_seconds",
                            format!("{:.2}", duration_seconds).as_str(),
                        ),
                        (
                            "inference_time_ms",
                            inference_start.elapsed().as_millis().to_string().as_str(),
                        ),
                    ],
                );
                return Err(error);
            }
        }

        // Get text
        let text_extraction_start = Instant::now();
        log::info!("[TRANSCRIPTION_DEBUG] Getting segments from Whisper output...");
        let num_segments = state.full_n_segments();

        log::info!(
            "[TRANSCRIPTION_DEBUG] Transcription complete: {} segments",
            num_segments
        );

        let mut text = String::new();
        for (i, segment) in state.as_iter().enumerate() {
            let segment_text = segment.to_string();
            log::info!("[TRANSCRIPTION_DEBUG] Segment {}: '{}'", i, segment_text);
            text.push_str(&segment_text);
            text.push(' ');
        }

        let result = text.trim().to_string();

        // Log text extraction performance
        let extraction_time = text_extraction_start.elapsed().as_millis() as u64;
        log_performance(
            "TEXT_EXTRACTION",
            extraction_time,
            Some(&format!(
                "segments={}, chars={}",
                num_segments,
                result.len()
            )),
        );

        let total_time = transcription_start.elapsed();

        // Log system resources after transcription (only in debug builds)
        #[cfg(debug_assertions)]
        system_monitor::log_resources_after_operation(
            "TRANSCRIPTION",
            total_time.as_millis() as u64,
        );

        if result.is_empty() {
            log_failed("TRANSCRIPTION", "Empty transcription result");
            log_with_context(
                log::Level::Debug,
                "Empty result",
                &[
                    ("segments", num_segments.to_string().as_str()),
                    ("total_time_ms", total_time.as_millis().to_string().as_str()),
                ],
            );
        } else if result == "[SOUND]" {
            log_failed("TRANSCRIPTION", "No speech detected");
            log_with_context(
                log::Level::Debug,
                "No speech",
                &[
                    ("result", "[SOUND]"),
                    ("segments", num_segments.to_string().as_str()),
                    ("total_time_ms", total_time.as_millis().to_string().as_str()),
                ],
            );
        } else {
            log_complete("TRANSCRIPTION", total_time.as_millis() as u64);
            log_with_context(
                log::Level::Debug,
                "Transcription complete",
                &[
                    ("result_length", result.len().to_string().as_str()),
                    ("segments", num_segments.to_string().as_str()),
                    (
                        "audio_duration_seconds",
                        format!("{:.2}", duration_seconds).as_str(),
                    ),
                ],
            );

            log::info!(
                "📝 Transcription success: {} chars in {:.2}s",
                result.len(),
                total_time.as_secs_f32()
            );
        }

        Ok(result)
    }
}

#[derive(Clone, Copy, Debug)]
struct AudioMetrics {
    rms: f64,
    peak: f64,
}

const SILENCE_RMS_THRESHOLD: f64 = 0.0005;
const SILENCE_PEAK_THRESHOLD: f64 = 0.003;

fn is_cancelled(cancel_flag: &AtomicBool) -> bool {
    cancel_flag.load(Ordering::SeqCst)
}

fn calculate_audio_metrics(samples: &[f32]) -> AudioMetrics {
    if samples.is_empty() {
        return AudioMetrics {
            rms: 0.0,
            peak: 0.0,
        };
    }

    let mut sum_squares = 0.0f64;
    let mut peak = 0.0f64;

    for &sample in samples {
        let value = f64::from(sample);
        let abs = value.abs();
        sum_squares += value * value;
        if abs > peak {
            peak = abs;
        }
    }

    AudioMetrics {
        rms: (sum_squares / samples.len() as f64).sqrt(),
        peak,
    }
}

fn is_likely_silence(metrics: AudioMetrics) -> bool {
    metrics.rms < SILENCE_RMS_THRESHOLD && metrics.peak < SILENCE_PEAK_THRESHOLD
}

/// Convert multi-channel audio to mono by averaging all channels
/// # Arguments
/// * `audio` - Interleaved audio samples (ch1, ch2, ch3, ch4, ch1, ch2, ...)
/// * `channels` - Number of channels in the audio
///
/// # Returns
/// Mono audio with averaged samples from all channels
fn convert_multichannel_to_mono(audio: &[f32], channels: usize) -> Result<Vec<f32>, String> {
    if channels == 0 {
        return Err("Channel count cannot be zero".to_string());
    }

    if channels == 1 {
        // Already mono, just return a copy
        return Ok(audio.to_vec());
    }

    let samples_per_channel = audio.len() / channels;
    let mut mono_audio = Vec::with_capacity(samples_per_channel);

    // Process each frame (set of samples across all channels)
    for i in 0..samples_per_channel {
        let mut sum = 0.0f32;

        // Sum all channels for this sample position
        for ch in 0..channels {
            let idx = i * channels + ch;
            if idx < audio.len() {
                sum += audio[idx];
            }
        }

        // Average the channels
        mono_audio.push(sum / channels as f32);
    }

    log::info!(
        "[AUDIO] Downmixed {}-channel audio to mono: {} samples -> {} samples",
        channels,
        audio.len(),
        mono_audio.len()
    );

    Ok(mono_audio)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_multichannel_to_mono() {
        // Test 4-channel audio downmixing
        // Simulating interleaved 4-channel audio: [ch1, ch2, ch3, ch4, ch1, ch2, ...]
        let four_channel_audio = vec![
            1.0, 2.0, 3.0, 4.0, // Frame 1: channels 1-4
            5.0, 6.0, 7.0, 8.0, // Frame 2: channels 1-4
            -1.0, -2.0, -3.0, -4.0, // Frame 3: channels 1-4
        ];

        let result = convert_multichannel_to_mono(&four_channel_audio, 4).unwrap();

        // Expected: average of each frame's channels
        // Frame 1: (1+2+3+4)/4 = 2.5
        // Frame 2: (5+6+7+8)/4 = 6.5
        // Frame 3: (-1-2-3-4)/4 = -2.5
        assert_eq!(result.len(), 3);
        assert!((result[0] - 2.5).abs() < 0.001);
        assert!((result[1] - 6.5).abs() < 0.001);
        assert!((result[2] - (-2.5)).abs() < 0.001);
    }

    #[test]
    fn test_convert_stereo_passthrough() {
        // Test that mono audio passes through unchanged
        let mono_audio = vec![1.0, 2.0, 3.0, 4.0];
        let result = convert_multichannel_to_mono(&mono_audio, 1).unwrap();
        assert_eq!(result, mono_audio);
    }

    #[test]
    fn test_audio_metrics_for_silence() {
        let samples = vec![0.0f32; 16_000];
        let metrics = calculate_audio_metrics(&samples);

        assert_eq!(metrics.rms, 0.0);
        assert_eq!(metrics.peak, 0.0);
        assert!(is_likely_silence(metrics));
    }

    #[test]
    fn test_audio_metrics_detect_speech_like_signal() {
        let samples = vec![0.02f32; 16_000];
        let metrics = calculate_audio_metrics(&samples);

        assert!(metrics.rms > SILENCE_RMS_THRESHOLD);
        assert!(metrics.peak > SILENCE_PEAK_THRESHOLD);
        assert!(!is_likely_silence(metrics));
    }

    #[test]
    fn test_convert_invalid_channels() {
        // Test that zero channels returns an error
        let audio = vec![1.0, 2.0];
        let result = convert_multichannel_to_mono(&audio, 0);
        assert!(result.is_err());
    }
}
