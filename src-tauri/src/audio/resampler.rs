use audioadapter_buffers::direct::SequentialSlice;
use rubato::{Fft, FixedSync, Resampler};

/// Resample audio from any sample rate to 16kHz for Whisper.
///
/// Uses rubato's FFT-based synchronous resampler with `process_all_into_buffer`,
/// which handles arbitrary-length clips in a single call — including chunking,
/// partial tails, and internal delay buffer flushing. This eliminates tail-sample
/// loss that occurred with manual single-shot `process_into_buffer` calls.
pub fn resample_to_16khz(input: &[f32], input_sample_rate: u32) -> Result<Vec<f32>, String> {
    if input_sample_rate == 16_000 {
        log::debug!("Audio already at 16kHz, no resampling needed");
        return Ok(input.to_vec());
    }

    log::info!("Resampling audio from {} Hz to 16000 Hz", input_sample_rate);

    // Fft resampler: fast, always best quality for fixed-ratio offline resampling.
    // FixedSync::Both lets rubato pick optimal chunk sizes for the ratio.
    let mut resampler = Fft::<f32>::new(
        input_sample_rate as usize,
        16_000,
        1024,
        1, // sub_chunks
        1, // mono
        FixedSync::Both,
    )
    .map_err(|e| format!("Failed to create resampler: {:?}", e))?;

    // Allocate output buffer sized by the resampler's own calculation.
    let out_len = resampler.process_all_needed_output_len(input.len());
    let mut output = vec![0.0f32; out_len];

    // Wrap input/output in audioadapter slices (1 channel = mono).
    let input_adapter = SequentialSlice::new(input, 1, input.len())
        .map_err(|e| format!("Failed to create input adapter: {:?}", e))?;
    let mut output_adapter = SequentialSlice::new_mut(&mut output, 1, out_len)
        .map_err(|e| format!("Failed to create output adapter: {:?}", e))?;

    // Single call handles chunking, partial tails, and delay buffer flush.
    let (_in_frames, out_frames) = resampler
        .process_all_into_buffer(&input_adapter, &mut output_adapter, input.len(), None)
        .map_err(|e| format!("Resampling failed: {:?}", e))?;

    output.truncate(out_frames);

    log::info!(
        "Resampled {} samples to {} samples (ratio: {:.4})",
        input.len(),
        output.len(),
        16_000_f64 / input_sample_rate as f64
    );

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_identity() {
        let input = vec![0.5f32; 16_000];
        let result = resample_to_16khz(&input, 16_000).unwrap();
        assert_eq!(input.len(), result.len());
    }

    #[test]
    fn test_resample_48khz_to_16khz() {
        let input = vec![0.5f32; 48_000];
        let result = resample_to_16khz(&input, 48_000).unwrap();
        let expected = (input.len() as f64 * 16_000.0 / 48_000.0).ceil() as usize;
        assert!(
            result.len() >= expected,
            "Expected at least {} output samples, got {}",
            expected,
            result.len()
        );
    }

    #[test]
    fn test_resample_24khz_to_16khz() {
        let input = vec![0.5f32; 24_000];
        let result = resample_to_16khz(&input, 24_000).unwrap();
        let expected = (input.len() as f64 * 16_000.0 / 24_000.0).ceil() as usize;
        assert!(
            result.len() >= expected,
            "Expected at least {} output samples, got {}",
            expected,
            result.len()
        );
    }

    #[test]
    fn test_resample_no_tail_loss_48khz() {
        // 5-second 48kHz sine wave — must not lose tail samples
        let sample_rate = 48_000u32;
        let num_samples = sample_rate as usize * 5;
        let input: Vec<f32> = (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect();

        let result = resample_to_16khz(&input, sample_rate).unwrap();
        let expected_min = (num_samples as f64 * 16_000.0 / sample_rate as f64).ceil() as usize;
        assert!(
            result.len() >= expected_min,
            "Expected at least {} output samples, got {} — tail loss detected",
            expected_min,
            result.len()
        );
    }

    #[test]
    fn test_resample_no_tail_loss_44100hz() {
        // 3-second 44.1kHz signal — must not lose tail samples
        let sample_rate = 44_100u32;
        let num_samples = sample_rate as usize * 3;
        let input: Vec<f32> = (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect();

        let result = resample_to_16khz(&input, sample_rate).unwrap();
        let expected_min = (num_samples as f64 * 16_000.0 / sample_rate as f64).ceil() as usize;
        assert!(
            result.len() >= expected_min,
            "Expected at least {} output samples, got {} — tail loss detected",
            expected_min,
            result.len()
        );
    }

    #[test]
    fn test_resample_short_input() {
        // 100 samples at 48kHz — must produce output
        let input: Vec<f32> = (0..100).map(|i| i as f32 * 0.01).collect();
        let result = resample_to_16khz(&input, 48_000).unwrap();
        assert!(!result.is_empty(), "Short input must produce some output");
    }

    #[test]
    fn test_resample_tail_content_preserved() {
        // Last 10% has 9x amplitude — tail must survive resampling
        let sample_rate = 48_000u32;
        let num_samples = 48_000usize;
        let boundary = (num_samples as f64 * 0.9) as usize;

        let input: Vec<f32> = (0..num_samples)
            .map(|i| {
                let amp = if i < boundary { 0.1f32 } else { 0.9f32 };
                amp * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate as f32).sin()
            })
            .collect();

        let result = resample_to_16khz(&input, sample_rate).unwrap();

        let output_boundary = (result.len() as f64 * 0.9) as usize;
        let head = &result[..output_boundary];
        let tail = &result[output_boundary..];

        let head_rms = (head.iter().map(|x| x * x).sum::<f32>() / head.len() as f32).sqrt();
        let tail_rms = (tail.iter().map(|x| x * x).sum::<f32>() / tail.len() as f32).sqrt();

        assert!(
            tail_rms > head_rms * 3.0,
            "Tail RMS ({:.4}) should be much higher than head RMS ({:.4}) — last 10% was lost",
            tail_rms,
            head_rms
        );
    }
}
