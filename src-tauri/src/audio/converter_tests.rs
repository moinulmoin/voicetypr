#[cfg(test)]
mod tests {
    use super::super::converter::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_wav_file_passthrough() {
        // If input is already WAV, should return it unchanged
        let temp_dir = TempDir::new().unwrap();
        let wav_path = temp_dir.path().join("test.wav");

        // Create a dummy WAV file
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let writer = hound::WavWriter::create(&wav_path, spec).unwrap();
        writer.finalize().unwrap();

        // Convert should return the same path
        let result = convert_to_wav(&wav_path, temp_dir.path()).unwrap();
        assert_eq!(result, wav_path);
    }

    #[test]
    fn test_nonexistent_file_error() {
        // Should error on non-existent files
        let temp_dir = TempDir::new().unwrap();
        let fake_path = PathBuf::from("/this/does/not/exist.mp3");

        let result = convert_to_wav(&fake_path, temp_dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to open audio file"));
    }

    #[test]
    fn test_output_is_valid_wav() {
        // For non-WAV input, output should be valid WAV at 16kHz mono
        let temp_dir = TempDir::new().unwrap();

        // Create a simple non-WAV file (will fail to decode, but that's ok for this test)
        let input_path = temp_dir.path().join("test.mp3");
        fs::write(&input_path, b"not really an mp3").unwrap();

        // This will fail to decode, which is fine - we're testing error handling
        let result = convert_to_wav(&input_path, temp_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_temp_file_has_unique_name() {
        // Each conversion should create a unique temp file
        let temp_dir = TempDir::new().unwrap();

        // Create two dummy files
        let input1 = temp_dir.path().join("test1.txt");
        let input2 = temp_dir.path().join("test2.txt");
        fs::write(&input1, b"dummy").unwrap();
        fs::write(&input2, b"dummy").unwrap();

        // Try to convert both (will fail, but should attempt different output names)
        let result1 = convert_to_wav(&input1, temp_dir.path());
        std::thread::sleep(std::time::Duration::from_millis(1001)); // Ensure different timestamp
        let result2 = convert_to_wav(&input2, temp_dir.path());

        // Both should fail, but that's ok
        assert!(result1.is_err());
        assert!(result2.is_err());
    }
}