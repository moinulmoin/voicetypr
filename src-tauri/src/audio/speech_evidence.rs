use super::normalizer::NormalizationMetrics;
use super::recorder::CaptureAudioMetrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechEvidenceClass {
    SpeechPositive,
    HighConfidenceNoInput,
    Uncertain,
}

impl SpeechEvidenceClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SpeechPositive => "speech_positive",
            Self::HighConfidenceNoInput => "high_confidence_no_input",
            Self::Uncertain => "uncertain",
        }
    }

    pub const fn would_skip_engine(self) -> bool {
        matches!(self, Self::HighConfidenceNoInput)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechEvidenceOutcome {
    AbortedBeforeResult,
    CancelledBeforeEngine,
    PreparationFailure,
    RecordingTooShort,
    EngineSuccess,
    EngineFailure,
}

impl SpeechEvidenceOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AbortedBeforeResult => "aborted_before_result",
            Self::CancelledBeforeEngine => "cancelled_before_engine",
            Self::PreparationFailure => "preparation_failure",
            Self::RecordingTooShort => "recording_too_short",
            Self::EngineSuccess => "success",
            Self::EngineFailure => "failure",
        }
    }
}

/// Attempt-scoped evidence finalizer. Construct it before fallible preparation,
/// then move it into the transcription future. `Drop` emits exactly one summary,
/// including when Tokio aborts the future before its first poll or during an await.
pub struct SpeechEvidenceAttempt {
    engine: String,
    route: &'static str,
    capture: Option<CaptureAudioMetrics>,
    prepared: Option<NormalizationMetrics>,
    outcome: SpeechEvidenceOutcome,
    #[cfg(test)]
    drop_counter: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
    #[cfg(test)]
    emitted_lines: Option<std::sync::Arc<std::sync::Mutex<Vec<String>>>>,
}

impl SpeechEvidenceAttempt {
    pub fn new(engine: String, route: &'static str, capture: Option<CaptureAudioMetrics>) -> Self {
        Self {
            engine,
            route,
            capture,
            prepared: None,
            outcome: SpeechEvidenceOutcome::AbortedBeforeResult,
            #[cfg(test)]
            drop_counter: None,
            #[cfg(test)]
            emitted_lines: None,
        }
    }

    pub fn set_prepared(&mut self, prepared: Option<NormalizationMetrics>) {
        self.prepared = prepared;
    }

    pub fn set_outcome(&mut self, outcome: SpeechEvidenceOutcome) {
        self.outcome = outcome;
    }

    fn formatted_summary(&self) -> String {
        let class = classify_speech_evidence(self.capture, self.prepared);
        let capture = self.capture.map(|metrics| {
            serde_json::json!({
                "sample_count": metrics.sample_count,
                "duration_ms": metrics.duration_ms,
                "rms": finite_f64(metrics.rms),
                "peak": finite_f32(metrics.peak),
                "sample_rate": metrics.sample_rate,
                "channels": metrics.channels,
                "sustained_speech": metrics.speech_detected,
            })
        });
        let prepared = self.prepared.map(|metrics| {
            serde_json::json!({
                "pre_gain_peak": finite_f32(metrics.pre_gain_peak),
                "speech_like_modulation": metrics.speech_like_modulation,
                "applied_gain": finite_f32(metrics.applied_gain),
                "input_duration_ms": metrics.input_duration_ms,
                "output_duration_ms": metrics.output_duration_ms,
                "trimmed_duration_ms": metrics.trimmed_duration_ms,
            })
        });
        let payload = serde_json::json!({
            "engine": self.engine,
            "route": self.route,
            "engine_outcome": self.outcome.as_str(),
            "class": class.as_str(),
            "would_skip_engine": class.would_skip_engine(),
            "capture": capture,
            "prepared": prepared,
        });
        format!("SPEECH_EVIDENCE {payload}")
    }
}

impl Drop for SpeechEvidenceAttempt {
    fn drop(&mut self) {
        let line = self.formatted_summary();
        log::info!("{}", line);
        #[cfg(test)]
        {
            if let Some(lines) = &self.emitted_lines {
                if let Ok(mut lines) = lines.lock() {
                    lines.push(line);
                }
            }
            if let Some(counter) = &self.drop_counter {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }
}

pub fn classify_speech_evidence(
    capture: Option<CaptureAudioMetrics>,
    prepared: Option<NormalizationMetrics>,
) -> SpeechEvidenceClass {
    let capture_speech = capture
        .map(|metrics| metrics.speech_detected)
        .unwrap_or(false);
    let prepared_speech = prepared
        .map(|metrics| metrics.speech_like_modulation)
        .unwrap_or(false);
    if capture_speech || prepared_speech {
        return SpeechEvidenceClass::SpeechPositive;
    }

    if capture
        .map(|metrics| {
            metrics.sample_count > 0
                && metrics.rms.is_finite()
                && metrics.peak.is_finite()
                && metrics.rms >= 0.0
                && metrics.peak >= 0.0
                && metrics.rms == 0.0
                && metrics.peak == 0.0
        })
        .unwrap_or(false)
    {
        SpeechEvidenceClass::HighConfidenceNoInput
    } else {
        SpeechEvidenceClass::Uncertain
    }
}

fn finite_f64(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

fn finite_f32(value: f32) -> Option<f32> {
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(
        sample_count: u64,
        rms: f64,
        peak: f32,
        speech_detected: bool,
    ) -> CaptureAudioMetrics {
        CaptureAudioMetrics {
            sample_count,
            duration_ms: 1000,
            rms,
            peak,
            sample_rate: 16_000,
            channels: 1,
            speech_detected,
        }
    }

    fn prepared(speech_like_modulation: bool) -> NormalizationMetrics {
        NormalizationMetrics {
            pre_gain_peak: 0.02,
            speech_like_modulation,
            applied_gain: 10.0,
            input_duration_ms: 1000,
            output_duration_ms: 1000,
            trimmed_duration_ms: 0,
        }
    }

    #[test]
    fn positive_capture_or_prepared_evidence_always_wins() {
        assert_eq!(
            classify_speech_evidence(Some(capture(16_000, 0.0, 0.0, true)), None),
            SpeechEvidenceClass::SpeechPositive
        );
        assert_eq!(
            classify_speech_evidence(Some(capture(16_000, 0.0, 0.0, false)), Some(prepared(true)),),
            SpeechEvidenceClass::SpeechPositive
        );
        assert_eq!(
            classify_speech_evidence(None, Some(prepared(true))),
            SpeechEvidenceClass::SpeechPositive
        );
    }

    #[test]
    fn only_finite_nonempty_digital_zero_is_high_confidence_no_input() {
        let class = classify_speech_evidence(Some(capture(16_000, 0.0, 0.0, false)), None);
        assert_eq!(class, SpeechEvidenceClass::HighConfidenceNoInput);
        assert!(class.would_skip_engine());

        for metrics in [
            capture(0, 0.0, 0.0, false),
            capture(16_000, 1e-12, 0.0, false),
            capture(16_000, 0.0, 1e-10, false),
            capture(16_000, f64::NAN, 0.0, false),
            capture(16_000, 0.0, f32::NAN, false),
            capture(16_000, f64::INFINITY, 0.0, false),
            capture(16_000, 0.0, f32::INFINITY, false),
            capture(16_000, -0.1, 0.0, false),
            capture(16_000, 0.0, -0.1, false),
        ] {
            assert_eq!(
                classify_speech_evidence(Some(metrics), None),
                SpeechEvidenceClass::Uncertain
            );
        }
    }

    #[test]
    fn any_nonzero_or_missing_capture_evidence_remains_uncertain() {
        assert_eq!(
            classify_speech_evidence(Some(capture(16_000, 1e-12, 1e-10, false)), None,),
            SpeechEvidenceClass::Uncertain
        );
        assert_eq!(
            classify_speech_evidence(Some(capture(16_000, 0.000_01, 0.005, false)), None,),
            SpeechEvidenceClass::Uncertain
        );
        assert_eq!(
            classify_speech_evidence(None, Some(prepared(false))),
            SpeechEvidenceClass::Uncertain
        );
    }

    #[test]
    fn structured_json_escapes_labels_and_marks_missing_metrics_null() {
        let attempt = SpeechEvidenceAttempt {
            engine: "remote \"lab\"\nserver".to_string(),
            route: "remote",
            capture: Some(capture(16_000, 0.1, 0.3, true)),
            prepared: None,
            outcome: SpeechEvidenceOutcome::EngineSuccess,
            drop_counter: None,
            emitted_lines: None,
        };
        let line = attempt.formatted_summary();
        let payload = line.strip_prefix("SPEECH_EVIDENCE ").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(payload).unwrap();

        assert_eq!(parsed["engine"], "remote \"lab\"\nserver");
        assert_eq!(parsed["engine_outcome"], "success");
        assert_eq!(parsed["class"], "speech_positive");
        assert_eq!(parsed["would_skip_engine"], false);
        assert_eq!(parsed["capture"]["rms"], 0.1);
        assert!(parsed["prepared"].is_null());
        assert_eq!(line.lines().count(), 1);
    }

    #[tokio::test]
    async fn abort_before_first_poll_emits_exactly_once() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut attempt = SpeechEvidenceAttempt::new("whisper".to_string(), "local", None);
        attempt.drop_counter = Some(counter.clone());
        attempt.emitted_lines = Some(lines.clone());
        let future = async move {
            let _attempt = attempt;
            std::future::pending::<()>().await;
        };

        let handle = tokio::spawn(future);
        handle.abort();
        let _ = handle.await;

        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
        let emitted = lines
            .lock()
            .ok()
            .map(|lines| lines.clone())
            .unwrap_or_default();
        assert_eq!(emitted.len(), 1);
        assert!(emitted[0].contains("\"engine_outcome\":\"aborted_before_result\""));
    }

    #[tokio::test]
    async fn abort_during_await_emits_exactly_once() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut attempt = SpeechEvidenceAttempt::new("openai".to_string(), "cloud", None);
        attempt.drop_counter = Some(counter.clone());
        attempt.emitted_lines = Some(lines.clone());
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            let _attempt = attempt;
            let _ = started_tx.send(());
            std::future::pending::<()>().await;
        });
        let _ = started_rx.await;

        handle.abort();
        let _ = handle.await;

        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(lines.lock().ok().map(|lines| lines.len()), Some(1));
    }

    #[tokio::test]
    async fn task_panic_emits_aborted_outcome_exactly_once() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut attempt = SpeechEvidenceAttempt::new("remote".to_string(), "remote", None);
        attempt.drop_counter = Some(counter.clone());
        attempt.emitted_lines = Some(lines.clone());
        let handle = tokio::spawn(async move {
            let _attempt = attempt;
            panic!("simulated task panic");
        });

        let error = handle.await.unwrap_err();

        assert!(error.is_panic());
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
        let emitted = lines
            .lock()
            .ok()
            .map(|lines| lines.clone())
            .unwrap_or_default();
        assert_eq!(emitted.len(), 1);
        assert!(emitted[0].contains("\"engine_outcome\":\"aborted_before_result\""));
    }

    #[test]
    fn every_explicit_terminal_outcome_emits_once() {
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let outcomes = [
            SpeechEvidenceOutcome::CancelledBeforeEngine,
            SpeechEvidenceOutcome::PreparationFailure,
            SpeechEvidenceOutcome::RecordingTooShort,
            SpeechEvidenceOutcome::EngineSuccess,
            SpeechEvidenceOutcome::EngineFailure,
        ];

        for outcome in outcomes {
            let mut attempt = SpeechEvidenceAttempt::new("whisper".to_string(), "local", None);
            attempt.emitted_lines = Some(lines.clone());
            attempt.set_outcome(outcome);
            drop(attempt);
        }

        let emitted = lines
            .lock()
            .ok()
            .map(|lines| lines.clone())
            .unwrap_or_default();
        assert_eq!(emitted.len(), outcomes.len());
        for (line, outcome) in emitted.iter().zip(outcomes) {
            assert!(line.contains(&format!("\"engine_outcome\":\"{}\"", outcome.as_str())));
        }
    }
}
