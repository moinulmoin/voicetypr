use std::path::Path;
use whisper_rs::{
    convert_integer_to_float_audio, convert_stereo_to_mono_audio, FullParams, SamplingStrategy,
    WhisperContext, WhisperContextParameters,
};

pub struct Transcriber {
    context: WhisperContext,
}

impl Transcriber {
    pub fn new(model_path: &Path) -> Result<Self, String> {
        let model_path_str = model_path
            .to_str()
            .ok_or_else(|| format!("Model path contains invalid UTF-8: {:?}", model_path))?;

        // Configure GPU usage based on platform and features
        let mut ctx_params = WhisperContextParameters::default();
        let mut gpu_used = false;
        
        // macOS: Try Metal first, fallback to CPU if it fails
        #[cfg(target_os = "macos")]
        {
            ctx_params.use_gpu(true);
            log::info!("Attempting to initialize Whisper with Metal acceleration...");
            
            match WhisperContext::new_with_params(model_path_str, ctx_params) {
                Ok(ctx) => {
                    log::info!("✓ Successfully initialized with Metal GPU acceleration");
                    gpu_used = true;
                    return Ok(Self { context: ctx });
                }
                Err(gpu_err) => {
                    log::warn!("Metal initialization failed: {}. Falling back to CPU...", gpu_err);
                    ctx_params = WhisperContextParameters::default();
                    ctx_params.use_gpu(false);
                    log::info!("Attempting CPU-only initialization...");
                }
            }
        }
        
        // Windows: Use Vulkan GPU if feature is enabled, otherwise CPU
        #[cfg(all(target_os = "windows", feature = "gpu-windows"))]
        {
            ctx_params.use_gpu(true);
            gpu_used = true;
            log::info!("Initializing with Vulkan GPU acceleration (gpu-windows feature enabled)");
        }
        
        #[cfg(all(target_os = "windows", not(feature = "gpu-windows")))]
        {
            ctx_params.use_gpu(false);
            gpu_used = false;
            log::info!("Initializing in CPU-only mode (maximum compatibility)");
        }

        // Create context (for Windows or macOS CPU fallback)
        let ctx = WhisperContext::new_with_params(model_path_str, ctx_params)
            .map_err(|e| format!("Failed to load model: {}", e))?;

        // Determine backend type for logging
        let backend_type = if gpu_used {
            #[cfg(all(target_os = "windows", feature = "gpu-windows"))]
            { "Vulkan GPU" }
            #[cfg(not(all(target_os = "windows", feature = "gpu-windows")))]
            { "CPU" }  // macOS fallback case
        } else {
            "CPU"
        };
        
        log::info!("✓ Whisper initialized successfully using {} backend", backend_type);

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
        log::info!(
            "[TRANSCRIPTION_DEBUG] Starting transcription of: {:?}",
            audio_path
        );

        // Check if file exists and is readable
        if !audio_path.exists() {
            let error = format!("Audio file does not exist: {:?}", audio_path);
            log::error!("[TRANSCRIPTION_DEBUG] {}", error);

            return Err(error);
        }

        // Early cancellation check
        if should_cancel() {
            log::info!("[TRANSCRIPTION_DEBUG] Transcription cancelled before starting");
            return Err("Transcription cancelled".to_string());
        }

        let file_size = std::fs::metadata(audio_path)
            .map_err(|e| format!("Cannot read file metadata: {}", e))?
            .len();
        log::info!("[TRANSCRIPTION_DEBUG] Audio file size: {} bytes", file_size);

        if file_size == 0 {
            let error = "Audio file is empty (0 bytes)";
            log::error!("[TRANSCRIPTION_DEBUG] {}", error);
            return Err(error.to_string());
        }

        // Read WAV file
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

        // Quality thresholds with temperature fallback
        // Lower entropy threshold to be more conservative
        params.set_entropy_thold(2.0); // Lower than default 2.4 to filter out more uncertain predictions

        // Stricter log probability threshold to enforce quality
        params.set_logprob_thold(-1.5); // Lower than default -1.0 for stricter probability requirements

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

        log::info!(
            "[TRANSCRIPTION_DEBUG] Running Whisper inference with {} samples...",
            resampled_audio.len()
        );

        let start_time = std::time::Instant::now();
        log::info!("[TRANSCRIPTION_DEBUG] Starting whisper full() inference...");

        state.full(params, &resampled_audio).map_err(|e| {
            let error = format!("Whisper inference failed: {}", e);
            log::error!("[TRANSCRIPTION_DEBUG] {}", error);

            error
        })?;

        log::info!(
            "[TRANSCRIPTION_DEBUG] Whisper inference completed in {:.2}s",
            start_time.elapsed().as_secs_f32()
        );

        // Get text
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
        if result.is_empty() {
            log::warn!("[TRANSCRIPTION_DEBUG] Transcription resulted in empty output");
        } else if result == "[SOUND]" {
            log::warn!("[TRANSCRIPTION_DEBUG] Transcription resulted in [SOUND] output (no speech detected)");
        } else {
            log::info!(
                "[TRANSCRIPTION_DEBUG] Final transcription: {} characters",
                result.len()
            );
        }

        Ok(result)
    }
}
