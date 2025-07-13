use crate::RecordingState;
use std::fmt;

#[derive(Debug, Clone)]
pub struct StateTransitionError {
    from: RecordingState,
    to: RecordingState,
    message: String,
}

impl fmt::Display for StateTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Invalid state transition from {:?} to {:?}: {}",
            self.from, self.to, self.message
        )
    }
}

impl std::error::Error for StateTransitionError {}

/// State machine for recording states with validation
pub struct RecordingStateMachine {
    current_state: RecordingState,
}

impl RecordingStateMachine {
    pub fn new() -> Self {
        Self {
            current_state: RecordingState::Idle,
        }
    }

    pub fn current(&self) -> RecordingState {
        self.current_state
    }

    /// Validate and perform state transition
    pub fn transition_to(&mut self, new_state: RecordingState) -> Result<(), StateTransitionError> {
        log::info!("[FLOW] Attempting state transition: {:?} -> {:?}", self.current_state, new_state);
        
        if self.is_valid_transition(self.current_state, new_state) {
            log::info!("[FLOW] State transition VALID: {:?} -> {:?}", self.current_state, new_state);
            let old_state = self.current_state;
            self.current_state = new_state;
            
            // Log warnings for potentially problematic transitions
            match (old_state, new_state) {
                (RecordingState::Transcribing, RecordingState::Idle) => {
                    log::info!("[FLOW] Completed transcription flow, now idle");
                }
                (RecordingState::Error, _) => {
                    log::warn!("[FLOW] Recovering from error state to {:?}", new_state);
                }
                (_, RecordingState::Error) => {
                    log::error!("[FLOW] Entered error state from {:?}", old_state);
                }
                _ => {}
            }
            
            Ok(())
        } else {
            log::error!("[FLOW] State transition INVALID: {:?} -> {:?}", self.current_state, new_state);
            Err(StateTransitionError {
                from: self.current_state,
                to: new_state,
                message: format!("Transition not allowed by state machine rules"),
            })
        }
    }

    /// Check if a state transition is valid
    fn is_valid_transition(&self, from: RecordingState, to: RecordingState) -> bool {
        match (from, to) {
            // From Idle
            (RecordingState::Idle, RecordingState::Starting) => true,
            (RecordingState::Idle, RecordingState::Error) => true,

            // From Starting
            (RecordingState::Starting, RecordingState::Recording) => true,
            (RecordingState::Starting, RecordingState::Error) => true,
            (RecordingState::Starting, RecordingState::Idle) => true, // Cancelled

            // From Recording
            (RecordingState::Recording, RecordingState::Stopping) => true,
            (RecordingState::Recording, RecordingState::Error) => true,

            // From Stopping
            (RecordingState::Stopping, RecordingState::Transcribing) => true,
            (RecordingState::Stopping, RecordingState::Error) => true,
            (RecordingState::Stopping, RecordingState::Idle) => true, // Cancelled

            // From Transcribing
            (RecordingState::Transcribing, RecordingState::Idle) => true, // Success
            (RecordingState::Transcribing, RecordingState::Error) => true,

            // From Error
            (RecordingState::Error, RecordingState::Idle) => true, // Reset

            // Same state transitions (no-op)
            (a, b) if a == b => true,

            // All other transitions are invalid
            _ => false,
        }
    }

    /// Check if recording can be started
    pub fn can_start_recording(&self) -> bool {
        matches!(self.current_state, RecordingState::Idle)
    }

    /// Check if recording can be stopped
    pub fn can_stop_recording(&self) -> bool {
        matches!(self.current_state, RecordingState::Recording)
    }

    /// Check if transcription can be started
    pub fn can_start_transcription(&self) -> bool {
        matches!(self.current_state, RecordingState::Stopping)
    }

    /// Reset to idle state (useful for error recovery)
    pub fn reset(&mut self) {
        log::info!("Resetting state machine to Idle from {:?}", self.current_state);
        self.current_state = RecordingState::Idle;
    }
    
    /// Force set the current state without validation (use with caution)
    pub(crate) fn force_state(&mut self, state: RecordingState) {
        log::warn!("[FLOW] FORCE setting state from {:?} to {:?} (bypassing validation)", self.current_state, state);
        
        // Log if this would have been an invalid transition
        if !self.is_valid_transition(self.current_state, state) {
            log::error!("[FLOW] WARNING: Forced transition {:?} -> {:?} would normally be INVALID", 
                      self.current_state, state);
        }
        
        self.current_state = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        let mut sm = RecordingStateMachine::new();
        
        // Valid flow: Idle -> Starting -> Recording -> Stopping -> Transcribing -> Idle
        assert!(sm.transition_to(RecordingState::Starting).is_ok());
        assert_eq!(sm.current(), RecordingState::Starting);
        
        assert!(sm.transition_to(RecordingState::Recording).is_ok());
        assert_eq!(sm.current(), RecordingState::Recording);
        
        assert!(sm.transition_to(RecordingState::Stopping).is_ok());
        assert_eq!(sm.current(), RecordingState::Stopping);
        
        assert!(sm.transition_to(RecordingState::Transcribing).is_ok());
        assert_eq!(sm.current(), RecordingState::Transcribing);
        
        assert!(sm.transition_to(RecordingState::Idle).is_ok());
        assert_eq!(sm.current(), RecordingState::Idle);
    }

    #[test]
    fn test_invalid_transitions() {
        let mut sm = RecordingStateMachine::new();
        
        // Cannot go directly from Idle to Recording
        assert!(sm.transition_to(RecordingState::Recording).is_err());
        
        // Cannot go from Idle to Stopping
        assert!(sm.transition_to(RecordingState::Stopping).is_err());
        
        // Start recording properly
        sm.transition_to(RecordingState::Starting).unwrap();
        sm.transition_to(RecordingState::Recording).unwrap();
        
        // Cannot go from Recording to Idle directly
        assert!(sm.transition_to(RecordingState::Idle).is_err());
    }

    #[test]
    fn test_error_recovery() {
        let mut sm = RecordingStateMachine::new();
        
        // Any state can transition to Error
        sm.transition_to(RecordingState::Starting).unwrap();
        assert!(sm.transition_to(RecordingState::Error).is_ok());
        
        // Error can only transition to Idle
        assert!(sm.transition_to(RecordingState::Recording).is_err());
        assert!(sm.transition_to(RecordingState::Idle).is_ok());
    }

    #[test]
    fn test_state_checks() {
        let mut sm = RecordingStateMachine::new();
        
        assert!(sm.can_start_recording());
        assert!(!sm.can_stop_recording());
        assert!(!sm.can_start_transcription());
        
        sm.transition_to(RecordingState::Starting).unwrap();
        sm.transition_to(RecordingState::Recording).unwrap();
        
        assert!(!sm.can_start_recording());
        assert!(sm.can_stop_recording());
        assert!(!sm.can_start_transcription());
        
        sm.transition_to(RecordingState::Stopping).unwrap();
        
        assert!(!sm.can_start_recording());
        assert!(!sm.can_stop_recording());
        assert!(sm.can_start_transcription());
    }
}