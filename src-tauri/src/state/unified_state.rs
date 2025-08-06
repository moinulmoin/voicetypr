use crate::state_machine::RecordingStateMachine;
use crate::RecordingState;
use std::sync::{Arc, Mutex, MutexGuard};

/// A unified state that combines the state machine and current state
/// This ensures they are always in sync
#[derive(Clone)]
pub struct UnifiedRecordingState {
    inner: Arc<Mutex<UnifiedStateInner>>,
}

struct UnifiedStateInner {
    machine: RecordingStateMachine,
    current: RecordingState,
}

impl UnifiedRecordingState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(UnifiedStateInner {
                machine: RecordingStateMachine::new(),
                current: RecordingState::Idle,
            })),
        }
    }

    /// Transition to a new state atomically
    pub fn transition_to(&self, new_state: RecordingState) -> Result<(), String> {
        let mut guard = self.lock_or_recover()?;

        // Validate transition
        guard
            .machine
            .transition_to(new_state)
            .map_err(|e| e.to_string())?;

        // Update current state only if validation passed
        guard.current = new_state;

        Ok(())
    }

    /// Get current state
    pub fn current(&self) -> RecordingState {
        match self.inner.lock() {
            Ok(guard) => guard.current,
            Err(poisoned) => {
                // If mutex is poisoned, recover and return state
                let guard = poisoned.into_inner();
                guard.current
            }
        }
    }

    /// Reset to initial state
    pub fn reset(&self) -> Result<(), String> {
        let mut guard = self.lock_or_recover()?;
        guard.machine.reset();
        guard.current = RecordingState::Idle;
        Ok(())
    }

    /// Force set state (use with caution, bypasses validation)
    pub fn force_set(&self, state: RecordingState) -> Result<(), String> {
        let mut guard = self.lock_or_recover()?;
        // Force both the machine and current state to the target state
        guard.machine.force_state(state);
        guard.current = state;
        Ok(())
    }

    /// Atomically transition with custom logic based on current state
    /// This prevents race conditions by holding the lock during the entire operation
    pub fn transition_with_fallback<F>(
        &self,
        new_state: RecordingState,
        fallback: F,
    ) -> Result<RecordingState, String>
    where
        F: FnOnce(RecordingState) -> Option<RecordingState>,
    {
        let mut guard = self.lock_or_recover()?;
        let current = guard.current;

        // First try normal transition
        if guard.machine.transition_to(new_state).is_ok() {
            guard.current = new_state;
            return Ok(new_state);
        }

        // If normal transition failed, check if we should force a different state
        if let Some(force_state) = fallback(current) {
            guard.machine.force_state(force_state);
            guard.current = force_state;
            Ok(force_state)
        } else {
            Err(format!(
                "Cannot transition from {:?} to {:?}",
                current, new_state
            ))
        }
    }

    /// Lock the state, recovering from poison if necessary
    fn lock_or_recover(&self) -> Result<MutexGuard<'_, UnifiedStateInner>, String> {
        match self.inner.lock() {
            Ok(guard) => Ok(guard),
            Err(poisoned) => {
                log::warn!("Recovering from poisoned mutex in UnifiedRecordingState");
                Ok(poisoned.into_inner())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_state_transitions() {
        let state = UnifiedRecordingState::new();

        // Valid transition
        assert!(state.transition_to(RecordingState::Starting).is_ok());
        assert_eq!(state.current(), RecordingState::Starting);

        // Invalid transition
        assert!(state.transition_to(RecordingState::Stopping).is_err());
        assert_eq!(state.current(), RecordingState::Starting); // State unchanged

        // Valid transition
        assert!(state.transition_to(RecordingState::Recording).is_ok());
        assert_eq!(state.current(), RecordingState::Recording);
    }

    #[test]
    fn test_unified_state_reset() {
        let state = UnifiedRecordingState::new();

        // Valid transition path: Idle -> Starting -> Recording
        state.transition_to(RecordingState::Starting).unwrap();
        state.transition_to(RecordingState::Recording).unwrap();
        state.reset().unwrap();

        assert_eq!(state.current(), RecordingState::Idle);

        // Can transition from idle again
        assert!(state.transition_to(RecordingState::Starting).is_ok());
    }

    #[test]
    fn test_unified_state_force_set() {
        let state = UnifiedRecordingState::new();

        // Force invalid transition (normally can't go from Idle to Stopping)
        state.force_set(RecordingState::Stopping).unwrap();
        assert_eq!(state.current(), RecordingState::Stopping);

        // After force set to Stopping, we can transition to Transcribing
        assert!(state.transition_to(RecordingState::Transcribing).is_ok());
        assert_eq!(state.current(), RecordingState::Transcribing);

        // Force set to an invalid state again
        state.force_set(RecordingState::Recording).unwrap();
        assert_eq!(state.current(), RecordingState::Recording);
    }
}
