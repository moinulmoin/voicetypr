use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::Instant;

use rubato::{audioadapter_buffers::direct::InterleavedSlice, Fft, FixedSync, Resampler};
use serde::{Deserialize, Serialize};
use whisper_rs::{
    convert_integer_to_float_audio, convert_stereo_to_mono_audio, get_lang_str, FullParams,
    SamplingStrategy, WhisperContext, WhisperContextParameters,
};

#[derive(Debug, Deserialize, Serialize)]
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
        initial_prompt: Option<String>,
    },
    Shutdown {
        id: u64,
    },
}

#[derive(Debug, Serialize)]
struct Segment {
    start: f64,
    end: f64,
    text: String,
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
        load_time_ms: u64,
    },
    Transcription {
        id: u64,
        ok: bool,
        backend: &'a str,
        text: String,
        transcript_language: Option<String>,
        segments: Vec<Segment>,
        audio_duration_ms: u64,
        processing_duration_ms: u64,
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

struct TranscriptionOutput {
    text: String,
    transcript_language: Option<String>,
    segments: Vec<Segment>,
    audio_duration_ms: u64,
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
                    load_time_ms: elapsed_millis_u64(started),
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
            initial_prompt,
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
                    initial_prompt.as_deref(),
                ) {
                    Ok(output) => Response::Transcription {
                        id,
                        ok: true,
                        backend: "vulkan",
                        text: output.text,
                        transcript_language: output.transcript_language,
                        segments: output.segments,
                        audio_duration_ms: output.audio_duration_ms,
                        processing_duration_ms: elapsed_millis_u64(started),
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

fn sanitize_initial_prompt(initial_prompt: Option<&str>) -> String {
    let mut prompt = initial_prompt.unwrap_or("").to_string();
    prompt.retain(|ch| ch != '\0');
    prompt
}

fn segment_timestamp_seconds(timestamp_centiseconds: i64) -> f64 {
    timestamp_centiseconds as f64 / 100.0
}

fn resolve_transcript_language(
    language: Option<&str>,
    translate: bool,
    state: &whisper_rs::WhisperState,
    threads: usize,
) -> Option<String> {
    if translate {
        return Some("en".to_string());
    }

    match language {
        Some(lang) if lang != "auto" => Some(lang.to_string()),
        _ => state
            .lang_detect(0, threads)
            .ok()
            .and_then(|(lang_id, _)| get_lang_str(lang_id).map(str::to_string)),
    }
}

fn transcribe_with_context(
    context: &WhisperContext,
    audio_path: &Path,
    language: Option<&str>,
    translate: bool,
    initial_prompt: Option<&str>,
) -> Result<TranscriptionOutput, String> {
    let audio = load_audio_16khz_mono(audio_path)?;
    let audio_duration_ms = ((audio.len() as f32 / 16_000.0) * 1_000.0) as u64;
    let duration_seconds = audio.len() as f32 / 16_000.0;
    if duration_seconds < 0.5 {
        return Err("Recording too short".to_string());
    }

    let mut params = FullParams::new(SamplingStrategy::BeamSearch {
        beam_size: 5,
        patience: -1.0,
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
    let threads = std::cmp::max(1, hw.saturating_sub(1));
    params.set_n_threads(threads as i32);
    params.set_no_context(false);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    params.set_no_speech_thold(0.6);
    params.set_entropy_thold(2.4);
    params.set_logprob_thold(-1.0);

    let prompt = sanitize_initial_prompt(initial_prompt);
    params.set_initial_prompt(&prompt);

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

    let transcript_language =
        resolve_transcript_language(language, translate, &state, threads);

    let mut text = String::new();
    let mut segments = Vec::new();
    for segment in state.as_iter() {
        let segment_text = segment.to_string();
        text.push_str(&segment_text);
        text.push(' ');
        segments.push(Segment {
            start: segment_timestamp_seconds(segment.start_timestamp()),
            end: segment_timestamp_seconds(segment.end_timestamp()),
            text: segment_text,
        });
    }

    Ok(TranscriptionOutput {
        text: text.trim().to_string(),
        transcript_language,
        segments,
        audio_duration_ms,
    })
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

fn elapsed_millis_u64(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{sanitize_initial_prompt, Request, Response, Segment};

    #[test]
    fn timing_responses_serialize_as_json_numbers() {
        let probe = serde_json::to_string(&Response::Probe {
            id: 7,
            ok: true,
            backend: "vulkan",
            load_time_ms: 123,
        })
        .expect("probe response should serialize");
        assert_eq!(
            probe,
            r#"{"type":"probe","id":7,"ok":true,"backend":"vulkan","load_time_ms":123}"#
        );

        let transcription = serde_json::to_string(&Response::Transcription {
            id: 8,
            ok: true,
            backend: "vulkan",
            text: "hello".to_string(),
            transcript_language: Some("en".to_string()),
            segments: vec![Segment {
                start: 0.0,
                end: 1.2,
                text: "hello".to_string(),
            }],
            audio_duration_ms: 1200,
            processing_duration_ms: 456,
        })
        .expect("transcription response should serialize");
        assert_eq!(
            transcription,
            r#"{"type":"transcription","id":8,"ok":true,"backend":"vulkan","text":"hello","transcript_language":"en","segments":[{"start":0.0,"end":1.2,"text":"hello"}],"audio_duration_ms":1200,"processing_duration_ms":456}"#
        );
    }

    #[test]
    fn transcribe_request_roundtrips_initial_prompt() {
        let original = Request::Transcribe {
            id: 9,
            model_path: "/models/whisper.bin".to_string(),
            audio_path: "/tmp/audio.wav".to_string(),
            language: Some("en".to_string()),
            translate: false,
            initial_prompt: Some("VoiceTypr".to_string()),
        };

        let json = serde_json::to_string(&original).expect("request should serialize");
        let roundtripped: Request =
            serde_json::from_str(&json).expect("request should deserialize");

        match roundtripped {
            Request::Transcribe {
                id,
                model_path,
                audio_path,
                language,
                translate,
                initial_prompt,
            } => {
                assert_eq!(id, 9);
                assert_eq!(model_path, "/models/whisper.bin");
                assert_eq!(audio_path, "/tmp/audio.wav");
                assert_eq!(language.as_deref(), Some("en"));
                assert!(!translate);
                assert_eq!(initial_prompt.as_deref(), Some("VoiceTypr"));
            }
            _ => panic!("expected transcribe request"),
        }
    }

    #[test]
    fn sanitize_initial_prompt_strips_nul_bytes() {
        assert_eq!(sanitize_initial_prompt(Some("hello\0world")), "helloworld");
        assert_eq!(sanitize_initial_prompt(None), "");
    }
}

fn write_response(stdout: &mut io::Stdout, response: &Response<'_>) -> io::Result<()> {
    serde_json::to_writer(&mut *stdout, response)?;
    stdout.write_all(b"\n")?;
    stdout.flush()
}
