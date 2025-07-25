use std::path::Path;
use whisper_rs::{
    convert_integer_to_float_audio,
    convert_stereo_to_mono_audio,
    FullParams,
    SamplingStrategy,
    WhisperContext,
    WhisperContextParameters,
};

pub struct Transcriber {
    context: WhisperContext,
}

impl Transcriber {
    pub fn new(model_path: &Path) -> Result<Self, String> {
        let model_path_str = model_path
            .to_str()
            .ok_or_else(|| format!("Model path contains invalid UTF-8: {:?}", model_path))?;

        // Enable GPU acceleration for better performance
        let mut ctx_params = WhisperContextParameters::default();
        ctx_params.use_gpu(true); // Enable Metal on macOS
        
        let ctx =
            WhisperContext::new_with_params(model_path_str, ctx_params)
                .map_err(|e| format!("Failed to load model: {}", e))?;

        Ok(Self { context: ctx })
    }

    pub fn transcribe_with_translation(&self, audio_path: &Path, language: Option<&str>, translate: bool) -> Result<String, String> {
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

        /* ----------------------------------------------
        4) Let Whisper handle resampling internally
        ---------------------------------------------- */
        // Whisper will resample to 16kHz internally if needed
        // No need for us to do it manually

        // Check cancellation after resampling
        if should_cancel() {
            log::info!("[TRANSCRIPTION_DEBUG] Transcription cancelled after resampling");
            return Err("Transcription cancelled".to_string());
        }

        log::debug!(
            "Audio normalised → mono 16 kHz: {} samples ({:.2}s)",
            audio.len(),
            audio.len() as f32 / 16_000_f32
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
                log::info!("[LANGUAGE] Auto-detection enabled");
                None
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
        } else {
            log::info!("[LANGUAGE] Final: Auto-detect mode");
            params.set_detect_language(true);
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

        params.set_no_context(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Suppress blank outputs to avoid empty transcriptions
        params.set_suppress_blank(true);
        
        // Suppress non-speech tokens like [MUSIC], [SOUND] for cleaner output
        params.set_suppress_nst(true);

        // Run transcription
        log::info!("[TRANSCRIPTION_DEBUG] Creating Whisper state...");
        let mut state = self.context.create_state().map_err(|e| {
            let error = format!("Failed to create Whisper state: {}", e);
            log::error!("[TRANSCRIPTION_DEBUG] {}", error);
            error
        })?;

        log::info!(
            "[TRANSCRIPTION_DEBUG] Running Whisper inference with {} samples...",
            audio.len()
        );
        state.full(params, &audio).map_err(|e| {
            let error = format!("Whisper inference failed: {}", e);
            log::error!("[TRANSCRIPTION_DEBUG] {}", error);
            error
        })?;

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

