use hound::{SampleFormat, WavSpec, WavWriter};
/// Test Data Helpers for Audio Testing
///
/// This module provides utilities for generating various types of test audio data
/// to thoroughly test audio validation, processing, and error handling.
use std::f32::consts::PI;
use tempfile::NamedTempFile;

/// Audio test data generator with various audio patterns
pub struct AudioTestDataGenerator {
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
}

impl Default for AudioTestDataGenerator {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            channels: 1,
            bits_per_sample: 16,
        }
    }
}

impl AudioTestDataGenerator {
    /// Create a new generator with custom parameters
    pub fn new(sample_rate: u32, channels: u16, bits_per_sample: u16) -> Self {
        Self {
            sample_rate,
            channels,
            bits_per_sample,
        }
    }

    /// Generate completely silent audio
    pub fn generate_silent_audio(&self, duration_seconds: f32) -> Vec<i16> {
        let sample_count = (self.sample_rate as f32 * duration_seconds) as usize;
        vec![0i16; sample_count * self.channels as usize]
    }

    /// Generate very quiet audio (just above silence threshold)
    pub fn generate_quiet_audio(&self, duration_seconds: f32, amplitude: f32) -> Vec<i16> {
        let sample_count = (self.sample_rate as f32 * duration_seconds) as usize;
        let max_amplitude = (amplitude * i16::MAX as f32) as i16;

        (0..sample_count)
            .flat_map(|i| {
                let sample = ((i as f32 * 0.01).sin() * max_amplitude as f32) as i16;
                vec![sample; self.channels as usize]
            })
            .collect()
    }

    /// Generate normal speech-like audio
    pub fn generate_speech_like_audio(&self, duration_seconds: f32) -> Vec<i16> {
        let sample_count = (self.sample_rate as f32 * duration_seconds) as usize;
        let base_frequency = 200.0; // Typical voice frequency

        (0..sample_count)
            .flat_map(|i| {
                let t = i as f32 / self.sample_rate as f32;
                let fundamental = (2.0 * PI * base_frequency * t).sin();
                let harmonic1 = 0.5 * (2.0 * PI * base_frequency * 2.0 * t).sin();
                let harmonic2 = 0.25 * (2.0 * PI * base_frequency * 3.0 * t).sin();

                // Add some amplitude modulation to simulate speech patterns
                let envelope = (2.0 * PI * 2.0 * t).sin().abs(); // 2 Hz modulation

                let sample = ((fundamental + harmonic1 + harmonic2) * envelope * 8000.0) as i16;
                vec![sample; self.channels as usize]
            })
            .collect()
    }

    /// Generate white noise
    pub fn generate_white_noise(&self, duration_seconds: f32, amplitude: f32) -> Vec<i16> {
        let sample_count = (self.sample_rate as f32 * duration_seconds) as usize;
        let max_amplitude = (amplitude * i16::MAX as f32) as i16;

        (0..sample_count)
            .flat_map(|_| {
                let sample = ((rand::random::<f32>() * 2.0 - 1.0) * max_amplitude as f32) as i16;
                vec![sample; self.channels as usize]
            })
            .collect()
    }

    /// Generate clipped/distorted audio
    pub fn generate_clipped_audio(&self, duration_seconds: f32) -> Vec<i16> {
        let sample_count = (self.sample_rate as f32 * duration_seconds) as usize;

        (0..sample_count)
            .flat_map(|i| {
                let sample = if (i / 100) % 2 == 0 {
                    i16::MAX // Clipped high
                } else {
                    i16::MIN // Clipped low
                };
                vec![sample; self.channels as usize]
            })
            .collect()
    }

    /// Generate audio with intermittent silence (realistic speech pattern)
    pub fn generate_intermittent_speech(&self, duration_seconds: f32) -> Vec<i16> {
        let sample_count = (self.sample_rate as f32 * duration_seconds) as usize;
        let speech_segment_size = self.sample_rate as usize; // 1 second segments

        (0..sample_count)
            .flat_map(|i| {
                let segment = i / speech_segment_size;
                let sample = if segment % 2 == 0 {
                    // Speech segments
                    let t = i as f32 / self.sample_rate as f32;
                    ((200.0 * 2.0 * PI * t).sin() * 8000.0) as i16
                } else {
                    // Silence segments
                    0
                };
                vec![sample; self.channels as usize]
            })
            .collect()
    }

    /// Generate a pure tone at specified frequency
    pub fn generate_pure_tone(
        &self,
        duration_seconds: f32,
        frequency_hz: f32,
        amplitude: f32,
    ) -> Vec<i16> {
        let sample_count = (self.sample_rate as f32 * duration_seconds) as usize;
        let max_amplitude = (amplitude * i16::MAX as f32) as i16;

        (0..sample_count)
            .flat_map(|i| {
                let t = i as f32 / self.sample_rate as f32;
                let sample = ((2.0 * PI * frequency_hz * t).sin() * max_amplitude as f32) as i16;
                vec![sample; self.channels as usize]
            })
            .collect()
    }

    /// Generate audio with varying amplitude (fade in/out)
    pub fn generate_fade_audio(&self, duration_seconds: f32, fade_type: FadeType) -> Vec<i16> {
        let sample_count = (self.sample_rate as f32 * duration_seconds) as usize;

        (0..sample_count)
            .flat_map(|i| {
                let t = i as f32 / sample_count as f32;
                let fade_factor = match fade_type {
                    FadeType::FadeIn => t,
                    FadeType::FadeOut => 1.0 - t,
                    FadeType::FadeInOut => {
                        if t < 0.5 {
                            2.0 * t
                        } else {
                            2.0 * (1.0 - t)
                        }
                    }
                };

                let base_sample = ((i as f32 * 0.01).sin() * 8000.0) as i16;
                let sample = (base_sample as f32 * fade_factor) as i16;
                vec![sample; self.channels as usize]
            })
            .collect()
    }

    /// Generate multi-frequency audio (complex waveform)
    pub fn generate_multi_frequency_audio(
        &self,
        duration_seconds: f32,
        frequencies: &[f32],
    ) -> Vec<i16> {
        let sample_count = (self.sample_rate as f32 * duration_seconds) as usize;

        (0..sample_count)
            .flat_map(|i| {
                let t = i as f32 / self.sample_rate as f32;
                let combined_sample: f32 = frequencies
                    .iter()
                    .map(|&freq| (2.0 * PI * freq * t).sin())
                    .sum::<f32>()
                    / frequencies.len() as f32;

                let sample = (combined_sample * 8000.0) as i16;
                vec![sample; self.channels as usize]
            })
            .collect()
    }

    /// Create a WAV file from sample data
    pub fn create_wav_file(
        &self,
        samples: Vec<i16>,
    ) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let temp_file = NamedTempFile::new()?;
        let spec = WavSpec {
            channels: self.channels,
            sample_rate: self.sample_rate,
            bits_per_sample: self.bits_per_sample,
            sample_format: SampleFormat::Int,
        };

        let mut writer = WavWriter::create(temp_file.path(), spec)?;
        for sample in samples {
            writer.write_sample(sample)?;
        }
        writer.finalize()?;

        Ok(temp_file)
    }

    /// Create a corrupted WAV file for testing error handling
    pub fn create_corrupted_wav_file(&self) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let temp_file = NamedTempFile::new()?;
        // Write invalid WAV data
        std::fs::write(
            temp_file.path(),
            b"RIFF invalid WAV data that should fail to parse",
        )?;
        Ok(temp_file)
    }

    /// Create an empty file
    pub fn create_empty_file(&self) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let temp_file = NamedTempFile::new()?;
        // File is empty by default
        Ok(temp_file)
    }
}

/// Fade types for audio generation
#[derive(Debug, Clone, Copy)]
pub enum FadeType {
    FadeIn,
    FadeOut,
    FadeInOut,
}

/// Predefined test audio scenarios for common testing patterns
pub struct TestAudioScenarios;

impl TestAudioScenarios {
    /// Get a generator with standard 16kHz mono settings
    pub fn standard_generator() -> AudioTestDataGenerator {
        AudioTestDataGenerator::default()
    }

    /// Create silent audio that should trigger "no speech detected"
    pub fn create_silent_recording(
        duration_seconds: f32,
    ) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let generator = Self::standard_generator();
        let samples = generator.generate_silent_audio(duration_seconds);
        generator.create_wav_file(samples)
    }

    /// Create quiet audio that should trigger "too quiet" warning
    pub fn create_quiet_recording(
        duration_seconds: f32,
    ) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let generator = Self::standard_generator();
        let samples = generator.generate_quiet_audio(duration_seconds, 0.03); // 3% amplitude
        generator.create_wav_file(samples)
    }

    /// Create valid speech-like audio that should pass validation
    pub fn create_valid_speech_recording(
        duration_seconds: f32,
    ) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let generator = Self::standard_generator();
        let samples = generator.generate_speech_like_audio(duration_seconds);
        generator.create_wav_file(samples)
    }

    /// Create too-short recording that should fail duration validation
    pub fn create_too_short_recording() -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let generator = Self::standard_generator();
        let samples = generator.generate_speech_like_audio(0.3); // 300ms - below 500ms threshold
        generator.create_wav_file(samples)
    }

    /// Create very long recording for performance testing
    pub fn create_long_recording(
        duration_seconds: f32,
    ) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let generator = Self::standard_generator();
        let samples = generator.generate_speech_like_audio(duration_seconds);
        generator.create_wav_file(samples)
    }

    /// Create clipped/distorted audio for robustness testing
    pub fn create_clipped_recording(
        duration_seconds: f32,
    ) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let generator = Self::standard_generator();
        let samples = generator.generate_clipped_audio(duration_seconds);
        generator.create_wav_file(samples)
    }

    /// Create intermittent speech (realistic pattern)
    pub fn create_realistic_speech_recording(
        duration_seconds: f32,
    ) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let generator = Self::standard_generator();
        let samples = generator.generate_intermittent_speech(duration_seconds);
        generator.create_wav_file(samples)
    }

    /// Create stereo audio for multi-channel testing
    pub fn create_stereo_recording(
        duration_seconds: f32,
    ) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let generator = AudioTestDataGenerator::new(16000, 2, 16); // Stereo
        let samples = generator.generate_speech_like_audio(duration_seconds);
        generator.create_wav_file(samples)
    }

    /// Create high sample rate audio
    pub fn create_high_quality_recording(
        duration_seconds: f32,
    ) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let generator = AudioTestDataGenerator::new(48000, 1, 16); // 48kHz
        let samples = generator.generate_speech_like_audio(duration_seconds);
        generator.create_wav_file(samples)
    }

    /// Create corrupted file for error handling testing
    pub fn create_corrupted_recording() -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let generator = Self::standard_generator();
        generator.create_corrupted_wav_file()
    }
}

/// Batch test data generator for creating multiple test files
pub struct BatchTestDataGenerator {
    base_generator: AudioTestDataGenerator,
}

impl BatchTestDataGenerator {
    pub fn new() -> Self {
        Self {
            base_generator: AudioTestDataGenerator::default(),
        }
    }

    /// Generate a comprehensive test suite of audio files
    pub fn generate_test_suite(
        &self,
    ) -> Result<Vec<(String, NamedTempFile)>, Box<dyn std::error::Error>> {
        let mut test_files = Vec::new();

        // Basic validation tests
        test_files.push((
            "silent_1s".to_string(),
            TestAudioScenarios::create_silent_recording(1.0)?,
        ));
        test_files.push((
            "quiet_1s".to_string(),
            TestAudioScenarios::create_quiet_recording(1.0)?,
        ));
        test_files.push((
            "valid_1s".to_string(),
            TestAudioScenarios::create_valid_speech_recording(1.0)?,
        ));
        test_files.push((
            "too_short".to_string(),
            TestAudioScenarios::create_too_short_recording()?,
        ));

        // Edge cases
        test_files.push((
            "clipped_1s".to_string(),
            TestAudioScenarios::create_clipped_recording(1.0)?,
        ));
        test_files.push((
            "realistic_3s".to_string(),
            TestAudioScenarios::create_realistic_speech_recording(3.0)?,
        ));
        test_files.push((
            "stereo_1s".to_string(),
            TestAudioScenarios::create_stereo_recording(1.0)?,
        ));

        // Performance tests
        test_files.push((
            "long_10s".to_string(),
            TestAudioScenarios::create_long_recording(10.0)?,
        ));
        test_files.push((
            "high_quality_1s".to_string(),
            TestAudioScenarios::create_high_quality_recording(1.0)?,
        ));

        // Error handling tests
        test_files.push((
            "corrupted".to_string(),
            TestAudioScenarios::create_corrupted_recording()?,
        ));

        Ok(test_files)
    }

    /// Generate specific test cases for audio validation
    pub fn generate_validation_test_cases(
        &self,
    ) -> Result<Vec<(String, NamedTempFile, &'static str)>, Box<dyn std::error::Error>> {
        let mut test_cases = Vec::new();

        test_cases.push((
            "silent".to_string(),
            TestAudioScenarios::create_silent_recording(1.0)?,
            "should_be_silent",
        ));
        test_cases.push((
            "too_quiet".to_string(),
            TestAudioScenarios::create_quiet_recording(1.0)?,
            "should_be_too_quiet",
        ));
        test_cases.push((
            "too_short".to_string(),
            TestAudioScenarios::create_too_short_recording()?,
            "should_be_too_short",
        ));
        test_cases.push((
            "valid".to_string(),
            TestAudioScenarios::create_valid_speech_recording(1.0)?,
            "should_be_valid",
        ));

        Ok(test_cases)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_generator_silent() {
        let generator = AudioTestDataGenerator::default();
        let samples = generator.generate_silent_audio(1.0);

        assert_eq!(samples.len(), 16000); // 1 second at 16kHz
        assert!(samples.iter().all(|&s| s == 0));
    }

    #[test]
    fn test_audio_generator_quiet() {
        let generator = AudioTestDataGenerator::default();
        let samples = generator.generate_quiet_audio(1.0, 0.1);

        assert_eq!(samples.len(), 16000);

        // Should have some non-zero samples but low amplitude
        let max_amplitude = samples.iter().map(|&s| s.abs()).max().unwrap();
        assert!(max_amplitude > 0);
        assert!(max_amplitude < 5000); // Should be quiet
    }

    #[test]
    fn test_audio_generator_speech_like() {
        let generator = AudioTestDataGenerator::default();
        let samples = generator.generate_speech_like_audio(1.0);

        assert_eq!(samples.len(), 16000);

        // Should have reasonable amplitude for speech
        let max_amplitude = samples.iter().map(|&s| s.abs()).max().unwrap();
        assert!(max_amplitude > 1000);
        assert!(max_amplitude < 32000);
    }

    #[test]
    fn test_wav_file_creation() {
        let generator = AudioTestDataGenerator::default();
        let samples = generator.generate_speech_like_audio(0.5);

        let wav_file = generator.create_wav_file(samples).unwrap();

        // Verify file exists and has content
        let metadata = std::fs::metadata(wav_file.path()).unwrap();
        assert!(metadata.len() > 0);
    }

    #[test]
    fn test_test_scenarios() {
        // Test that all scenario methods work
        let _silent = TestAudioScenarios::create_silent_recording(1.0).unwrap();
        let _quiet = TestAudioScenarios::create_quiet_recording(1.0).unwrap();
        let _valid = TestAudioScenarios::create_valid_speech_recording(1.0).unwrap();
        let _short = TestAudioScenarios::create_too_short_recording().unwrap();
        let _corrupted = TestAudioScenarios::create_corrupted_recording().unwrap();
    }

    #[test]
    fn test_batch_generator() {
        let batch_generator = BatchTestDataGenerator::new();
        let test_suite = batch_generator.generate_test_suite().unwrap();

        assert!(test_suite.len() >= 10); // Should have multiple test files

        // Verify all files exist
        for (name, file) in &test_suite {
            let metadata = std::fs::metadata(file.path()).unwrap();
            assert!(metadata.len() > 0, "File {} should have content", name);
        }
    }

    #[test]
    fn test_validation_test_cases() {
        let batch_generator = BatchTestDataGenerator::new();
        let validation_cases = batch_generator.generate_validation_test_cases().unwrap();

        assert_eq!(validation_cases.len(), 4);

        for (name, file, expected_result) in &validation_cases {
            let metadata = std::fs::metadata(file.path()).unwrap();
            assert!(metadata.len() > 0, "File {} should have content", name);
            assert!(!expected_result.is_empty());
        }
    }

    #[test]
    fn test_different_sample_rates() {
        let rates = [8000, 16000, 22050, 44100, 48000];

        for &rate in &rates {
            let generator = AudioTestDataGenerator::new(rate, 1, 16);
            let samples = generator.generate_speech_like_audio(1.0);

            assert_eq!(samples.len(), rate as usize);

            let wav_file = generator.create_wav_file(samples).unwrap();
            let metadata = std::fs::metadata(wav_file.path()).unwrap();
            assert!(metadata.len() > 0);
        }
    }

    #[test]
    fn test_stereo_generation() {
        let generator = AudioTestDataGenerator::new(16000, 2, 16);
        let samples = generator.generate_speech_like_audio(1.0);

        // Stereo should have double the samples (2 channels)
        assert_eq!(samples.len(), 32000);

        let wav_file = generator.create_wav_file(samples).unwrap();
        let metadata = std::fs::metadata(wav_file.path()).unwrap();
        assert!(metadata.len() > 0);
    }
}
