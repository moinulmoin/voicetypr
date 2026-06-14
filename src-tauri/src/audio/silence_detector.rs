use std::time::{Duration, Instant};

pub const VOICE_RMS_THRESHOLD: f32 = 0.005;
pub const NO_SPEECH_WARNING_AFTER: Duration = Duration::from_secs(5);
pub const LONG_SILENCE_WARNING_AFTER: Duration = Duration::from_secs(10);
pub const SILENCE_TIMEOUT_AFTER: Duration = Duration::from_secs(60);
/// Minimum continuous above-threshold duration that counts as real voice.
/// A transient blip (keyboard, fan, brief cough) shorter than this must NOT be
/// treated as speech — otherwise ambient noise flips the session into the
/// post-speech branch and a silent recording never cancels.
pub const MIN_VOICE_DURATION: Duration = Duration::from_millis(300);

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
    voice_run_start: Option<Instant>,
    min_voice_duration: Duration,
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
            voice_run_start: None,
            min_voice_duration: MIN_VOICE_DURATION,
        }
    }

    fn update_at(&mut self, rms: f32, now: Instant) -> Option<SilenceDetectorEvent> {
        if self.last_event.is_terminal() {
            return None;
        }

        if rms > self.voice_threshold {
            // Require SUSTAINED above-threshold audio before treating it as voice.
            // A blip under min_voice_duration must not flip speech_detected or reset
            // last_voice_time, so intermittent ambient noise can't masquerade as speech.
            let run_start = *self.voice_run_start.get_or_insert(now);
            if now.saturating_duration_since(run_start) >= self.min_voice_duration {
                self.last_voice_time = now;
                self.speech_detected = true;
                if self.last_event != SilenceDetectorEvent::Clear {
                    return self.emit_if_changed(SilenceDetectorEvent::Clear);
                }
                return None;
            }
            // Unconfirmed blip: fall through to silence-tier evaluation below
            // without disturbing speech_detected / last_voice_time.
        } else {
            self.voice_run_start = None;
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

    // Feed sustained voice (>= MIN_VOICE_DURATION continuous) so speech_detected
    // confirms; returns the instant speech is confirmed (the new last_voice_time).
    fn confirm_speech(d: &mut SilenceDetector, start: Instant) -> Instant {
        d.update_at(SPEECH, start);
        let confirmed = start + MIN_VOICE_DURATION;
        d.update_at(SPEECH, confirmed);
        confirmed
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
    fn single_frame_is_not_speech_but_sustained_voice_is() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);

        // One above-threshold frame is a blip, not speech.
        assert_eq!(
            detector.update_at(SPEECH, start + Duration::from_secs(1)),
            None
        );
        assert!(!detector.speech_detected);

        // Continuous voice past MIN_VOICE_DURATION confirms speech.
        detector.update_at(SPEECH, start + Duration::from_secs(1) + MIN_VOICE_DURATION);
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

    // Real-world: a "silent" recording is never truly 0.0 — a keyboard tap or fan
    // tick is a brief above-threshold blip. It must NOT read as speech, and the
    // recording must still cancel at the no-speech timeout.
    #[test]
    fn brief_noise_blip_does_not_count_as_speech_and_still_cancels() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);

        assert_eq!(
            detector.update_at(SPEECH, start + Duration::from_secs(1)),
            None
        );
        assert_eq!(
            detector.update_at(SILENT, start + Duration::from_millis(1_050)),
            None
        );
        assert!(!detector.speech_detected);

        assert_eq!(
            detector.update_at(SILENT, start + SILENCE_TIMEOUT_AFTER),
            Some(SilenceDetectorEvent::TimeoutNoSpeech)
        );
    }

    // Real-world: intermittent ambient blips across the minute must not keep
    // resetting the silence clock (the original bug dragged transcription out for
    // minutes). With no sustained voice, it still cancels at 60s.
    #[test]
    fn intermittent_blips_do_not_reset_no_speech_timeout() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);

        for secs in [3u64, 12, 25, 40, 55] {
            detector.update_at(SPEECH, start + Duration::from_secs(secs));
            detector.update_at(
                SILENT,
                start + Duration::from_secs(secs) + Duration::from_millis(50),
            );
            assert!(
                !detector.speech_detected,
                "blip at {secs}s must not confirm speech"
            );
        }

        assert_eq!(
            detector.update_at(SILENT, start + SILENCE_TIMEOUT_AFTER),
            Some(SilenceDetectorEvent::TimeoutNoSpeech)
        );
    }

    #[test]
    fn dead_mic_warning_clears_when_sustained_speech_arrives() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);

        assert_eq!(
            detector.update_at(SILENT, start + NO_SPEECH_WARNING_AFTER),
            Some(SilenceDetectorEvent::DeadMicWarn)
        );
        // First voice frame is an unconfirmed blip — no clear yet.
        assert_eq!(
            detector.update_at(SPEECH, start + Duration::from_secs(6)),
            None
        );
        // Sustained past MIN_VOICE_DURATION confirms and clears the warning.
        assert_eq!(
            detector.update_at(SPEECH, start + Duration::from_secs(6) + MIN_VOICE_DURATION),
            Some(SilenceDetectorEvent::Clear)
        );
    }

    #[test]
    fn post_speech_long_silence_warns_once_after_ten_seconds() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);
        let voiced = confirm_speech(&mut detector, start + Duration::from_secs(1));

        assert_eq!(
            detector.update_at(SILENT, voiced + Duration::from_secs(9)),
            None
        );
        assert_eq!(
            detector.update_at(SILENT, voiced + LONG_SILENCE_WARNING_AFTER),
            Some(SilenceDetectorEvent::LongSilenceWarn)
        );
        assert_eq!(
            detector.update_at(SILENT, voiced + Duration::from_secs(15)),
            None
        );
    }

    #[test]
    fn long_silence_warning_clears_when_speech_resumes() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);
        let voiced = confirm_speech(&mut detector, start + Duration::from_secs(1));

        assert_eq!(
            detector.update_at(SILENT, voiced + LONG_SILENCE_WARNING_AFTER),
            Some(SilenceDetectorEvent::LongSilenceWarn)
        );
        // First resumed frame is an unconfirmed blip...
        assert_eq!(
            detector.update_at(SPEECH, voiced + Duration::from_secs(12)),
            None
        );
        // ...sustained voice clears it.
        assert_eq!(
            detector.update_at(
                SPEECH,
                voiced + Duration::from_secs(12) + MIN_VOICE_DURATION
            ),
            Some(SilenceDetectorEvent::Clear)
        );
    }

    #[test]
    fn post_speech_timeout_emits_timeout_with_speech_once_after_sixty_seconds() {
        let start = t0();
        let mut detector = SilenceDetector::new_at(start);
        let voiced = confirm_speech(&mut detector, start + Duration::from_secs(1));

        assert_eq!(
            detector.update_at(SILENT, voiced + SILENCE_TIMEOUT_AFTER),
            Some(SilenceDetectorEvent::TimeoutWithSpeech)
        );
        assert_eq!(
            detector.update_at(SILENT, voiced + Duration::from_secs(61)),
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
