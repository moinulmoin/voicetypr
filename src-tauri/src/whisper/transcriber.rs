use std::path::Path;
use whisper_rs::{
    convert_integer_to_float_audio,
    convert_stereo_to_mono_audio,
    // crate version <0.15 does not provide a helper for arbitrary resampling,
    // we implement a lightweight linear-interpolation resampler below.
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
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().unwrap(),
            WhisperContextParameters::default(),
        )
        .map_err(|e| format!("Failed to load model: {}", e))?;

        Ok(Self { context: ctx })
    }

    pub fn transcribe(&self, audio_path: &Path, language: Option<&str>) -> Result<String, String> {
        // Read WAV file
        let mut reader = hound::WavReader::open(audio_path).map_err(|e| e.to_string())?;

        let spec = reader.spec();
        log::debug!(
            "WAV spec: channels={}, sample_rate={}, bits={}",
            spec.channels,
            spec.sample_rate,
            spec.bits_per_sample
        );

        /* ----------------------------------------------
        1) read raw i16 pcm
        ---------------------------------------------- */
        let samples_i16: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();

        /* ----------------------------------------------
        2) i16 → f32  (range -1.0 … 1.0)
        ---------------------------------------------- */
        let mut audio: Vec<f32> = vec![0.0; samples_i16.len()];
        convert_integer_to_float_audio(&samples_i16, &mut audio).map_err(|e| e.to_string())?;

        /* ----------------------------------------------
        3) stereo → mono  (Whisper needs mono)
        ---------------------------------------------- */
        if spec.channels == 2 {
            audio = convert_stereo_to_mono_audio(&audio).map_err(|e| e.to_string())?;
        } else if spec.channels != 1 {
            return Err(format!("Unsupported channel count: {}", spec.channels));
        }

        /* ----------------------------------------------
        4) resample to 16 000 Hz  (model's native rate)
        ---------------------------------------------- */
        if spec.sample_rate != 16_000 {
            audio = resample_linear(&audio, spec.sample_rate as usize, 16_000);
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

        // Set language - default to English instead of auto
        let final_lang = if let Some(lang) = language {
            if lang == "auto" {
                // If explicitly set to auto, use English as default
                "en"
            } else {
                // Validate language code
                match lang {
                    "en" | "es" | "fr" | "de" | "it" | "pt" | "ru" | "ja" | "ko" | "zh" => lang,
                    _ => {
                        log::warn!("Invalid language code '{}', defaulting to English", lang);
                        "en"
                    }
                }
            }
        } else {
            // Default to English
            "en"
        };

        log::info!("Using language for transcription: {}", final_lang);
        params.set_language(Some(final_lang));

        // Explicitly set to transcribe mode (not translate)
        params.set_translate(false);

        params.set_no_context(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Don't suppress tokens - let Whisper decide
        // params.set_suppress_nst(true);

        // Run transcription
        log::info!("Starting transcription...");
        let mut state = self.context.create_state().map_err(|e| e.to_string())?;

        state
            .full(params, &audio)
            .map_err(|e| format!("Transcription failed: {}", e))?;

        // Get text
        let num_segments = state.full_n_segments().map_err(|e| e.to_string())?;

        log::info!("Transcription complete: {} segments", num_segments);

        let mut text = String::new();
        for i in 0..num_segments {
            let segment = state.full_get_segment_text(i).map_err(|e| e.to_string())?;
            log::debug!("Segment {}: {}", i, segment);
            text.push_str(&segment);
            text.push(' ');
        }

        let result = text.trim().to_string();
        if result.is_empty() || result == "[SOUND]" {
            log::warn!("Transcription resulted in empty or [SOUND] output");
        }

        Ok(result)
    }
}

/// Naive linear-interpolation resampler for mono f32 audio.
/// Good enough for converting 48 kHz → 16 kHz speech.
fn resample_linear(input: &[f32], in_rate: usize, out_rate: usize) -> Vec<f32> {
    if in_rate == out_rate {
        return input.to_vec();
    }

    let ratio = out_rate as f64 / in_rate as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut output = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let pos = (i as f64) / ratio;
        let idx = pos.floor() as usize;
        let frac = pos - (idx as f64);

        let sample = if idx + 1 < input.len() {
            // linear interp
            input[idx] * (1.0 - frac as f32) + input[idx + 1] * (frac as f32)
        } else {
            input[idx]
        };
        output.push(sample);
    }
    output
}
