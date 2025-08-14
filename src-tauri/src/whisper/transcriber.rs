use std::path::Path;
use std::time::Instant;
use whisper_rs::{
    convert_integer_to_float_audio, convert_stereo_to_mono_audio, FullParams, SamplingStrategy,
    WhisperContext, WhisperContextParameters,
};

use crate::utils::logger::*;
#[cfg(debug_assertions)]
use crate::utils::system_monitor;

pub struct Transcriber {
    context: WhisperContext,
}

impl Transcriber {
    pub fn new(model_path: &Path) -> Result<Self, String> {
        let init_start = Instant::now();
        let model_path_str = model_path
            .to_str()
            .ok_or_else(|| format!("Model path contains invalid UTF-8: {:?}", model_path))?;

        log_start("TRANSCRIBER_INIT");
        log_with_context(log::Level::Debug, "Initializing transcriber", &[
            ("model_path", model_path_str),
            ("platform", std::env::consts::OS)
        ]);

        // Log model file info
        if let Ok(metadata) = std::fs::metadata(model_path) {
            let size_mb = metadata.len() / 1024 / 1024;
            log_file_operation("MODEL_LOAD", model_path_str, true, Some(metadata.len()), None);
            log::info!("🤖 Model file size: {}MB", size_mb);
        }

        // Configure GPU usage based on platform and features
        let mut ctx_params = WhisperContextParameters::default();
        #[allow(unused_assignments)] // gpu_used is assigned in multiple conditional blocks
        let mut gpu_used = false;
        
        // macOS: Try Metal first, fallback to CPU if it fails
        #[cfg(target_os = "macos")]
        {
            ctx_params.use_gpu(true);
            let metal_start = Instant::now();
            
            log_with_context(log::Level::Info, "🎮 METAL_INIT", &[
                ("backend", "Metal"),
                ("platform", "macOS"),
                ("attempt", "gpu_first")
            ]);
            
            match WhisperContext::new_with_params(model_path_str, ctx_params) {
                Ok(ctx) => {
                    #[allow(unused_assignments)] // This assignment is used later but flagged due to multiple conditional paths
                    {
                        gpu_used = true; // Metal GPU acceleration succeeded
                    }
                    let init_time = metal_start.elapsed().as_millis();
                    
                    log_performance("METAL_INIT", init_time as u64, Some("gpu_acceleration_enabled"));
                    log_with_context(log::Level::Info, "🎮 METAL_SUCCESS", &[
                        ("init_time_ms", &init_time.to_string().as_str()),
                        ("acceleration", "enabled")
                    ]);
                    
                    log_complete("TRANSCRIBER_INIT", init_start.elapsed().as_millis() as u64);
                    log_with_context(log::Level::Debug, "Transcriber initialized", &[
                        ("backend", "Metal"),
                        ("model_path", model_path_str)
                    ]);
                    
                    return Ok(Self { context: ctx });
                }
                Err(gpu_err) => {
                    log_with_context(log::Level::Info, "🎮 METAL_FALLBACK", &[
                        ("error", &gpu_err.to_string().as_str()),
                        ("fallback_to", "CPU"),
                        ("attempt_time_ms", &metal_start.elapsed().as_millis().to_string().as_str())
                    ]);
                    
                    ctx_params = WhisperContextParameters::default();
                    ctx_params.use_gpu(false);
                    log::info!("🔄 Attempting CPU-only initialization...");
                }
            }
        }
        
        // Windows: Try Vulkan GPU first, fallback to CPU if it fails (just like macOS!)
        #[cfg(target_os = "windows")]
        {
            ctx_params.use_gpu(true);
            let vulkan_start = Instant::now();
            
            // Check if Vulkan runtime is available
            let vulkan_available = std::path::Path::new("C:\\Windows\\System32\\vulkan-1.dll").exists();
            
            log_with_context(log::Level::Info, "🎮 VULKAN_INIT", &[
                ("backend", "Vulkan"),
                ("platform", "Windows"),
                ("vulkan_dll_available", &vulkan_available.to_string().as_str()),
                ("attempt", "gpu_first")
            ]);
            
            if !vulkan_available {
                log::warn!("⚠️  Vulkan runtime not found. GPU acceleration unavailable.");
            }
            
            match WhisperContext::new_with_params(model_path_str, ctx_params) {
                Ok(ctx) => {
                    gpu_used = true;
                    let init_time = vulkan_start.elapsed().as_millis();
                    
                    log_performance("VULKAN_INIT", init_time as u64, Some("gpu_acceleration_enabled"));
                    log_with_context(log::Level::Info, "🎮 VULKAN_SUCCESS", &[
                        ("init_time_ms", &init_time.to_string().as_str()),
                        ("acceleration", "enabled")
                    ]);
                    
                    log_complete("TRANSCRIBER_INIT", init_start.elapsed().as_millis() as u64);
                    log_with_context(log::Level::Debug, "Transcriber initialized", &[
                        ("backend", "Vulkan"),
                        ("model_path", model_path_str)
                    ]);
                    
                    return Ok(Self { context: ctx });
                }
                Err(gpu_err) => {
                    log_with_context(log::Level::Info, "🎮 VULKAN_FALLBACK", &[
                        ("error", &gpu_err.to_string().as_str()),
                        ("fallback_to", "CPU"),
                        ("attempt_time_ms", &vulkan_start.elapsed().as_millis().to_string().as_str())
                    ]);
                    
                    ctx_params = WhisperContextParameters::default();
                    ctx_params.use_gpu(false);
                    gpu_used = false;
                    log::info!("🔄 Attempting CPU-only initialization...");
                }
            }
        }

        // Create context (for Windows CPU fallback or other platforms)
        let cpu_start = Instant::now();
        let ctx = WhisperContext::new_with_params(model_path_str, ctx_params)
            .map_err(|e| {
                log_failed("TRANSCRIBER_INIT", &e.to_string());
                log_with_context(log::Level::Debug, "CPU fallback failed", &[
                    ("model_path", model_path_str),
                    ("backend", "CPU_FALLBACK")
                ]);
                format!("Failed to load model: {}", e)
            })?;

        // Determine backend type for logging
        let backend_type = if gpu_used {
            if cfg!(target_os = "windows") {
                "Vulkan GPU"
            } else if cfg!(target_os = "macos") {
                "Metal GPU"
            } else {
                "GPU"
            }
        } else {
            "CPU"
        };
        
        let cpu_time = cpu_start.elapsed().as_millis();
        
        log_with_context(log::Level::Info, "🎮 WHISPER_BACKEND", &[
            ("backend", backend_type),
            ("gpu_used", &gpu_used.to_string().as_str()),
            ("init_time_ms", &cpu_time.to_string().as_str())
        ]);
        
        log_complete("TRANSCRIBER_INIT", init_start.elapsed().as_millis() as u64);
        log_with_context(log::Level::Debug, "Transcriber initialization complete", &[
            ("backend", backend_type),
            ("model_path", model_path_str),
            ("gpu_acceleration", &gpu_used.to_string().as_str())
        ]);
        
        log::info!("✅ Whisper initialized successfully using {} backend", backend_type);
        
        // Log model capabilities and validation
        log_with_context(log::Level::Debug, "Model initialization validation", &[
            ("model_loaded", "true"),
            ("backend", backend_type),
            ("supports_multilingual", "true"), // Whisper models are multilingual
            ("model_size_mb", &(std::fs::metadata(model_path).map(|m| m.len() / 1024 / 1024).unwrap_or(0).to_string()).as_str())
        ]);

        Ok(Self { context: ctx })
    }

    pub fn transcribe_with_translation(
        &self,
        audio_path: &Path,
        language: Option<&str>,
        translate: bool,
    ) -> Result<String, String> {
        self.transcribe_with_cancellation(audio_path, language, translate, || false)
    }

    pub fn transcribe_with_cancellation<F>(
        &self,
        audio_path: &Path,
        language: Option<&str>,
        translate: bool,
        should_cancel: F,
    ) -> Result<String, String>
    where
        F: Fn() -> bool,
    {
        let transcription_start = Instant::now();
        let audio_path_str = format!("{:?}", audio_path);
        
        // Monitor system resources before transcription (only in debug builds)
        #[cfg(debug_assertions)]
        system_monitor::log_resources_before_operation("TRANSCRIPTION");
        
        log_start("TRANSCRIPTION");
        log_with_context(log::Level::Debug, "Starting transcription", &[
            ("audio_path", &audio_path_str),
            ("language", language.unwrap_or("auto")),
            ("translate", &translate.to_string().as_str()),
            ("timestamp", &chrono::Utc::now().to_rfc3339().as_str())
        ]);

        // Check if file exists and is readable
        if !audio_path.exists() {
            let error = format!("Audio file does not exist: {:?}", audio_path);
            log_failed("TRANSCRIPTION", &error);
            log_with_context(log::Level::Debug, "File validation failed", &[
                ("stage", "file_validation"),
                ("audio_path", &audio_path_str)
            ]);
            return Err(error);
        }

        // Early cancellation check
        if should_cancel() {
            log::info!("[TRANSCRIPTION_DEBUG] Transcription cancelled before starting");
            return Err("Transcription cancelled".to_string());
        }

        let file_size = std::fs::metadata(audio_path)
            .map_err(|e| {
                let error = format!("Cannot read file metadata: {}", e);
                log_failed("TRANSCRIPTION", &error);
                log_with_context(log::Level::Debug, "Metadata read failed", &[
                    ("stage", "metadata_read"),
                    ("audio_path", &audio_path_str)
                ]);
                error
            })?
            .len();
            
        log_file_operation("TRANSCRIPTION_INPUT", &audio_path_str, true, Some(file_size), None);

        if file_size == 0 {
            let error = "Audio file is empty (0 bytes)";
            log_failed("TRANSCRIPTION", error);
            log_with_context(log::Level::Debug, "File size check failed", &[
                ("stage", "file_size_check"),
                ("file_size", "0")
            ]);
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

        // Check cancellation after reading samples
        if should_cancel() {
            log::info!("[TRANSCRIPTION_DEBUG] Transcription cancelled after reading samples");
            return Err("Transcription cancelled".to_string());
        }

        /* ----------------------------------------------
        2) i16 → f32  (range -1.0 … 1.0)
        ---------------------------------------------- */
        let mut audio: Vec<f32> = vec![0.0; samples_i16.len()];
        convert_integer_to_float_audio(&samples_i16, &mut audio).map_err(|e| e.to_string())?;

        // Check cancellation after conversion
        if should_cancel() {
            log::info!("[TRANSCRIPTION_DEBUG] Transcription cancelled after audio conversion");
            return Err("Transcription cancelled".to_string());
        }

        /* ----------------------------------------------
        3) stereo → mono  (Whisper needs mono)
        ---------------------------------------------- */
        if spec.channels == 2 {
            audio = convert_stereo_to_mono_audio(&audio).map_err(|e| e.to_string())?;
        } else if spec.channels != 1 {
            return Err(format!("Unsupported channel count: {}", spec.channels));
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
        log_performance("AUDIO_PREPROCESSING", preprocessing_time, Some(&format!("samples={}", resampled_audio.len())));
        log_with_context(log::Level::Debug, "Audio preprocessing complete", &[
            ("preprocessing_time_ms", &preprocessing_time.to_string().as_str()),
            ("sample_rate", "16000"),
            ("channels", "1"),
            ("samples", &resampled_audio.len().to_string().as_str())
        ]);

        // Check cancellation after resampling
        if should_cancel() {
            log::info!("[TRANSCRIPTION_DEBUG] Transcription cancelled after resampling");
            return Err("Transcription cancelled".to_string());
        }

        log::debug!(
            "Audio ready for Whisper: {} samples at 16kHz ({:.2}s)",
            resampled_audio.len(),
            resampled_audio.len() as f32 / 16_000_f32
        );

        // Create transcription parameters - use BeamSearch for better accuracy
        let mut params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: 5,
            patience: -1.0,
        });

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

        // Use all available CPU cores for multi-threaded processing
        let threads = std::thread::available_parallelism()
            .map(|n| n.get() as i32)
            .unwrap_or(4); // Default to 4 threads if detection fails
        params.set_n_threads(threads);
        log::info!("[PERFORMANCE] Using {} threads for transcription", threads);

        params.set_no_context(false); // Enable context for better word recognition
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

        // Temperature settings - slight randomness helps avoid repetitive loops
        params.set_temperature(0.2); // Small amount of randomness instead of deterministic
        params.set_temperature_inc(0.2); // Increase by 0.2 on fallback (default)
        params.set_max_initial_ts(1.0); // Limit initial timestamp search

        // Limit segment length to prevent runaway hallucinations
        params.set_max_len(0); // 0 means no limit
        params.set_length_penalty(-1.0); // Default penalty

        // Run transcription
        log::info!("[TRANSCRIPTION_DEBUG] Creating Whisper state...");
        let mut state = self.context.create_state().map_err(|e| {
            let error = format!("Failed to create Whisper state: {}", e);
            log::error!("[TRANSCRIPTION_DEBUG] {}", error);

            error
        })?;

        let samples_count = resampled_audio.len();
        let duration_seconds = samples_count as f32 / 16_000_f32;
        
        log_audio_metrics("WHISPER_INPUT", 0.0, 0.0, duration_seconds, None);

        let inference_start = Instant::now();
        log_start("WHISPER_INFERENCE");
        log_with_context(log::Level::Debug, "Starting Whisper inference", &[
            ("samples", &samples_count.to_string().as_str()),
            ("duration_seconds", &format!("{:.2}", duration_seconds).as_str()),
            ("language", language.unwrap_or("auto")),
            ("translate", &translate.to_string().as_str())
        ]);

        match state.full(params, &resampled_audio) {
            Ok(_) => {
                let inference_time = inference_start.elapsed();
                let inference_ms = inference_time.as_millis();
                
                log_performance("WHISPER_INFERENCE", inference_ms as u64, 
                    Some(&format!("audio_duration={:.2}s, samples={}", duration_seconds, samples_count)));
                    
                log::info!("✅ Whisper inference completed in {:.2}s", inference_time.as_secs_f32());
            }
            Err(e) => {
                let error = format!("Whisper inference failed: {}", e);
                log_failed("WHISPER_INFERENCE", &error);
                log_with_context(log::Level::Debug, "Inference failed", &[
                    ("samples", &samples_count.to_string().as_str()),
                    ("duration_seconds", &format!("{:.2}", duration_seconds).as_str()),
                    ("inference_time_ms", &inference_start.elapsed().as_millis().to_string().as_str())
                ]);
                return Err(error);
            }
        }

        // Get text
        let text_extraction_start = Instant::now();
        log::info!("[TRANSCRIPTION_DEBUG] Getting segments from Whisper output...");
        let num_segments = state.full_n_segments().map_err(|e| {
            let error = format!("Failed to get segments: {}", e);
            log::error!("[TRANSCRIPTION_DEBUG] {}", error);
            error
        })?;

        log::info!(
            "[TRANSCRIPTION_DEBUG] Transcription complete: {} segments",
            num_segments
        );

        let mut text = String::new();
        for i in 0..num_segments {
            let segment = state.full_get_segment_text(i).map_err(|e| {
                let error = format!("Failed to get segment {}: {}", i, e);
                log::error!("[TRANSCRIPTION_DEBUG] {}", error);
                error
            })?;
            log::info!("[TRANSCRIPTION_DEBUG] Segment {}: '{}'", i, segment);
            text.push_str(&segment);
            text.push(' ');
        }

        let result = text.trim().to_string();
        
        // Log text extraction performance
        let extraction_time = text_extraction_start.elapsed().as_millis() as u64;
        log_performance("TEXT_EXTRACTION", extraction_time, Some(&format!("segments={}, chars={}", num_segments, result.len())));
        
        let total_time = transcription_start.elapsed();
        
        // Log system resources after transcription (only in debug builds)
        #[cfg(debug_assertions)]
        system_monitor::log_resources_after_operation("TRANSCRIPTION", total_time.as_millis() as u64);
        
        if result.is_empty() {
            log_failed("TRANSCRIPTION", "Empty transcription result");
            log_with_context(log::Level::Debug, "Empty result", &[
                ("segments", &num_segments.to_string().as_str()),
                ("total_time_ms", &total_time.as_millis().to_string().as_str())
            ]);
        } else if result == "[SOUND]" {
            log_failed("TRANSCRIPTION", "No speech detected");
            log_with_context(log::Level::Debug, "No speech", &[
                ("result", "[SOUND]"),
                ("segments", &num_segments.to_string().as_str()),
                ("total_time_ms", &total_time.as_millis().to_string().as_str())
            ]);
        } else {
            log_complete("TRANSCRIPTION", total_time.as_millis() as u64);
            log_with_context(log::Level::Debug, "Transcription complete", &[
                ("result_length", &result.len().to_string().as_str()),
                ("segments", &num_segments.to_string().as_str()),
                ("audio_duration_seconds", &format!("{:.2}", duration_seconds).as_str())
            ]);
            
            log::info!("📝 Transcription success: {} chars in {:.2}s", result.len(), total_time.as_secs_f32());
        }

        Ok(result)
    }
}
