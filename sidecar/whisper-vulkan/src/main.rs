use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::Instant;

use rubato::{audioadapter_buffers::direct::InterleavedSlice, Fft, FixedSync, Resampler};
use serde::{Deserialize, Serialize};
use whisper_rs::{
    convert_integer_to_float_audio, convert_stereo_to_mono_audio, FullParams, SamplingStrategy,
    WhisperContext, WhisperContextParameters,
};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request {
    Health {
        id: u64,
    },
    Probe {
        id: u64,
        model_path: String,
    },
    Transcribe {
        id: u64,
        model_path: String,
        audio_path: String,
        language: Option<String>,
        translate: bool,
    },
    Shutdown {
        id: u64,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Response<'a> {
    Health {
        id: u64,
        ok: bool,
        backend: &'a str,
    },
    Probe {
        id: u64,
        ok: bool,
        backend: &'a str,
        load_time_ms: u128,
    },
    Transcription {
        id: u64,
        ok: bool,
        backend: &'a str,
        text: String,
        inference_time_ms: u128,
    },
    Shutdown {
        id: u64,
        ok: bool,
        backend: &'a str,
    },
    Error {
        id: u64,
        ok: bool,
        code: &'a str,
        message: String,
    },
}

struct CachedContext {
    model_path: String,
    context: WhisperContext,
}

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut cache: Option<CachedContext> = None;

    for line_result in stdin.lock().lines() {
        let line = match line_result {
            Ok(line) => line,
            Err(err) => {
                let _ = write_response(
                    &mut stdout,
                    &Response::Error {
                        id: 0,
                        ok: false,
                        code: "stdin_read_failed",
                        message: err.to_string(),
                    },
                );
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let request = match serde_json::from_str::<Request>(&line) {
            Ok(request) => request,
            Err(err) => {
                let _ = write_response(
                    &mut stdout,
                    &Response::Error {
                        id: 0,
                        ok: false,
                        code: "invalid_request",
                        message: format!("invalid JSON request: {err}"),
                    },
                );
                continue;
            }
        };

        let should_shutdown = matches!(request, Request::Shutdown { .. });
        let response = handle_request(request, &mut cache);
        if write_response(&mut stdout, &response).is_err() || should_shutdown {
            break;
        }
    }
}

fn handle_request<'a>(request: Request, cache: &'a mut Option<CachedContext>) -> Response<'a> {
    match request {
        Request::Health { id } => Response::Health {
            id,
            ok: true,
            backend: "vulkan",
        },
        Request::Probe { id, model_path } => {
            let started = Instant::now();
            match ensure_context(cache, &model_path) {
                Ok(_) => Response::Probe {
                    id,
                    ok: true,
                    backend: "vulkan",
                    load_time_ms: started.elapsed().as_millis(),
                },
                Err(message) => Response::Error {
                    id,
                    ok: false,
                    code: "probe_failed",
                    message,
                },
            }
        }
        Request::Transcribe {
            id,
            model_path,
            audio_path,
            language,
            translate,
        } => {
            let started = Instant::now();
            match ensure_context(cache, &model_path) {
                Err(message) => Response::Error {
                    id,
                    ok: false,
                    code: "context_failed",
                    message,
                },
                Ok(ctx) => match transcribe_with_context(
                    ctx,
                    Path::new(&audio_path),
                    language.as_deref(),
                    translate,
                ) {
                    Ok(text) => Response::Transcription {
                        id,
                        ok: true,
                        backend: "vulkan",
                        text,
                        inference_time_ms: started.elapsed().as_millis(),
                    },
                    Err(message) => Response::Error {
                        id,
                        ok: false,
                        code: "transcription_failed",
                        message,
                    },
                },
            }
        }
        Request::Shutdown { id } => Response::Shutdown {
            id,
            ok: true,
            backend: "vulkan",
        },
    }
}

fn ensure_context<'a>(
    cache: &'a mut Option<CachedContext>,
    model_path: &str,
) -> Result<&'a WhisperContext, String> {
    if cache
        .as_ref()
        .map(|cached| cached.model_path.as_str() != model_path)
        .unwrap_or(true)
    {
        let mut params = WhisperContextParameters::default();
        params.use_gpu(true);
        let context = WhisperContext::new_with_params(model_path, params)
            .map_err(|err| format!("failed to load Vulkan Whisper context: {err}"))?;
        cache.replace(CachedContext {
            model_path: model_path.to_string(),
            context,
        });
    }

    cache
        .as_ref()
        .map(|cached| &cached.context)
        .ok_or_else(|| "Vulkan context cache was not initialized".to_string())
}

fn transcribe_with_context(
    context: &WhisperContext,
    audio_path: &Path,
    language: Option<&str>,
    translate: bool,
) -> Result<String, String> {
    let audio = load_audio_16khz_mono(audio_path)?;
    let duration_seconds = audio.len() as f32 / 16_000.0;
    if duration_seconds < 0.5 {
        return Err("Recording too short".to_string());
    }

    let mut params = FullParams::new(SamplingStrategy::BeamSearch {
        beam_size: 5,
        patience: -1.0, // whisper.cpp sentinel: use the default beam-search patience.
    });

    let final_language = match language {
        Some("auto") | None => Some("en"),
        Some(language) => Some(language),
    };
    params.set_language(final_language);
    params.set_translate(translate);

    let hw = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    params.set_n_threads(std::cmp::max(1, hw.saturating_sub(1)) as i32);
    params.set_no_context(false); // Match the main transcriber; each request creates a fresh WhisperState.
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    params.set_no_speech_thold(0.6);
    params.set_entropy_thold(2.4);
    params.set_logprob_thold(-1.0);
    params.set_initial_prompt("");
    params.set_temperature(0.2);
    params.set_temperature_inc(0.2);
    params.set_max_initial_ts(1.0);
    params.set_max_len(0);
    params.set_length_penalty(-1.0);

    let mut state = context
        .create_state()
        .map_err(|err| format!("failed to create Whisper state: {err}"))?;
    state
        .full(params, &audio)
        .map_err(|err| format!("Whisper inference failed: {err}"))?;

    let mut text = String::new();
    for segment in state.as_iter() {
        text.push_str(&segment.to_string());
        text.push(' ');
    }

    Ok(text.trim().to_string())
}

fn load_audio_16khz_mono(audio_path: &Path) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(audio_path)
        .map_err(|err| format!("failed to open WAV file: {err}"))?;
    let spec = reader.spec();
    if spec.bits_per_sample != 16 || spec.sample_format != hound::SampleFormat::Int {
        return Err(format!(
            "unsupported WAV format: {}-bit {:?}; expected 16-bit PCM",
            spec.bits_per_sample, spec.sample_format
        ));
    }

    let samples_i16: Vec<i16> = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to read WAV samples: {err}"))?;

    let mut audio = vec![0.0; samples_i16.len()];
    convert_integer_to_float_audio(&samples_i16, &mut audio).map_err(|err| err.to_string())?;

    if spec.channels == 2 {
        let mut mono = vec![0.0; audio.len() / 2];
        convert_stereo_to_mono_audio(&audio, &mut mono).map_err(|err| err.to_string())?;
        audio = mono;
    } else if spec.channels > 2 {
        audio = convert_multichannel_to_mono(&audio, spec.channels as usize)?;
    } else if spec.channels != 1 {
        return Err(format!("invalid channel count: {}", spec.channels));
    }

    if spec.sample_rate == 16_000 {
        return Ok(audio);
    }

    resample_to_16khz(&audio, spec.sample_rate)
}

fn convert_multichannel_to_mono(audio: &[f32], channels: usize) -> Result<Vec<f32>, String> {
    if channels == 0 {
        return Err("channel count cannot be zero".to_string());
    }
    if channels == 1 {
        return Ok(audio.to_vec());
    }

    let samples_per_channel = audio.len() / channels;
    let mut mono = Vec::with_capacity(samples_per_channel);
    for frame in audio.chunks_exact(channels) {
        let sum: f32 = frame.iter().copied().sum();
        mono.push(sum / channels as f32);
    }
    Ok(mono)
}

fn resample_to_16khz(audio: &[f32], sample_rate: u32) -> Result<Vec<f32>, String> {
    if audio.is_empty() {
        return Ok(Vec::new());
    }

    let input_frames = audio.len();
    let input = InterleavedSlice::new(audio, 1, input_frames)
        .map_err(|err| format!("failed to adapt resampler input: {err}"))?;

    let mut resampler = Fft::<f32>::new(sample_rate as usize, 16_000, 1024, 2, 1, FixedSync::Both)
        .map_err(|err| format!("failed to create resampler: {err}"))?;

    let output_capacity = resampler.process_all_needed_output_len(input_frames);
    let mut output = vec![0.0; output_capacity];
    let mut output_adapter = InterleavedSlice::new_mut(&mut output, 1, output_capacity)
        .map_err(|err| format!("failed to adapt resampler output: {err}"))?;

    let (_, output_frames) = resampler
        .process_all_into_buffer(&input, &mut output_adapter, input_frames, None)
        .map_err(|err| format!("resampling failed: {err}"))?;
    output.truncate(output_frames);

    Ok(output)
}

fn write_response(stdout: &mut io::Stdout, response: &Response<'_>) -> io::Result<()> {
    serde_json::to_writer(&mut *stdout, response)?;
    stdout.write_all(b"\n")?;
    stdout.flush()
}
