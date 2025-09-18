#[cfg(test)]
mod tests {
    use crate::commands::audio::{transcribe_audio_file, read_audio_file};
    use crate::tests::test_helpers::create_test_app;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_transcribe_audio_file_with_nonexistent_file() {
        let app = create_test_app().await;
        let fake_path = "/this/does/not/exist.m4a".to_string();

        let result = transcribe_audio_file(app, fake_path, "base.en".to_string()).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Audio file not found"));
    }

    #[tokio::test]
    async fn test_transcribe_audio_file_creates_temp_wav() {
        let app = create_test_app().await;
        let temp_dir = TempDir::new().unwrap();

        // Create a dummy M4A file (won't be valid audio, but tests the path)
        let m4a_path = temp_dir.path().join("test.m4a");
        fs::write(&m4a_path, b"dummy m4a content").unwrap();

        // This will fail at the audio decoding stage, but that's ok
        // We're testing that it attempts conversion for non-WAV files
        let result = transcribe_audio_file(
            app,
            m4a_path.to_str().unwrap().to_string(),
            "base.en".to_string()
        ).await;

        // Should fail but with audio format error, not file not found
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(!error.contains("Audio file not found"));
    }

    #[tokio::test]
    async fn test_transcribe_audio_file_handles_wav_directly() {
        let app = create_test_app().await;
        let temp_dir = TempDir::new().unwrap();

        // Create a valid WAV file
        let wav_path = temp_dir.path().join("test.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = hound::WavWriter::create(&wav_path, spec).unwrap();
        // Write some silence
        for _ in 0..16000 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();

        // This should attempt to transcribe (may fail if no model, but tests the path)
        let result = transcribe_audio_file(
            app,
            wav_path.to_str().unwrap().to_string(),
            "base.en".to_string()
        ).await;

        // May fail due to missing model or license, but shouldn't be file error
        if result.is_err() {
            let error = result.unwrap_err();
            assert!(!error.contains("Audio file not found"));
            assert!(!error.contains("Failed to probe audio format"));
        }
    }

    #[tokio::test]
    async fn test_read_audio_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.audio");
        let test_content = b"test audio data";
        fs::write(&test_file, test_content).unwrap();

        let result = read_audio_file(test_file.to_str().unwrap().to_string()).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), test_content.to_vec());
    }

    #[tokio::test]
    async fn test_read_audio_file_not_exists() {
        let result = read_audio_file("/nonexistent/file.wav".to_string()).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to read audio file"));
    }
}