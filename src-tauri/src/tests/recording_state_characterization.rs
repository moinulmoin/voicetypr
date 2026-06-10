use crate::state::app_state::AppState;
use crate::state::unified_state::UnifiedRecordingState;
use crate::state_machine::RecordingStateMachine;
use crate::RecordingState;
use std::sync::atomic::Ordering;

fn all_recording_states() -> [RecordingState; 6] {
    [
        RecordingState::Idle,
        RecordingState::Starting,
        RecordingState::Recording,
        RecordingState::Stopping,
        RecordingState::Transcribing,
        RecordingState::Error,
    ]
}

fn expected_transition_validity(from: RecordingState, to: RecordingState) -> bool {
    from == to
        || matches!(
            (from, to),
            (RecordingState::Idle, RecordingState::Starting)
                | (RecordingState::Idle, RecordingState::Error)
                | (RecordingState::Starting, RecordingState::Recording)
                | (RecordingState::Starting, RecordingState::Error)
                | (RecordingState::Starting, RecordingState::Idle)
                | (RecordingState::Recording, RecordingState::Stopping)
                | (RecordingState::Recording, RecordingState::Error)
                | (RecordingState::Stopping, RecordingState::Transcribing)
                | (RecordingState::Stopping, RecordingState::Error)
                | (RecordingState::Stopping, RecordingState::Idle)
                | (RecordingState::Transcribing, RecordingState::Idle)
                | (RecordingState::Transcribing, RecordingState::Error)
                | (RecordingState::Error, RecordingState::Idle)
        )
}

#[test]
fn recording_state_machine_matches_full_transition_validity_table() {
    for from in all_recording_states() {
        for to in all_recording_states() {
            let mut machine = RecordingStateMachine::new();
            machine.force_state(from);

            let result = machine.transition_to(to);
            let expected_valid = expected_transition_validity(from, to);

            if expected_valid {
                assert!(
                    result.is_ok(),
                    "transition {from:?} -> {to:?} should be valid"
                );
            } else {
                assert!(
                    result.is_err(),
                    "transition {from:?} -> {to:?} should be invalid"
                );
            }
            assert_eq!(
                machine.current(),
                if expected_valid { to } else { from },
                "transition {from:?} -> {to:?} left machine in unexpected state"
            );
        }
    }
}

#[test]
fn recording_state_machine_reset_returns_every_state_to_idle() {
    for state in all_recording_states() {
        let mut machine = RecordingStateMachine::new();
        machine.force_state(state);

        machine.reset();

        assert_eq!(machine.current(), RecordingState::Idle);
    }
}

#[test]
fn recording_state_machine_force_state_bypasses_transition_validation() {
    let mut machine = RecordingStateMachine::new();

    assert!(machine.transition_to(RecordingState::Recording).is_err());
    assert_eq!(machine.current(), RecordingState::Idle);

    machine.force_state(RecordingState::Recording);

    assert_eq!(machine.current(), RecordingState::Recording);
}

#[test]
fn transition_with_fallback_uses_primary_transition_when_valid() {
    let state = UnifiedRecordingState::new();

    let result = state
        .transition_with_fallback(RecordingState::Starting, |_| {
            panic!("fallback must not run for a valid primary transition")
        })
        .unwrap();

    assert_eq!(result, RecordingState::Starting);
    assert_eq!(state.current(), RecordingState::Starting);
}

#[test]
fn transition_with_fallback_invokes_fallback_for_invalid_transition() {
    let state = UnifiedRecordingState::new();

    let result = state
        .transition_with_fallback(RecordingState::Recording, |current| {
            assert_eq!(current, RecordingState::Idle);
            Some(RecordingState::Error)
        })
        .unwrap();

    assert_eq!(result, RecordingState::Error);
    assert_eq!(state.current(), RecordingState::Error);
}

#[test]
fn transition_with_fallback_none_errors_and_keeps_state() {
    let state = UnifiedRecordingState::new();

    let result = state.transition_with_fallback(RecordingState::Recording, |current| {
        assert_eq!(current, RecordingState::Idle);
        None
    });

    assert!(result.is_err());
    assert_eq!(state.current(), RecordingState::Idle);
}

#[test]
fn unified_force_set_round_trips_through_current() {
    let state = UnifiedRecordingState::new();

    for forced_state in all_recording_states() {
        state.force_set(forced_state).unwrap();
        assert_eq!(state.current(), forced_state);
    }
}

#[test]
fn request_cancellation_marks_cancellation_requested() {
    let state = AppState::new();

    state.request_cancellation();

    assert!(state.is_cancellation_requested());
}

#[test]
fn clear_cancellation_clears_flag_for_new_operation() {
    let state = AppState::new();
    state.request_cancellation();

    // Flag-level contract: clear_cancellation still clears stale cancellation
    // for a new operation. The start_recording race is fixed by clearing only
    // before Starting and checking the flag at commit before entering Recording.
    state.clear_cancellation();

    assert!(!state.is_cancellation_requested());
}

#[test]
fn pending_stop_after_start_swap_consumes_and_resets_flag() {
    let state = AppState::new();

    state.pending_stop_after_start.store(true, Ordering::SeqCst);

    assert!(state.pending_stop_after_start.swap(false, Ordering::SeqCst));
    assert!(!state.pending_stop_after_start.load(Ordering::SeqCst));
    assert!(!state.pending_stop_after_start.swap(false, Ordering::SeqCst));
}
