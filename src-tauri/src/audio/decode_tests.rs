#[cfg(test)]
mod tests {
    use crate::audio::decode::normalize_to_wav;
    use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Write a synthetic stereo 48 kHz, 16-bit sine WAV of the given duration.
    fn write_synthetic_stereo_48k(path: &std::path::Path, duration_secs: f64) {
        let spec = WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut writer = WavWriter::create(path, spec).unwrap();
        let n = (duration_secs * 48_000.0) as usize;
        for i in 0..n {
            let t = i as f32 / 48_000.0;
            let v = (t * 2.0 * std::f32::consts::PI * 440.0).sin() * 0.5;
            let s = (v * 32_767.0) as i16;
            writer.write_sample(s).unwrap();
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn normalizes_stereo_48k_to_mono_16k_s16() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("in.wav");
        let output = dir.path().join("out.wav");

        write_synthetic_stereo_48k(&input, 0.5);
        normalize_to_wav(&input, &output).expect("normalize should succeed");

        let mut reader = WavReader::open(&output).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, 1, "must be mono");
        assert_eq!(spec.sample_rate, 16_000, "must be 16 kHz");
        assert_eq!(spec.bits_per_sample, 16, "must be 16-bit");
        assert_eq!(spec.sample_format, SampleFormat::Int);

        let count = reader.samples::<i16>().filter_map(Result::ok).count();
        let expected = 0.5_f64 * 16_000.0;
        let lo = (expected * 0.95) as usize;
        let hi = (expected * 1.05) as usize;
        assert!(
            (lo..=hi).contains(&count),
            "expected ~{expected} samples (+/- 5%), got {count}"
        );
    }

    #[test]
    fn passthrough_canonical_wav_byte_copies() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("canonical.wav");
        let output = dir.path().join("out.wav");

        let spec = WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut writer = WavWriter::create(&input, spec).unwrap();
        for i in 0..1_600 {
            let s = ((i % 256) as i16) - 128;
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();

        normalize_to_wav(&input, &output).expect("passthrough should succeed");

        let in_bytes = std::fs::read(&input).unwrap();
        let out_bytes = std::fs::read(&output).unwrap();
        assert_eq!(in_bytes, out_bytes, "passthrough must byte-copy");
    }

    #[test]
    fn text_file_returns_err_not_panic() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("not_audio.txt");
        let output = dir.path().join("out.wav");
        std::fs::write(&input, b"this is text, not audio").unwrap();

        let result = normalize_to_wav(&input, &output);
        assert!(result.is_err(), "text file must error, not panic");
    }

    #[test]
    fn truncated_wav_returns_err_not_panic() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("trunc.wav");
        let output = dir.path().join("out.wav");

        // Looks vaguely like a WAV header but is bogus and too short to parse.
        std::fs::write(&input, b"RIFFxxxxWAVEfmt ").unwrap();

        let result = normalize_to_wav(&input, &output);
        assert!(result.is_err(), "truncated WAV must error, not panic");
    }

    // --- Machine-local integration tests against staged real-world fixtures ---
    // Run with: cargo test -- --ignored
    // These are ignored because they read files from a scratchpad path that
    // only exists on the development machine.

    // Fixture dir from the VOICETYPR_TEST_AUDIO_DIR env var. Unset in CI, so the
    // #[ignore]d tests below don't run there; set it to a dir of real-world audio
    // files to run them locally: `VOICETYPR_TEST_AUDIO_DIR=/path cargo test -- --ignored`.
    fn realfiles_dir() -> PathBuf {
        PathBuf::from(std::env::var("VOICETYPR_TEST_AUDIO_DIR").unwrap_or_default())
    }

    fn run_realfile(name: &str) -> Result<(), String> {
        let input = realfiles_dir().join(name);
        let out_dir = TempDir::new().unwrap();
        let output = out_dir.path().join("out.wav");
        normalize_to_wav(&input, &output)
    }

    #[test]
    #[ignore = "reads machine-local realfiles fixtures"]
    fn real_tiktok_mp4_decodes() {
        let r = run_realfile("real_tiktok.mp4");
        assert!(r.is_ok(), "real_tiktok.mp4 failed: {:?}", r.err());
    }

    #[test]
    #[ignore = "reads machine-local realfiles fixtures"]
    fn real_pitch_mp4_decodes() {
        let r = run_realfile("real_pitch.mp4");
        assert!(r.is_ok(), "real_pitch.mp4 failed: {:?}", r.err());
    }

    #[test]
    #[ignore = "reads machine-local realfiles fixtures"]
    fn real_whatsapp_mp3_decodes() {
        let r = run_realfile("real_whatsapp.mp3");
        assert!(r.is_ok(), "real_whatsapp.mp3 failed: {:?}", r.err());
    }

    #[test]
    #[ignore = "reads machine-local realfiles fixtures"]
    fn gen_aac_mkv_decodes() {
        let r = run_realfile("gen_aac.mkv");
        assert!(r.is_ok(), "gen_aac.mkv failed: {:?}", r.err());
    }

    #[test]
    #[ignore = "reads machine-local realfiles fixtures"]
    fn gen_opus_webm_returns_unsupported() {
        let r = run_realfile("gen_opus.webm");
        assert!(r.is_err());
        assert!(
            r.unwrap_err().contains("opus not yet supported"),
            "opus files must report Phase 2 not-yet-supported"
        );
    }

    #[test]
    #[ignore = "reads machine-local realfiles fixtures"]
    fn gen_opus_mkv_returns_unsupported() {
        let r = run_realfile("gen_opus.mkv");
        assert!(r.is_err());
        assert!(
            r.unwrap_err().contains("opus not yet supported"),
            "opus files must report Phase 2 not-yet-supported"
        );
    }

    #[test]
    #[ignore = "reads machine-local realfiles fixtures"]
    fn real_demo_mp4_returns_no_supported_audio_track() {
        let r = run_realfile("real_demo.mp4");
        assert!(r.is_err());
        let msg = r.unwrap_err();
        assert!(
            msg.contains("no supported audio track") || msg.contains("audio decode failed"),
            "unexpected message: {msg}"
        );
    }
}
