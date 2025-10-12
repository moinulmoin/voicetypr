use super::normalizer::normalize_to_whisper_wav;
use hound::{SampleFormat, WavSpec, WavWriter};
use std::f32::consts::PI;
use std::fs;
use std::path::{Path, PathBuf};

fn temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("voicetypr_test_{}", name))
}

fn write_sine_wav(path: &Path, sample_rate: u32, channels: u16, secs: f32, amp: f32, freq: f32, silent_channels: &[u16]) {
    let spec = WavSpec { channels, sample_rate, bits_per_sample: 16, sample_format: SampleFormat::Int };
    let mut writer = WavWriter::create(path, spec).expect("create wav");
    let total_frames = (secs * sample_rate as f32) as usize;

    for n in 0..total_frames {
        let t = n as f32 / sample_rate as f32;
        let sample = (amp * (2.0 * PI * freq * t).sin()).clamp(-1.0, 1.0);
        for ch in 0..channels {
            let s = if silent_channels.contains(&ch) { 0.0 } else { sample };
            let i = (s * 32767.0) as i16;
            writer.write_sample(i).expect("write sample");
        }
    }
    writer.finalize().expect("finalize wav");
}

#[test]
fn normalize_fails_on_missing_input() {
    let missing = temp_file("missing_input.wav");
    // Ensure it does not exist
    let _ = fs::remove_file(&missing);
    let out_dir = std::env::temp_dir();
    let err = normalize_to_whisper_wav(&missing, &out_dir).unwrap_err();
    assert!(err.contains("Input WAV does not exist"));
}

#[test]
fn normalize_basic_16k_mono_peak_and_format() {
    let input = temp_file("basic_16k_mono_in.wav");
    let out_dir = temp_file("basic_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    // 0.25s of 1kHz sine at 0.5 amplitude
    write_sine_wav(&input, 16_000, 1, 0.25, 0.5, 1000.0, &[]);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");

    // Read back and validate format
    let mut reader = hound::WavReader::open(&out_path).expect("open normalized");
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16_000);
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.bits_per_sample, 16);
    assert_eq!(spec.sample_format, SampleFormat::Int);

    // Check approximate peak around target (0.8) with tolerance due to dither/quantization
    let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
    assert!(!samples.is_empty());
    let max = samples.iter().map(|s| s.abs() as i32).max().unwrap() as f32;
    let peak = max / i16::MAX as f32;
    // Allow generous tolerance (±0.1)
    assert!(peak > 0.65 && peak <= 0.85, "peak out of expected range: {}", peak);

    // Cleanup
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn normalize_resamples_48k_to_16k() {
    let input = temp_file("resample_48k_in.wav");
    let out_dir = temp_file("resample_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    // 0.3s at 48kHz, mono
    write_sine_wav(&input, 48_000, 1, 0.3, 0.4, 800.0, &[]);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");

    let reader = hound::WavReader::open(&out_path).expect("open normalized");
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16_000);
    assert_eq!(spec.channels, 1);

    // Duration should be roughly preserved (0.3s)
    let frames = reader.duration() / spec.channels as u32;
    let duration = frames as f32 / spec.sample_rate as f32;
    assert!((duration - 0.3).abs() < 0.05, "duration {}s not ~0.3s", duration);

    // Cleanup
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn normalize_downmix_ignores_silent_channel() {
    let input = temp_file("downmix_stereo_in.wav");
    let out_dir = temp_file("downmix_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    // Stereo: ch0 is 0.5 sine, ch1 is silent
    write_sine_wav(&input, 16_000, 2, 0.25, 0.5, 500.0, &[1]);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");

    // Ensure output is mono 16k and non-silent
    let samples: Vec<i16> = hound::WavReader::open(&out_path)
        .expect("open out")
        .samples::<i16>()
        .map(|s| s.unwrap())
        .collect();
    assert!(!samples.is_empty());
    let max = samples.iter().map(|s| s.abs() as i32).max().unwrap();
    assert!(max > 0, "output should not be silent");

    // Cleanup
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}
