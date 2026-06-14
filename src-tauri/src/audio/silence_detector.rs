use std::time::{Duration, Instant};

pub const VOICE_RMS_THRESHOLD: f32 = 0.005;
pub const NO_SPEECH_WARNING_AFTER: Duration = Duration::from_secs(5);
pub const LONG_SILENCE_WARNING_AFTER: Duration = Duration::from_secs(10);
pub const SILENCE_TIMEOUT_AFTER: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SilenceDetectorEvent {
    Clear,
    DeadMicWarn,
    LongSilenceWarn,
    TimeoutWithSpeech,
    TimeoutNoSpeech,
}

impl SilenceDetectorEvent {
    #[inline]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::TimeoutWithSpeech | Self::TimeoutNoSpeech)
    }
}

pub struct SilenceDetector {
    started_at: Instant,
    last_voice_time: Instant,
    last_event: SilenceDetectorEvent,
    speech_detected: bool,
    voice_threshold: f32,
    no_speech_warning_after: Duration,
    long_silence_warning_after: Duration,
    silence_timeout_after: Duration,
}

impl SilenceDetector {
    pub fn new() -> Self {
        Self::new_at(Instant::now())
    }

    pub fn update(&mut self, rms: f32) -> Option<SilenceDetectorEvent> {
        self.update_at(rms, Instant::now())
    }

    fn new_at(now: Instant) -> Self {
        Self {
            started_at: now,
            last_voice_time: now,
            last_event: SilenceDetectorEvent::Clear,
            speech_detected: false,
            voice_threshold: VOICE_RMS_THRESHOLD,
            no_speech_warning_after: NO_SPEECH_WARNING_AFTER,
            long_silence_warning_after: LONG_SILENCE_WARNING_AFTER,
            silence_timeout_after: SILENCE_TIMEOUT_AFTER,
        }
    }

    fn update_at(&mut self, rms: f32, now: Instant) -> Option<SilenceDetectorEvent> {
        if self.last_event.is_terminal() {
            return None;
        }

        if rms > self.voice_threshold {
            self.last_voice_time = now;
            self.speech_detected = true;
            if self.last_event != SilenceDetectorEvent::Clear {
                return self.emit_if_changed(SilenceDetectorEvent::Clear);
            }
            return None;
        }

        if !self.speech_detected {
            let elapsed = now.saturating_duration_since(self.started_at);
            let tier = if elapsed >= self.silence_timeout_after {
                SilenceDetectorEvent::TimeoutNoSpeech
            } else if elapsed >= self.no_speech_warning_after {
                SilenceDetectorEvent::DeadMicWarn
            } else {
                SilenceDetectorEvent::Clear
            };
            return self.emit_if_changed(tier);
        }

        let elapsed = now.saturating_duration_since(self.last_voice_time);
        let tier = if elapsed >= self.silence_timeout_after {
            SilenceDetectorEvent::TimeoutWithSpeech
        } else if elapsed >= self.long_silence_warning_after {
            SilenceDetectorEvent::LongSilenceWarn
        } else {
            SilenceDetectorEvent::Clear
        };
        self.emit_if_changed(tier)
    }

    fn emit_if_changed(&mut self, event: SilenceDetectorEvent) -> Option<SilenceDetectorEvent> {
        if self.last_event == event {
            return None;
        }
        self.last_event = event;
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SILENT: f32 = 0.0;
    const SPEECH: f32 = VOICE_RMS_THRESHOLD + 0.001;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn starts_clear_without_speech_before_threshold() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);

        assert_eq!(detector.update_at(SILENT, start), None);
        assert_eq!(
            detector.update_at(SILENT, start + Duration::from_secs(4)),
            None
        );
        assert!(!detector.speech_detected);
    }

    #[test]
    fn dead_mic_warns_once_after_five_seconds_without_speech() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);

        assert_eq!(
            detector.update_at(SILENT, start + NO_SPEECH_WARNING_AFTER),
            Some(SilenceDetectorEvent::DeadMicWarn)
        );
        assert_eq!(
            detector.update_at(SILENT, start + Duration::from_secs(6)),
            None
        );
        assert_eq!(
            detector.update_at(SILENT, start + Duration::from_secs(30)),
            None
        );
    }

    #[test]
    fn speech_above_threshold_flips_speech_detected() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);

        assert!(!detector.speech_detected);
        assert_eq!(
            detector.update_at(SPEECH, start + Duration::from_secs(1)),
            None
        );
        assert!(detector.speech_detected);
    }

    #[test]
    fn threshold_equal_to_voice_threshold_is_not_speech() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);

        assert_eq!(
            detector.update_at(VOICE_RMS_THRESHOLD, start + Duration::from_secs(1)),
            None
        );
        assert!(!detector.speech_detected);
    }

    #[test]
    fn dead_mic_warning_clears_when_speech_arrives() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);

        assert_eq!(
            detector.update_at(SILENT, start + NO_SPEECH_WARNING_AFTER),
            Some(SilenceDetectorEvent::DeadMicWarn)
        );
        assert_eq!(
            detector.update_at(SPEECH, start + Duration::from_secs(6)),
            Some(SilenceDetectorEvent::Clear)
        );
    }

    #[test]
    fn post_speech_long_silence_warns_once_after_ten_seconds() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);
        let speech_at = start + Duration::from_secs(1);

        assert_eq!(detector.update_at(SPEECH, speech_at), None);
        assert_eq!(
            detector.update_at(SILENT, speech_at + Duration::from_secs(9)),
            None
        );
        assert_eq!(
            detector.update_at(SILENT, speech_at + LONG_SILENCE_WARNING_AFTER),
            Some(SilenceDetectorEvent::LongSilenceWarn)
        );
        assert_eq!(
            detector.update_at(SILENT, speech_at + Duration::from_secs(15)),
            None
        );
    }

    #[test]
    fn long_silence_warning_clears_when_speech_resumes() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);
        let speech_at = start + Duration::from_secs(1);

        assert_eq!(detector.update_at(SPEECH, speech_at), None);
        assert_eq!(
            detector.update_at(SILENT, speech_at + LONG_SILENCE_WARNING_AFTER),
            Some(SilenceDetectorEvent::LongSilenceWarn)
        );
        assert_eq!(
            detector.update_at(SPEECH, speech_at + Duration::from_secs(12)),
            Some(SilenceDetectorEvent::Clear)
        );
    }

    #[test]
    fn post_speech_timeout_emits_timeout_with_speech_once_after_sixty_seconds() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);
        let speech_at = start + Duration::from_secs(1);

        assert_eq!(detector.update_at(SPEECH, speech_at), None);
        assert_eq!(
            detector.update_at(SILENT, speech_at + SILENCE_TIMEOUT_AFTER),
            Some(SilenceDetectorEvent::TimeoutWithSpeech)
        );
        assert_eq!(
            detector.update_at(SILENT, speech_at + Duration::from_secs(61)),
            None
        );
    }

    #[test]
    fn no_speech_timeout_emits_timeout_no_speech_once_after_sixty_seconds() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);

        assert_eq!(
            detector.update_at(SILENT, start + SILENCE_TIMEOUT_AFTER),
            Some(SilenceDetectorEvent::TimeoutNoSpeech)
        );
        assert_eq!(
            detector.update_at(SILENT, start + Duration::from_secs(61)),
            None
        );
    }

    #[test]
    fn terminal_event_is_final_and_never_clears_after_late_speech() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);

        assert_eq!(
            detector.update_at(SILENT, start + SILENCE_TIMEOUT_AFTER),
            Some(SilenceDetectorEvent::TimeoutNoSpeech)
        );
        assert_eq!(
            detector.update_at(SPEECH, start + Duration::from_secs(61)),
            None
        );
        assert_eq!(
            detector.update_at(SILENT, start + Duration::from_secs(62)),
            None
        );
    }
}
