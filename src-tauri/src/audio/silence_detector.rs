use std::time::{Duration, Instant};

/// Simple silence detector based on audio level
pub struct SilenceDetector {
    /// Last time voice was detected
    last_voice_time: Instant,
    /// How long silence before stopping
    silence_duration: Duration,
    /// RMS threshold for voice detection
    voice_threshold: f32,
}

impl SilenceDetector {
    pub fn new(silence_duration: Duration) -> Self {
        Self {
            last_voice_time: Instant::now(),
            silence_duration,
            voice_threshold: 0.005,  // 0.5% - matches original whisper.cpp threshold
        }
    }
    
    /// Update with current RMS level and check if should stop
    pub fn update(&mut self, rms: f32) -> bool {
        if rms > self.voice_threshold {
            // Voice detected, update timestamp
            self.last_voice_time = Instant::now();
            false  // Don't stop
        } else {
            // Check if silence duration exceeded
            self.last_voice_time.elapsed() > self.silence_duration
        }
    }
}