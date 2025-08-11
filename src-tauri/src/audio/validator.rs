use std::path::Path;
use hound::WavReader;

/// Result of audio validation analysis
#[derive(Debug, Clone)]
pub enum AudioValidationResult {
    /// Audio is valid for transcription
    Valid { 
        energy: f32, 
        duration: f32, 
        peak: f32,
        sample_count: usize,
    },
    /// Audio is completely silent (no detectable signal)
    Silent,
    /// Audio has signal but is too quiet for reliable transcription
    TooQuiet { 
        energy: f32, 
        suggestion: String,
    },
    /// Audio duration is too short for meaningful transcription
    TooShort { 
        duration: f32,
    },
    /// Invalid audio format or file corruption
    InvalidFormat(String),
}

/// Validation thresholds for audio analysis
pub struct ValidationThresholds {
    /// Minimum RMS energy required (0.01 = 1%)
    pub min_energy: f32,
    /// Minimum peak amplitude required (0.05 = 5%)
    pub min_peak: f32,
    /// Minimum duration in seconds (0.5s)
    pub min_duration: f32,
    /// Threshold below which audio is considered completely silent (0.001 = 0.1%)
    pub silence_threshold: f32,
}

impl Default for ValidationThresholds {
    fn default() -> Self {
        Self {
            min_energy: 0.01,      // 1% RMS energy
            min_peak: 0.05,        // 5% peak amplitude
            min_duration: 0.5,     // 0.5 seconds
            silence_threshold: 0.001, // 0.1% - below this is considered silence
        }
    }
}

/// Audio validator for pre-transcription quality checks
pub struct AudioValidator {
    thresholds: ValidationThresholds,
}

impl AudioValidator {
    /// Create new audio validator with default thresholds
    pub fn new() -> Self {
        Self {
            thresholds: ValidationThresholds::default(),
        }
    }

    /// Create audio validator with custom thresholds
    pub fn with_thresholds(thresholds: ValidationThresholds) -> Self {
        Self { thresholds }
    }

    /// Validate audio file for transcription readiness
    /// 
    /// Performs fast analysis of audio quality without loading entire file into memory.
    /// Returns validation result with detailed analysis.
    pub fn validate_audio_file<P: AsRef<Path>>(&self, path: P) -> Result<AudioValidationResult, String> {
        let path = path.as_ref();
        
        // Open WAV reader
        let mut reader = WavReader::open(path)
            .map_err(|e| format!("Failed to open audio file: {}", e))?;

        let spec = reader.spec();
        
        // Calculate duration from WAV header
        let sample_count = reader.len() as usize;
        let duration = sample_count as f32 / (spec.sample_rate as f32 * spec.channels as u16 as f32);

        log::debug!("Audio analysis: {}Hz, {} channels, {} samples, {:.2}s duration", 
                   spec.sample_rate, spec.channels, sample_count, duration);

        // Check minimum duration first (fastest check)
        if duration < self.thresholds.min_duration {
            return Ok(AudioValidationResult::TooShort { duration });
        }

        // Analyze audio samples for energy and peak detection
        let analysis = self.analyze_audio_samples(&mut reader)?;

        // Validate against thresholds
        self.validate_analysis(analysis, duration, sample_count)
    }

    /// Analyze audio samples for RMS energy and peak amplitude
    fn analyze_audio_samples(&self, reader: &mut WavReader<std::io::BufReader<std::fs::File>>) 
        -> Result<AudioAnalysis, String> 
    {
        let spec = reader.spec();
        let mut sum_squares = 0.0f64;
        let mut peak_amplitude = 0.0f32;
        let mut sample_count = 0usize;

        // Process samples based on bit depth
        match spec.bits_per_sample {
            16 => {
                for sample_result in reader.samples::<i16>() {
                    let sample = sample_result
                        .map_err(|e| format!("Failed to read sample: {}", e))?;
                    
                    // Convert to normalized float [-1.0, 1.0]
                    let normalized = sample as f32 / i16::MAX as f32;
                    let abs_sample = normalized.abs();
                    
                    // Update statistics
                    sum_squares += (normalized as f64).powi(2);
                    peak_amplitude = peak_amplitude.max(abs_sample);
                    sample_count += 1;
                }
            }
            32 => {
                for sample_result in reader.samples::<i32>() {
                    let sample = sample_result
                        .map_err(|e| format!("Failed to read sample: {}", e))?;
                    
                    // Convert to normalized float [-1.0, 1.0]
                    let normalized = sample as f32 / i32::MAX as f32;
                    let abs_sample = normalized.abs();
                    
                    // Update statistics
                    sum_squares += (normalized as f64).powi(2);
                    peak_amplitude = peak_amplitude.max(abs_sample);
                    sample_count += 1;
                }
            }
            _ => {
                return Err(format!("Unsupported bit depth: {} bits", spec.bits_per_sample));
            }
        }

        if sample_count == 0 {
            return Ok(AudioAnalysis {
                rms_energy: 0.0,
                peak_amplitude: 0.0,
                sample_count: 0,
            });
        }

        // Calculate RMS (Root Mean Square) energy
        let rms_energy = (sum_squares / sample_count as f64).sqrt() as f32;

        log::debug!("Audio analysis: RMS={:.6}, Peak={:.6}, Samples={}", 
                   rms_energy, peak_amplitude, sample_count);

        Ok(AudioAnalysis {
            rms_energy,
            peak_amplitude,
            sample_count,
        })
    }

    /// Validate analysis results against thresholds
    fn validate_analysis(&self, analysis: AudioAnalysis, duration: f32, total_samples: usize) 
        -> Result<AudioValidationResult, String> 
    {
        let AudioAnalysis { rms_energy, peak_amplitude, sample_count: _ } = analysis;

        // Check for complete silence (no signal at all)
        if rms_energy < self.thresholds.silence_threshold && peak_amplitude < self.thresholds.silence_threshold {
            log::info!("Audio validation: Silent audio detected (RMS={:.6}, Peak={:.6})", 
                      rms_energy, peak_amplitude);
            return Ok(AudioValidationResult::Silent);
        }

        // Check if audio is too quiet for reliable transcription
        if rms_energy < self.thresholds.min_energy || peak_amplitude < self.thresholds.min_peak {
            let suggestion = if peak_amplitude < self.thresholds.min_peak {
                format!("Audio is too quiet (peak: {:.1}%). Try speaking louder or closer to the microphone.", 
                       peak_amplitude * 100.0)
            } else {
                format!("Audio energy is low (RMS: {:.2}%). Check microphone sensitivity or recording environment.", 
                       rms_energy * 100.0)
            };

            log::info!("Audio validation: Too quiet (RMS={:.6}, Peak={:.6})", 
                      rms_energy, peak_amplitude);
            
            return Ok(AudioValidationResult::TooQuiet { 
                energy: rms_energy, 
                suggestion 
            });
        }

        // Audio passes all validation checks
        log::info!("Audio validation: PASSED (RMS={:.6}, Peak={:.6}, Duration={:.2}s)", 
                  rms_energy, peak_amplitude, duration);

        Ok(AudioValidationResult::Valid {
            energy: rms_energy,
            duration,
            peak: peak_amplitude,
            sample_count: total_samples,
        })
    }
}

/// Internal struct for audio analysis results
#[derive(Debug)]
struct AudioAnalysis {
    rms_energy: f32,
    peak_amplitude: f32,
    sample_count: usize,
}

impl Default for AudioValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use hound::WavWriter;

    /// Create a test WAV file with specified samples
    fn create_test_wav(samples: Vec<i16>, sample_rate: u32) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
        let temp_file = NamedTempFile::new()?;
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        
        let mut writer = WavWriter::create(temp_file.path(), spec)?;
        for sample in samples {
            writer.write_sample(sample)?;
        }
        writer.finalize()?;
        
        Ok(temp_file)
    }

    #[test]
    fn test_silent_audio_detection() {
        let validator = AudioValidator::new();
        
        // Create completely silent audio (1 second of zeros at 16kHz)
        let silent_samples = vec![0i16; 16000];
        let temp_file = create_test_wav(silent_samples, 16000).unwrap();
        
        let result = validator.validate_audio_file(temp_file.path()).unwrap();
        
        match result {
            AudioValidationResult::Silent => {
                // Expected result
            }
            other => panic!("Expected Silent, got {:?}", other),
        }
    }

    #[test]
    fn test_too_quiet_audio() {
        let validator = AudioValidator::new();
        
        // Create very quiet audio (amplitude just above silence threshold)
        let quiet_samples: Vec<i16> = (0..16000)
            .map(|i| ((i as f32 * 0.01).sin() * 500.0) as i16) // Quiet but detectable sine wave
            .collect();
        
        let temp_file = create_test_wav(quiet_samples, 16000).unwrap();
        
        let result = validator.validate_audio_file(temp_file.path()).unwrap();
        
        match result {
            AudioValidationResult::TooQuiet { .. } => {
                // Expected result
            }
            other => panic!("Expected TooQuiet, got {:?}", other),
        }
    }

    #[test]
    fn test_too_short_audio() {
        let validator = AudioValidator::new();
        
        // Create audio shorter than minimum duration (0.1 seconds)
        let short_samples: Vec<i16> = (0..1600) // 0.1 seconds at 16kHz
            .map(|i| ((i as f32 * 0.01).sin() * 16000.0) as i16) // Loud enough
            .collect();
        
        let temp_file = create_test_wav(short_samples, 16000).unwrap();
        
        let result = validator.validate_audio_file(temp_file.path()).unwrap();
        
        match result {
            AudioValidationResult::TooShort { duration } => {
                assert!(duration < 0.5);
            }
            other => panic!("Expected TooShort, got {:?}", other),
        }
    }

    #[test]
    fn test_valid_audio() {
        let validator = AudioValidator::new();
        
        // Create valid audio (1 second of reasonable amplitude)
        let valid_samples: Vec<i16> = (0..16000)
            .map(|i| ((i as f32 * 0.01).sin() * 8000.0) as i16) // 25% amplitude
            .collect();
        
        let temp_file = create_test_wav(valid_samples, 16000).unwrap();
        
        let result = validator.validate_audio_file(temp_file.path()).unwrap();
        
        match result {
            AudioValidationResult::Valid { energy, duration, peak, .. } => {
                assert!(energy > 0.01);
                assert!(duration >= 0.5);
                assert!(peak > 0.05);
            }
            other => panic!("Expected Valid, got {:?}", other),
        }
    }

    #[test]
    fn test_edge_case_very_long_audio() {
        let validator = AudioValidator::new();
        
        // Create very long audio (10 seconds) to test performance
        let long_samples: Vec<i16> = (0..160000) // 10 seconds at 16kHz
            .map(|i| ((i as f32 * 0.001).sin() * 8000.0) as i16)
            .collect();
        
        let temp_file = create_test_wav(long_samples, 16000).unwrap();
        
        let start = std::time::Instant::now();
        let result = validator.validate_audio_file(temp_file.path()).unwrap();
        let duration = start.elapsed();
        
        // Should complete quickly even for long files
        assert!(duration.as_millis() < 1000, "Validation took too long: {}ms", duration.as_millis());
        
        match result {
            AudioValidationResult::Valid { duration: audio_duration, .. } => {
                assert!((audio_duration - 10.0).abs() < 0.1); // Should be ~10 seconds
            }
            other => panic!("Expected Valid for long audio, got {:?}", other),
        }
    }

    #[test]
    fn test_edge_case_minimum_valid_duration() {
        let validator = AudioValidator::new();
        
        // Create audio exactly at the minimum duration threshold (0.5 seconds)
        let min_samples: Vec<i16> = (0..8000) // Exactly 0.5 seconds at 16kHz
            .map(|i| ((i as f32 * 0.1).sin() * 8000.0) as i16)
            .collect();
        
        let temp_file = create_test_wav(min_samples, 16000).unwrap();
        
        let result = validator.validate_audio_file(temp_file.path()).unwrap();
        
        match result {
            AudioValidationResult::Valid { duration, .. } => {
                assert!((duration - 0.5).abs() < 0.01); // Should be exactly 0.5 seconds
            }
            other => panic!("Expected Valid for minimum duration, got {:?}", other),
        }
    }

    #[test]
    fn test_edge_case_just_below_minimum_duration() {
        let validator = AudioValidator::new();
        
        // Create audio just below minimum duration (0.49 seconds)
        let short_samples: Vec<i16> = (0..7840) // 0.49 seconds at 16kHz
            .map(|i| ((i as f32 * 0.1).sin() * 8000.0) as i16)
            .collect();
        
        let temp_file = create_test_wav(short_samples, 16000).unwrap();
        
        let result = validator.validate_audio_file(temp_file.path()).unwrap();
        
        match result {
            AudioValidationResult::TooShort { duration } => {
                assert!(duration < 0.5);
            }
            other => panic!("Expected TooShort for sub-minimum duration, got {:?}", other),
        }
    }

    #[test]
    fn test_edge_case_clipped_audio() {
        let validator = AudioValidator::new();
        
        // Create clipped audio (samples at maximum amplitude)
        let clipped_samples: Vec<i16> = (0..16000)
            .map(|i| {
                if i % 100 < 50 {
                    i16::MAX // Clipped high
                } else {
                    i16::MIN // Clipped low
                }
            })
            .collect();
        
        let temp_file = create_test_wav(clipped_samples, 16000).unwrap();
        
        let result = validator.validate_audio_file(temp_file.path()).unwrap();
        
        match result {
            AudioValidationResult::Valid { peak, .. } => {
                assert!(peak > 0.9); // Should detect high peak amplitude
            }
            other => panic!("Expected Valid for clipped audio, got {:?}", other),
        }
    }

    #[test]
    fn test_edge_case_intermittent_silence() {
        let validator = AudioValidator::new();
        
        // Create audio with intermittent silence (realistic speech pattern)
        let intermittent_samples: Vec<i16> = (0..16000)
            .map(|i| {
                if (i / 2000) % 2 == 0 {
                    // Speech segments
                    ((i as f32 * 0.01).sin() * 8000.0) as i16
                } else {
                    // Silence segments
                    0
                }
            })
            .collect();
        
        let temp_file = create_test_wav(intermittent_samples, 16000).unwrap();
        
        let result = validator.validate_audio_file(temp_file.path()).unwrap();
        
        match result {
            AudioValidationResult::Valid { energy, .. } => {
                // Should still be valid despite silence periods
                assert!(energy > 0.005); // Lower than pure speech but above silence
            }
            other => panic!("Expected Valid for intermittent audio, got {:?}", other),
        }
    }

    #[test]
    fn test_edge_case_custom_thresholds() {
        // Test with very strict thresholds
        let strict_thresholds = ValidationThresholds {
            min_energy: 0.1,      // 10% RMS energy (very high)
            min_peak: 0.3,        // 30% peak amplitude (very high)
            min_duration: 2.0,    // 2 seconds (longer)
            silence_threshold: 0.001,
        };
        
        let validator = AudioValidator::with_thresholds(strict_thresholds);
        
        // Create audio that would normally be valid
        let normal_samples: Vec<i16> = (0..16000)
            .map(|i| ((i as f32 * 0.01).sin() * 4000.0) as i16) // 12.5% amplitude
            .collect();
        
        let temp_file = create_test_wav(normal_samples, 16000).unwrap();
        
        let result = validator.validate_audio_file(temp_file.path()).unwrap();
        
        match result {
            AudioValidationResult::TooQuiet { .. } | AudioValidationResult::TooShort { .. } => {
                // Expected with strict thresholds
            }
            other => panic!("Expected TooQuiet or TooShort with strict thresholds, got {:?}", other),
        }
    }

    #[test]
    fn test_edge_case_different_sample_rates() {
        let validator = AudioValidator::new();
        
        // Test with different sample rates
        let sample_rates = [8000, 16000, 22050, 44100, 48000];
        
        for &sample_rate in &sample_rates {
            // Create 1 second of audio at this sample rate
            let samples: Vec<i16> = (0..sample_rate as usize)
                .map(|i| ((i as f32 * 0.01).sin() * 8000.0) as i16)
                .collect();
            
            let temp_file = create_test_wav(samples, sample_rate).unwrap();
            
            let result = validator.validate_audio_file(temp_file.path()).unwrap();
            
            match result {
                AudioValidationResult::Valid { duration, .. } => {
                    // Duration should be approximately 1 second regardless of sample rate
                    assert!((duration - 1.0).abs() < 0.01, 
                           "Duration incorrect for {}Hz: {}s", sample_rate, duration);
                }
                other => panic!("Expected Valid for {}Hz, got {:?}", sample_rate, other),
            }
        }
    }

    #[test]
    fn test_edge_case_file_corruption_simulation() {
        let validator = AudioValidator::new();
        
        // Create a file that looks like WAV but has invalid data
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), b"RIFF fake WAV file").unwrap();
        
        let result = validator.validate_audio_file(temp_file.path());
        
        match result {
            Err(error_msg) => {
                assert!(error_msg.contains("Failed to open audio file"));
            }
            Ok(other) => panic!("Expected error for corrupted file, got {:?}", other),
        }
    }

    #[test]
    fn test_edge_case_empty_file() {
        let validator = AudioValidator::new();
        
        // Create completely empty file
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        // File is already empty by default
        
        let result = validator.validate_audio_file(temp_file.path());
        
        match result {
            Err(error_msg) => {
                assert!(error_msg.contains("Failed to open audio file"));
            }
            Ok(other) => panic!("Expected error for empty file, got {:?}", other),
        }
    }

    #[test]
    fn test_edge_case_noise_vs_speech() {
        let validator = AudioValidator::new();
        
        // Create white noise (should be detected as valid but quiet)
        let noise_samples: Vec<i16> = (0..16000)
            .map(|_| ((rand::random::<f32>() * 2.0 - 1.0) * 2000.0) as i16)
            .collect();
        
        let temp_file = create_test_wav(noise_samples, 16000).unwrap();
        
        let result = validator.validate_audio_file(temp_file.path()).unwrap();
        
        match result {
            AudioValidationResult::TooQuiet { .. } => {
                // Noise should typically be flagged as too quiet
            }
            AudioValidationResult::Valid { .. } => {
                // Might be valid if noise amplitude is sufficient
            }
            other => panic!("Unexpected result for noise: {:?}", other),
        }
    }

    #[test]
    fn test_edge_case_mono_vs_stereo() {
        let validator = AudioValidator::new();
        
        // Create stereo audio (2 channels)
        let _mono_samples = vec![8000i16; 16000]; // 1 second mono
        
        // Create stereo WAV manually
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let spec = hound::WavSpec {
            channels: 2, // Stereo
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        
        let mut writer = hound::WavWriter::create(temp_file.path(), spec).unwrap();
        for _ in 0..8000 { // 0.5 seconds stereo = 8000 frames
            writer.write_sample(8000i16).unwrap(); // Left channel
            writer.write_sample(8000i16).unwrap(); // Right channel
        }
        writer.finalize().unwrap();
        
        let result = validator.validate_audio_file(temp_file.path()).unwrap();
        
        match result {
            AudioValidationResult::Valid { duration, .. } => {
                // Should handle stereo correctly
                assert!((duration - 0.5).abs() < 0.01);
            }
            other => panic!("Expected Valid for stereo audio, got {:?}", other),
        }
    }

    /// Performance benchmarking test
    #[test]
    fn test_validation_performance_benchmark() {
        let validator = AudioValidator::new();
        
        // Create various audio files and measure validation time
        let test_cases = vec![
            ("silent_1s", vec![0i16; 16000]),
            ("quiet_1s", (0..16000).map(|i| ((i as f32 * 0.01).sin() * 500.0) as i16).collect()),
            ("normal_1s", (0..16000).map(|i| ((i as f32 * 0.01).sin() * 8000.0) as i16).collect()),
            ("loud_1s", (0..16000).map(|i| ((i as f32 * 0.01).sin() * 16000.0) as i16).collect()),
        ];
        
        for (name, samples) in test_cases {
            let temp_file = create_test_wav(samples, 16000).unwrap();
            
            let start = std::time::Instant::now();
            let _result = validator.validate_audio_file(temp_file.path()).unwrap();
            let duration = start.elapsed();
            
            // All validations should complete quickly
            assert!(duration.as_millis() < 100, 
                   "Validation of {} took too long: {}ms", name, duration.as_millis());
            
            println!("✅ {} validation: {}ms", name, duration.as_millis());
        }
    }

    /// Test the validator with extremely short samples
    #[test] 
    fn test_microsecond_audio() {
        let validator = AudioValidator::new();
        
        // Create extremely short audio (160 samples = 0.01 seconds at 16kHz)
        let micro_samples: Vec<i16> = (0..160)
            .map(|i| ((i as f32 * 0.1).sin() * 8000.0) as i16)
            .collect();
        
        let temp_file = create_test_wav(micro_samples, 16000).unwrap();
        
        let result = validator.validate_audio_file(temp_file.path()).unwrap();
        
        match result {
            AudioValidationResult::TooShort { duration } => {
                assert!(duration < 0.1);
                assert!(duration > 0.0);
            }
            other => panic!("Expected TooShort for microsecond audio, got {:?}", other),
        }
    }
}