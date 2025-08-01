use rubato::{Resampler, SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction};

/// Resample audio from any sample rate to 16kHz for Whisper
pub fn resample_to_16khz(input: &[f32], input_sample_rate: u32) -> Result<Vec<f32>, String> {
    // If already at 16kHz, just return a copy
    if input_sample_rate == 16_000 {
        log::debug!("Audio already at 16kHz, no resampling needed");
        return Ok(input.to_vec());
    }

    log::info!("Resampling audio from {} Hz to 16000 Hz", input_sample_rate);

    // Calculate resampling ratio
    let resample_ratio = 16_000_f64 / input_sample_rate as f64;
    
    // Configure resampler parameters for good quality
    let params = SincInterpolationParameters {
        sinc_len: 256,      // Good balance of quality and performance
        f_cutoff: 0.95,     // Prevent aliasing
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    // Create resampler with fixed input chunk size
    // We'll process the entire audio at once for simplicity
    let chunk_size = input.len();
    let mut resampler = SincFixedIn::<f32>::new(
        resample_ratio,
        2.0,            // Maximum delay in seconds
        params,
        chunk_size,
        1,              // Single channel
    ).map_err(|e| format!("Failed to create resampler: {:?}", e))?;

    // Calculate expected output size
    let output_frames = resampler.output_frames_max();
    let mut output = vec![0.0f32; output_frames];

    // Process the audio
    let mut input_frames_used = 0;
    let mut output_frames_written = 0;

    // Process the entire input
    let (used, written) = resampler.process_into_buffer(
        &[input], 
        &mut [&mut output], 
        None
    ).map_err(|e| format!("Resampling failed: {:?}", e))?;

    input_frames_used += used;
    output_frames_written += written;

    // Handle any remaining samples
    if input_frames_used < input.len() {
        log::warn!(
            "Not all input samples were processed: {} of {} used", 
            input_frames_used, 
            input.len()
        );
    }

    // Trim output to actual size
    output.truncate(output_frames_written);

    log::info!(
        "Resampled {} samples to {} samples (ratio: {:.4})",
        input.len(),
        output_frames_written,
        resample_ratio
    );

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_identity() {
        // Test that 16kHz input returns unchanged
        let input = vec![0.5f32; 16_000];
        let result = resample_to_16khz(&input, 16_000).unwrap();
        assert_eq!(input.len(), result.len());
    }

    #[test]
    fn test_resample_48khz_to_16khz() {
        // Test 48kHz to 16kHz (3:1 ratio)
        let input = vec![0.5f32; 48_000];
        let result = resample_to_16khz(&input, 48_000).unwrap();
        // Should be approximately 1/3 the size
        assert!((result.len() as f32 - 16_000.0).abs() < 100.0);
    }

    #[test]
    fn test_resample_24khz_to_16khz() {
        // Test 24kHz to 16kHz (3:2 ratio)
        let input = vec![0.5f32; 24_000];
        let result = resample_to_16khz(&input, 24_000).unwrap();
        // Should be approximately 2/3 the size
        assert!((result.len() as f32 - 16_000.0).abs() < 100.0);
    }
}