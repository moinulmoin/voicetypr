use std::fs::File;
use std::path::{Path, PathBuf};
use symphonia::core::audio::AudioBufferRef;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Convert any audio file to WAV format (16kHz, mono, 16-bit PCM)
/// Returns the path to the converted WAV file
#[allow(dead_code)]
pub fn convert_to_wav(input_path: &Path, output_dir: &Path) -> Result<PathBuf, String> {
    // Check if input is already a WAV file
    if input_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase() == "wav")
        .unwrap_or(false)
    {
        // Already a WAV, return the input path
        return Ok(input_path.to_path_buf());
    }

    // Create output WAV path
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("Time error: {}", e))?
        .as_secs();
    let output_path = output_dir.join(format!("converted_{}.wav", timestamp));

    // Open the input file
    let file = File::open(input_path).map_err(|e| format!("Failed to open audio file: {}", e))?;

    // Create media source stream
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    // Create probe hint from file extension
    let mut hint = Hint::new();
    if let Some(ext) = input_path.extension() {
        if let Some(ext_str) = ext.to_str() {
            hint.with_extension(ext_str);
        }
    }

    // Probe the media source
    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = Default::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)
        .map_err(|e| format!("Failed to probe audio format: {}", e))?;

    // Get format reader
    let mut format = probed.format;

    // Find first audio track
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| "No supported audio tracks found".to_string())?;

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let channels = track
        .codec_params
        .channels
        .map(|ch| ch.count())
        .unwrap_or(2);

    // Create decoder
    let dec_opts: DecoderOptions = Default::default();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &dec_opts)
        .map_err(|e| format!("Failed to create audio decoder: {}", e))?;

    // Decode all audio samples
    let mut all_samples = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(_)) => break,
            Err(Error::ResetRequired) => break,
            Err(err) => return Err(format!("Error reading packet: {}", err)),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                // Convert to i16 samples
                let samples_i16 = match decoded {
                    AudioBufferRef::S32(buf) => {
                        let mut samples = Vec::new();
                        for plane in buf.planes().planes() {
                            for &sample in plane.iter() {
                                samples.push((sample >> 16) as i16);
                            }
                        }
                        samples
                    }
                    AudioBufferRef::S16(buf) => {
                        let mut samples = Vec::new();
                        for plane in buf.planes().planes() {
                            samples.extend_from_slice(plane);
                        }
                        samples
                    }
                    AudioBufferRef::F32(buf) => {
                        let mut samples = Vec::new();
                        for plane in buf.planes().planes() {
                            for &sample in plane.iter() {
                                samples.push((sample.clamp(-1.0, 1.0) * 32767.0) as i16);
                            }
                        }
                        samples
                    }
                    AudioBufferRef::F64(buf) => {
                        let mut samples = Vec::new();
                        for plane in buf.planes().planes() {
                            for &sample in plane.iter() {
                                samples.push((sample.clamp(-1.0, 1.0) * 32767.0) as i16);
                            }
                        }
                        samples
                    }
                    _ => continue,
                };
                all_samples.extend(samples_i16);
            }
            Err(Error::IoError(_)) | Err(Error::DecodeError(_)) => continue,
            Err(err) => return Err(format!("Error decoding audio: {}", err)),
        }
    }

    // Convert to mono if needed
    let mono_samples = if channels > 1 {
        // Average channels to create mono
        all_samples
            .chunks(channels)
            .map(|chunk| {
                let sum: i32 = chunk.iter().map(|&s| s as i32).sum();
                (sum / channels as i32) as i16
            })
            .collect()
    } else {
        all_samples
    };

    // Resample to 16kHz if needed
    let final_samples = if sample_rate != 16000 {
        use rubato::{
            Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType,
            WindowFunction,
        };

        // Convert i16 to f32 for resampling
        let samples_f32: Vec<f32> = mono_samples.iter().map(|&s| s as f32 / 32768.0).collect();

        // Setup resampler
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Cubic,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };

        let mut resampler = SincFixedIn::<f32>::new(
            16000.0 / sample_rate as f64,
            2.0,
            params,
            samples_f32.len(),
            1,
        )
        .map_err(|e| format!("Failed to create resampler: {}", e))?;

        // Resample
        let resampled = resampler
            .process(&[samples_f32], None)
            .map_err(|e| format!("Failed to resample: {}", e))?;

        // Convert back to i16
        resampled[0]
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect()
    } else {
        mono_samples
    };

    // Write WAV file using hound
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(&output_path, spec)
        .map_err(|e| format!("Failed to create WAV file: {}", e))?;

    for sample in final_samples {
        writer
            .write_sample(sample)
            .map_err(|e| format!("Failed to write sample: {}", e))?;
    }

    writer
        .finalize()
        .map_err(|e| format!("Failed to finalize WAV file: {}", e))?;

    Ok(output_path)
}
