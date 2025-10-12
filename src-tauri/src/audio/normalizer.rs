use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use rand::Rng;
use crate::audio::resampler::resample_to_16khz;
use std::fs;
use std::path::{Path, PathBuf};

const TARGET_RATE: u32 = 16_000;
const TARGET_CHANNELS: u16 = 1;
const TARGET_BITS: u16 = 16;
const TARGET_PEAK: f32 = 0.8; // ~ -1.9 dBFS
const SILENCE_RMS_THRESHOLD: f32 = 1e-4; // ~ -80 dBFS

/// Normalize any WAV (our recorder output) to Whisper contract:
/// WAV PCM S16LE, mono, 16 kHz, peak-normalized with light dither.
pub fn normalize_to_whisper_wav(input_wav: &Path, out_dir: &Path) -> Result<PathBuf, String> {
    if !input_wav.exists() {
        return Err(format!("Input WAV does not exist: {:?}", input_wav));
    }

    fs::create_dir_all(out_dir).map_err(|e| format!("Failed to create out_dir: {}", e))?;

    // Open source wav (expect our recorder: PCM 16-bit interleaved)
    let mut reader = WavReader::open(input_wav)
        .map_err(|e| format!("Failed to open WAV: {}", e))?;
    let spec = reader.spec();

    if spec.sample_format != SampleFormat::Int || spec.bits_per_sample != 16 {
        // For now we handle our own 16-bit files; other formats can be extended later.
        // Avoid surprising runtime errors by surfacing a clear message.
        log::warn!(
            "Normalizer expected 16-bit PCM. Got {:?} {}-bit; proceeding best-effort.",
            spec.sample_format, spec.bits_per_sample
        );
    }

    let channels = spec.channels.max(1);
    let sample_rate = spec.sample_rate.max(1);

    // Read samples as i16 → f32 [-1,1]
    let samples_i16: Vec<i16> = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read samples: {}", e))?;
    if samples_i16.is_empty() {
        return Err("WAV contains no samples".to_string());
    }

    let mut samples_f32: Vec<f32> = samples_i16
        .iter()
        .map(|&s| s as f32 / i16::MAX as f32)
        .collect();

    // If multi-channel, compute per-channel RMS and ignore near-silent channels.
    let mono: Vec<f32> = if channels == 1 {
        samples_f32
    } else {
        downmix_equal_power_ignore_silent(&samples_f32, channels as usize)
    };

    // Resample to 16 kHz using our high-quality rubato resampler
    let resampled = if sample_rate != TARGET_RATE {
        resample_to_16khz(&mono, sample_rate)?
    } else {
        mono
    };

    // Peak normalize to TARGET_PEAK with a soft clamp
    let peak = resampled
        .iter()
        .fold(0.0f32, |m, &x| m.max(x.abs()));
    let gain = if peak > 0.0 { (TARGET_PEAK / peak).min(10.0) } else { 1.0 };
    let mut normalized: Vec<f32> = if (gain - 1.0).abs() > 1e-3 {
        resampled.iter().map(|&x| (x * gain).clamp(-1.0, 1.0)).collect()
    } else {
        resampled
    };

    // Quantize to i16 with TPDF dither
    let mut rng = rand::thread_rng();
    let mut pcm_i16 = Vec::with_capacity(normalized.len());
    for &x in &normalized {
        // TPDF dither: add two independent uniform(-0.5,0.5) LSBs
        let dither = (rng.gen::<f32>() - 0.5) + (rng.gen::<f32>() - 0.5);
        let y = (x * i16::MAX as f32 + dither).clamp(i16::MIN as f32, i16::MAX as f32);
        pcm_i16.push(y as i16);
    }

    // Write final WAV
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let out_path = out_dir.join(format!("normalized_{}.wav", ts));
    let out_spec = WavSpec {
        channels: TARGET_CHANNELS,
        sample_rate: TARGET_RATE,
        bits_per_sample: TARGET_BITS,
        sample_format: SampleFormat::Int,
    };
    let mut writer =
        WavWriter::create(&out_path, out_spec).map_err(|e| format!("WAV create failed: {}", e))?;
    for s in pcm_i16 {
        writer
            .write_sample(s)
            .map_err(|e| format!("WAV write failed: {}", e))?;
    }
    writer.finalize().map_err(|e| format!("WAV finalize failed: {}", e))?;

    Ok(out_path)
}

fn downmix_equal_power_ignore_silent(input: &[f32], channels: usize) -> Vec<f32> {
    if channels == 0 {
        return vec![];
    }
    let frames = input.len() / channels;
    if frames == 0 {
        return vec![];
    }

    // RMS per channel
    let mut sumsq = vec![0.0f32; channels];
    for frame in 0..frames {
        let base = frame * channels;
        for ch in 0..channels {
            let s = input[base + ch];
            sumsq[ch] += s * s;
        }
    }
    let rms: Vec<f32> = sumsq
        .iter()
        .map(|&s| (s / frames as f32).sqrt())
        .collect();
    let mut active: Vec<usize> = rms
        .iter()
        .enumerate()
        .filter(|(_, &e)| e > SILENCE_RMS_THRESHOLD)
        .map(|(i, _)| i)
        .collect();
    if active.is_empty() {
        // If all channels are silent by threshold, use all channels to avoid empty output
        active = (0..channels).collect();
    }

    let gain = (1.0f32 / (active.len() as f32)).sqrt();

    let mut out = Vec::with_capacity(frames);
    for frame in 0..frames {
        let base = frame * channels;
        let mut sum = 0.0f32;
        for &ch in &active {
            sum += input[base + ch];
        }
        out.push((sum * gain).clamp(-1.0, 1.0));
    }
    out
}
