use serde::Serialize;
use std::sync::{Mutex, MutexGuard};

pub(crate) const STARTUP_TRAY_ATTEMPTS: u32 = 5;
pub(crate) const DEFERRED_TRAY_RETRY_DELAYS_SECS: [u64; 3] = [2, 10, 30];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrayStatus {
    pub(crate) available: bool,
    pub(crate) attempts: u32,
    pub(crate) last_error: Option<String>,
}

#[derive(Default)]
pub(crate) struct TrayStatusState(Mutex<TrayStatus>);

impl TrayStatusState {
    fn lock(&self) -> MutexGuard<'_, TrayStatus> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn snapshot(&self) -> TrayStatus {
        self.lock().clone()
    }

    pub(crate) fn record_success(&self) -> TrayStatus {
        let mut status = self.lock();
        status.available = true;
        status.attempts += 1;
        status.last_error = None;
        status.clone()
    }

    pub(crate) fn record_failure(&self, error: String) -> TrayStatus {
        let mut status = self.lock();
        status.available = false;
        status.attempts += 1;
        status.last_error = Some(error);
        status.clone()
    }

    pub(crate) fn record_present(&self) -> TrayStatus {
        let mut status = self.lock();
        status.available = true;
        status.last_error = None;
        status.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::TrayStatusState;

    #[test]
    fn records_failure_then_recovery_without_losing_attempt_count() {
        let state = TrayStatusState::default();

        let failed = state.record_failure("shell unavailable".to_string());
        assert!(!failed.available);
        assert_eq!(failed.attempts, 1);
        assert_eq!(failed.last_error.as_deref(), Some("shell unavailable"));

        let recovered = state.record_success();
        assert!(recovered.available);
        assert_eq!(recovered.attempts, 2);
        assert_eq!(recovered.last_error, None);
    }

    #[test]
    fn observing_an_existing_tray_does_not_count_as_an_attempt() {
        let state = TrayStatusState::default();
        state.record_failure("first failure".to_string());

        let present = state.record_present();
        assert!(present.available);
        assert_eq!(present.attempts, 1);
        assert_eq!(present.last_error, None);
    }
}
