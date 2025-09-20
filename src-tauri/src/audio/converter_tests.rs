#[cfg(test)]
mod tests {
    use super::super::converter::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_wav_file_passthrough() {
        // ACTUALLY USEFUL: Ensures WAV files aren't unnecessarily re-encoded
        let temp_dir = TempDir::new().unwrap();
        let wav_path = temp_dir.path().join("test.wav");

        // Create a valid WAV file
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let writer = hound::WavWriter::create(&wav_path, spec).unwrap();
        writer.finalize().unwrap();

        // Should return the same path without conversion
        let result = convert_to_wav(&wav_path, temp_dir.path()).unwrap();
        assert_eq!(result, wav_path);
    }

    #[test]
    fn test_nonexistent_file_error() {
        // ACTUALLY USEFUL: Ensures proper error for missing files
        let temp_dir = TempDir::new().unwrap();
        let fake_path = temp_dir.path().join("/this/does/not/exist.mp3");

        let result = convert_to_wav(&fake_path, temp_dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to open audio file"));
    }

    #[test]
    fn test_invalid_audio_format_error() {
        // ACTUALLY USEFUL: Ensures graceful handling of non-audio files
        let temp_dir = TempDir::new().unwrap();
        let text_file = temp_dir.path().join("not_audio.txt");
        fs::write(&text_file, b"This is text, not audio").unwrap();

        let result = convert_to_wav(&text_file, temp_dir.path());
        assert!(result.is_err());
        // Should fail at format probing, not file opening
        assert!(!result.unwrap_err().contains("Failed to open audio file"));
    }
}